/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

/// Top-level IR node for `{namespace}-ffi.ts`.
///
/// All names and types are fully resolved strings; the template
/// iterates and interpolates without logic.
pub(crate) struct TsFfiModule {
    pub module_name: String,
    pub strict_type_checking: bool,
    pub is_jsi: bool,
    pub functions: Vec<FfiFunctionDecl>,
    /// Interleaved in the order from `general::Namespace::ffi_definitions`.
    pub definitions: Vec<FfiDefinitionDecl>,
    pub has_continuation_callback: bool,
    pub has_foreign_future: bool,
    pub core_types: CoreTypeUses,
}

/// The `@ubjs/core` types that the declarations of a `-ffi.ts` name.
/// The templates import a type only when it is named.
#[derive(Default)]
pub(crate) struct CoreTypeUses {
    pub rust_call_status: bool,
    pub gc_object: bool,
    pub result: bool,
}

impl CoreTypeUses {
    pub(crate) fn of(functions: &[FfiFunctionDecl], definitions: &[FfiDefinitionDecl]) -> Self {
        let mut uses = Self::default();
        for func in functions {
            func.arguments.iter().for_each(|a| uses.add(&a.type_name));
            func.return_type.iter().for_each(|t| uses.add(t));
        }
        for def in definitions {
            match def {
                FfiDefinitionDecl::Callback(cb) => {
                    cb.arguments.iter().for_each(|a| uses.add(&a.type_name));
                    uses.add(&cb.return_type);
                }
                FfiDefinitionDecl::Struct(s) => {
                    s.fields.iter().for_each(|f| uses.add(&f.type_name))
                }
            }
        }
        uses
    }

    /// Check each identifier in the type, so that a core type inside
    /// another type, for example `Array<UniffiGcObject>`, is also found.
    fn add(&mut self, type_name: &str) {
        let is_ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
        for name in type_name.split(|c: char| !is_ident_char(c)) {
            match name {
                "UniffiRustCallStatus" => self.rust_call_status = true,
                "UniffiGcObject" => self.gc_object = true,
                "UniffiResult" => self.result = true,
                _ => {}
            }
        }
    }

    pub(crate) fn any(&self) -> bool {
        self.rust_call_status || self.gc_object || self.result
    }
}

pub(crate) enum FfiExportedName {
    Callback(String),
    Struct(String),
}

impl FfiExportedName {
    pub fn name(&self) -> &str {
        match self {
            Self::Callback(n) | Self::Struct(n) => n,
        }
    }
}

impl TsFfiModule {
    pub(crate) fn exported_names(&self) -> Vec<FfiExportedName> {
        self.definitions
            .iter()
            .filter_map(|def| match def {
                FfiDefinitionDecl::Callback(cb) if cb.exported => {
                    Some(FfiExportedName::Callback(cb.name.clone()))
                }
                FfiDefinitionDecl::Struct(s) if s.exported => {
                    Some(FfiExportedName::Struct(s.name.clone()))
                }
                _ => None,
            })
            .collect()
    }
}

pub(crate) struct FfiFunctionDecl {
    /// Includes the `ubrn_` prefix.
    pub name: String,
    pub arguments: Vec<FfiArgDecl>,
    pub return_type: Option<String>,
}

pub(crate) struct FfiArgDecl {
    /// camelCase, except `"uniffi_out_err"` which keeps its uniffi convention name.
    pub name: String,
    pub type_name: String,
}

pub(crate) enum FfiDefinitionDecl {
    Callback(FfiCallbackDecl),
    Struct(FfiStructDecl),
}

pub(crate) struct FfiCallbackDecl {
    pub exported: bool,
    pub name: String,
    /// Excludes output parameters (`uniffi_out_return`, `uniffi_out_dropped_callback`).
    pub arguments: Vec<FfiArgDecl>,
    /// `"void"` for non-blocking, `"UniffiResult<void>"` for blocking
    /// with no return value, or the FFI type name otherwise.
    pub return_type: String,
}

pub(crate) struct FfiStructDecl {
    pub exported: bool,
    pub name: String,
    pub fields: Vec<FfiFieldDecl>,
}

pub(crate) struct FfiFieldDecl {
    pub name: String,
    pub type_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(type_name: &str) -> FfiArgDecl {
        FfiArgDecl {
            name: "a".into(),
            type_name: type_name.into(),
        }
    }

    #[test]
    fn core_type_uses_finds_only_the_named_types() {
        let none = CoreTypeUses::of(&[], &[]);
        assert!(!none.any());

        let functions = [FfiFunctionDecl {
            name: "f".into(),
            arguments: vec![arg("bigint"), arg("UniffiRustCallStatus")],
            return_type: Some("number".into()),
        }];
        let uses = CoreTypeUses::of(&functions, &[]);
        assert!(uses.rust_call_status && !uses.gc_object && !uses.result);

        let functions = [FfiFunctionDecl {
            name: "f".into(),
            arguments: vec![],
            return_type: Some("UniffiGcObject".into()),
        }];
        let uses = CoreTypeUses::of(&functions, &[]);
        assert!(!uses.rust_call_status && uses.gc_object && !uses.result);

        let definitions = [FfiDefinitionDecl::Callback(FfiCallbackDecl {
            exported: false,
            name: "UniffiCallbackInterfaceClone".into(),
            arguments: vec![arg("bigint")],
            return_type: "UniffiResult<void>".into(),
        })];
        let uses = CoreTypeUses::of(&[], &definitions);
        assert!(!uses.rust_call_status && !uses.gc_object && uses.result);

        let definitions = [FfiDefinitionDecl::Struct(FfiStructDecl {
            exported: true,
            name: "UniffiForeignFutureResultU8".into(),
            fields: vec![FfiFieldDecl {
                name: "call_status".into(),
                type_name: "UniffiRustCallStatus".into(),
            }],
        })];
        let uses = CoreTypeUses::of(&[], &definitions);
        assert!(uses.rust_call_status && !uses.gc_object && !uses.result);
    }

    #[test]
    fn core_type_uses_finds_types_inside_other_types() {
        let uses_of = |type_name: &str| {
            let functions = [FfiFunctionDecl {
                name: "f".into(),
                arguments: vec![],
                return_type: Some(type_name.into()),
            }];
            let uses = CoreTypeUses::of(&functions, &[]);
            (uses.rust_call_status, uses.gc_object, uses.result)
        };
        assert_eq!(uses_of("Array<UniffiRustCallStatus>"), (true, false, false));
        assert_eq!(uses_of("UniffiGcObject | undefined"), (false, true, false));
        assert_eq!(uses_of("Promise<UniffiResult<void>>"), (false, false, true));
        assert_eq!(uses_of("UniffiResult<UniffiGcObject>"), (false, true, true));
        // A longer name that starts with a core name is not a use.
        assert_eq!(uses_of("UniffiResultU8"), (false, false, false));
    }
}
