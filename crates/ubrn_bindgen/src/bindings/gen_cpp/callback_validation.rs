/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::BTreeSet;

use anyhow::{anyhow, bail, Context, Result};
use uniffi_bindgen::interface::{AsType, Method, Type};
use uniffi_bindgen::ComponentInterface;

/// Native callback cancellation can discard plain serialized values, but cannot
/// release handles embedded in them without a generated typed decoder. Keep this
/// restriction in the JSI generator; other ABI backends have different cleanup.
pub(super) fn validate_callback_discard(ci: &ComponentInterface) -> Result<()> {
    for callback in ci.callback_interface_definitions() {
        for method in callback.methods() {
            validate_method(ci, callback.name(), method)?;
        }
    }
    for object in ci
        .object_definitions()
        .iter()
        .filter(|o| o.has_callback_interface())
    {
        for method in object.methods() {
            validate_method(ci, object.name(), method)?;
        }
    }
    Ok(())
}

fn validate_method(ci: &ComponentInterface, owner: &str, method: &Method) -> Result<()> {
    let check = |ty: &Type, position: String| {
        validate_type(ci, ty, &mut BTreeSet::new()).with_context(|| {
            format!(
                "JSI callback `{}.{}` {} cannot be discarded safely during runtime teardown; \
                 typed native handle cleanup is required",
                owner,
                method.name(),
                position,
            )
        })
    };
    for argument in method.arguments() {
        check(
            &argument.as_type(),
            format!("argument `{}`", argument.name()),
        )?;
    }
    if let Some(ty) = method.return_type() {
        check(ty, "return value".into())?;
    }
    if let Some(ty) = method.throws_type() {
        check(ty, "error value".into())?;
    }
    Ok(())
}

fn component_for<'a>(
    ci: &'a ComponentInterface,
    module_path: &str,
) -> Result<&'a ComponentInterface> {
    let crate_name = module_path.split("::").next().unwrap_or(module_path);
    if crate_name == ci.crate_name() {
        Ok(ci)
    } else {
        ci.find_component_interface(crate_name).ok_or_else(|| {
            anyhow!(
                "cannot inspect external callback value from `{module_path}` without its schema"
            )
        })
    }
}

fn validate_type(ci: &ComponentInterface, ty: &Type, seen: &mut BTreeSet<Type>) -> Result<()> {
    if !seen.insert(ty.clone()) {
        return Ok(());
    }
    match ty {
        Type::Object {
            module_path, name, ..
        } => {
            bail!("value contains Rust object handle `{module_path}::{name}`")
        }
        Type::CallbackInterface { module_path, name } => {
            bail!("value contains callback handle `{module_path}::{name}`")
        }
        Type::Box { inner_type }
        | Type::Optional { inner_type }
        | Type::Sequence { inner_type }
        | Type::Set { inner_type }
        | Type::Custom {
            builtin: inner_type,
            ..
        } => validate_type(ci, inner_type, seen),
        Type::Map {
            key_type,
            value_type,
        } => {
            validate_type(ci, key_type, seen)?;
            validate_type(ci, value_type, seen)
        }
        Type::Record { module_path, name } => {
            let record = component_for(ci, module_path)?
                .get_record_definition(name)
                .ok_or_else(|| anyhow!("missing callback record schema `{module_path}::{name}`"))?;
            for field in record.fields() {
                validate_type(ci, &field.as_type(), seen)?;
            }
            Ok(())
        }
        Type::Enum { module_path, name } => {
            let enumeration = component_for(ci, module_path)?
                .get_enum_definition(name)
                .ok_or_else(|| anyhow!("missing callback enum schema `{module_path}::{name}`"))?;
            for variant in enumeration.variants() {
                for field in variant.fields() {
                    validate_type(ci, &field.as_type(), seen)?;
                }
            }
            Ok(())
        }
        Type::UInt8
        | Type::Int8
        | Type::UInt16
        | Type::Int16
        | Type::UInt32
        | Type::Int32
        | Type::UInt64
        | Type::Int64
        | Type::Float32
        | Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Bytes
        | Type::Timestamp
        | Type::Duration => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::gen_cpp::{generate_cpp, Config};
    use crate::bindings::metadata::ModuleMetadata;
    use uniffi_meta::ObjectImpl;

    fn schema(body: &str) -> ComponentInterface {
        ComponentInterface::from_webidl(&format!("namespace guard_test {{}}; {body}"), "guard_test")
            .expect("valid UDL fixture")
    }

    fn generated_error(body: &str) -> String {
        let mut ci = schema(body);
        ci.derive_ffi_funcs().unwrap();
        let module = ModuleMetadata::new(ci.namespace());
        format!(
            "{:#}",
            generate_cpp(&ci, &Config::default(), &module).unwrap_err()
        )
    }

    #[test]
    fn jsi_generation_rejects_nested_callback_input_handles() {
        let error = generated_error(
            r#"
            interface NativeObject {};
            dictionary Holder { sequence<NativeObject?> values; };
            [Enum] interface Event { Value(record<string, Holder> payload); };
            callback interface Observer { void changed(Event event); };
        "#,
        );
        assert!(
            error.contains("Observer.changed") && error.contains("argument `event`"),
            "{error}"
        );
        assert!(
            error.contains("Rust object handle `guard_test::NativeObject`"),
            "{error}"
        );
    }

    #[test]
    fn jsi_generation_rejects_callback_results_and_errors() {
        for (method, position) in [
            ("Holder changed();", "return value"),
            ("[Throws=Failure] void changed();", "error value"),
        ] {
            let error = generated_error(&format!(
                r#"
                interface NativeObject {{}};
                dictionary Holder {{ NativeObject value; }};
                [Error] interface Failure {{ Wrapped(Holder value); }};
                callback interface Observer {{ {method} }};
            "#
            ));
            assert!(
                error.contains(position) && error.contains("Rust object handle"),
                "{error}"
            );
        }
    }

    #[test]
    fn jsi_generation_checks_foreign_implemented_traits() {
        let error = generated_error(
            r#"
            interface NativeObject {};
            [Trait, WithForeign] interface Observer { void changed(NativeObject value); };
        "#,
        );
        assert!(
            error.contains("Observer.changed") && error.contains("Rust object handle"),
            "{error}"
        );
    }

    #[test]
    fn callback_guard_checks_custom_box_set_and_map_keys() {
        let ci = schema("");
        let object = Type::Object {
            module_path: "external".into(),
            name: "NativeObject".into(),
            imp: ObjectImpl::Struct,
        };
        let types = [
            Type::Box {
                inner_type: Box::new(object.clone()),
            },
            Type::Set {
                inner_type: Box::new(object.clone()),
            },
            Type::Custom {
                module_path: "guard_test".into(),
                name: "Wrapped".into(),
                builtin: Box::new(object.clone()),
            },
            Type::Map {
                key_type: Box::new(object),
                value_type: Box::new(Type::String),
            },
        ];
        for ty in types {
            let error = validate_type(&ci, &ty, &mut BTreeSet::new())
                .unwrap_err()
                .to_string();
            assert!(error.contains("Rust object handle"), "{error}");
        }
    }

    #[test]
    fn callback_guard_resolves_external_records_and_rejects_unknown_schemas() {
        let mut ci = schema("dictionary Holder { string safe; };");
        let external = ComponentInterface::from_webidl(
            "namespace external {}; interface NativeObject {}; dictionary Holder { NativeObject value; };", "external"
        ).unwrap();
        ci.set_all_component_interfaces(vec![external]);
        let ty = Type::Record {
            module_path: "external::nested".into(),
            name: "Holder".into(),
        };
        assert!(validate_type(&ci, &ty, &mut BTreeSet::new())
            .unwrap_err()
            .to_string()
            .contains("Rust object handle"));
        ci.set_all_component_interfaces(vec![]);
        assert!(validate_type(&ci, &ty, &mut BTreeSet::new())
            .unwrap_err()
            .to_string()
            .contains("without its schema"));
    }

    #[test]
    fn callback_guard_accepts_recursive_value_records_and_ignores_regular_object_methods() {
        let ci = schema(
            r#"
            dictionary Tree { sequence<Tree> children; bytes data; };
            interface NativeObject { NativeObject copy(); };
            callback interface Observer { Tree changed(Tree value); };
        "#,
        );
        validate_callback_discard(&ci).unwrap();
    }
}
