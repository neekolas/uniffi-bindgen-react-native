/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

//! Renders the `-ffi.ts` of each flavor from real pipeline metadata, and
//! checks that the file imports from `@ubjs/core` exactly the names that it
//! uses.

use std::collections::BTreeSet;

use uniffi_bindgen::pipeline::{general, initial::UniffiMetaConverter};
use uniffi_meta::{
    CallbackInterfaceMetadata, FnMetadata, FnParamMetadata, Metadata, NamespaceMetadata,
    ObjectImpl, ObjectMetadata, TraitMethodMetadata, Type,
};

use super::{
    ffi_module::TsFfiModule,
    ffi_module_player::{LibResolution, PlayerFfiModule},
    generate_lowlevel_code, generate_player_lowlevel_code, Config,
};
use crate::AbiFlavor;

/// The local names that a `-ffi.ts` can import from `@ubjs/core`.
const CORE_NAMES: &[&str] = &[
    "UniffiStructuralEquality",
    "RuntimeUniffiForeignFuture",
    "UniffiRustCallStatus",
    "UniffiGcObject",
    "RuntimeUniffiRustFutureContinuationCallback",
    "UniffiResult",
];

fn param(name: &str, ty: Type) -> FnParamMetadata {
    FnParamMetadata {
        name: name.into(),
        ty,
        by_ref: false,
        optional: false,
        default: None,
    }
}

fn function(crate_name: &str, name: &str, is_async: bool) -> Metadata {
    Metadata::Func(FnMetadata {
        module_path: crate_name.into(),
        name: name.into(),
        orig_name: None,
        is_async,
        inputs: vec![param("a", Type::UInt32), param("b", Type::UInt32)],
        return_type: Some(Type::UInt32),
        throws: None,
        // The pipeline needs a checksum; its value does not matter here.
        checksum: Some(0),
        docstring: None,
    })
}

fn trait_method(crate_name: &str, index: u32, name: &str, is_async: bool) -> Metadata {
    Metadata::TraitMethod(TraitMethodMetadata {
        module_path: crate_name.into(),
        trait_name: "Listener".into(),
        index,
        name: name.into(),
        orig_name: None,
        is_async,
        inputs: vec![param("value", Type::UInt32)],
        return_type: Some(Type::String),
        throws: None,
        takes_self_by_arc: false,
        // The pipeline needs a checksum; its value does not matter here.
        checksum: Some(0),
        docstring: None,
    })
}

/// A namespace with an object, a callback interface with a sync and an async
/// method (the async method makes the `ForeignFuture` structs), and an async
/// function (which makes the Rust future types).
fn full_items(crate_name: &str) -> Vec<Metadata> {
    vec![
        Metadata::Object(ObjectMetadata {
            module_path: crate_name.into(),
            name: "Obj".into(),
            orig_name: None,
            remote: false,
            imp: ObjectImpl::Struct,
            docstring: None,
        }),
        Metadata::CallbackInterface(CallbackInterfaceMetadata {
            module_path: crate_name.into(),
            name: "Listener".into(),
            docstring: None,
        }),
        trait_method(crate_name, 0, "on_event", false),
        trait_method(crate_name, 1, "on_event_later", true),
        function(crate_name, "add", false),
        function(crate_name, "add_later", true),
    ]
}

/// A namespace with one sync function only.
fn plain_items(crate_name: &str) -> Vec<Metadata> {
    vec![function(crate_name, "add", false)]
}

fn namespace(crate_name: &str, items: Vec<Metadata>) -> general::Namespace {
    let mut converter = UniffiMetaConverter::default();
    converter
        .add_metadata_item(Metadata::Namespace(NamespaceMetadata {
            crate_name: crate_name.into(),
            name: crate_name.into(),
        }))
        .unwrap();
    for item in items {
        converter.add_metadata_item(item).unwrap();
    }
    let initial = converter.try_into_initial_ir().unwrap();
    let mut root = general::pipeline("react-native").execute(initial).unwrap();
    root.namespaces.shift_remove(crate_name).unwrap()
}

/// Render the `-ffi.ts` as `cli.rs` does for this flavor.
fn render(namespace: &general::Namespace, flavor: AbiFlavor) -> String {
    let config = Config::default();
    if flavor.supports_player() {
        let module = PlayerFfiModule::from_general(
            namespace,
            &config,
            &flavor,
            namespace.crate_name.clone(),
            Some(LibResolution::Colocated),
        );
        generate_player_lowlevel_code(module).unwrap()
    } else {
        let module = TsFfiModule::from_general(namespace, &flavor, &config);
        generate_lowlevel_code(module).unwrap()
    }
}

/// Split the file into the local names that it imports from `@ubjs/core`,
/// and the rest of the file.
fn split_core_imports(code: &str) -> (BTreeSet<String>, String) {
    let mut imported = BTreeSet::new();
    let mut body = String::new();
    let mut rest = code;
    while let Some(start) = rest.find("import ") {
        let Some(len) = rest[start..].find(';') else {
            break;
        };
        let statement = &rest[start..start + len + 1];
        body.push_str(&rest[..start]);
        if statement.ends_with("from '@ubjs/core';") {
            let open = statement.find('{').unwrap();
            let close = statement.find('}').unwrap();
            for spec in statement[open + 1..close].split(',') {
                let spec = spec.trim();
                if spec.is_empty() {
                    continue;
                }
                let spec = spec.strip_prefix("type ").unwrap_or(spec);
                let local = spec.rsplit(" as ").next().unwrap().trim();
                imported.insert(local.to_string());
            }
        } else {
            body.push_str(statement);
        }
        rest = &rest[start + len + 1..];
    }
    body.push_str(rest);
    (imported, body)
}

fn core_names_used_in(body: &str) -> BTreeSet<String> {
    body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .filter(|word| CORE_NAMES.contains(word))
        .map(String::from)
        .collect()
}

/// Check that the file imports each core name that it uses, and uses each
/// core name that it imports. Returns the imported names.
fn check_imports(crate_name: &str, flavor: AbiFlavor) -> BTreeSet<String> {
    let items = if crate_name == "full" {
        full_items(crate_name)
    } else {
        plain_items(crate_name)
    };
    let code = render(&namespace(crate_name, items), flavor.clone());
    let (imported, body) = split_core_imports(&code);
    for name in &imported {
        assert!(
            CORE_NAMES.contains(&name.as_str()),
            "{flavor:?} {crate_name}: add `{name}` to CORE_NAMES:\n{code}"
        );
    }
    assert_eq!(
        imported,
        core_names_used_in(&body),
        "{flavor:?} {crate_name}: the imports from @ubjs/core (left) are not the names used (right):\n{code}"
    );
    imported
}

fn names(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn jsi_ffi_imports_what_it_uses() {
    let full = check_imports("full", AbiFlavor::Jsi);
    // The checks of the future types, and their imports, are kept.
    assert!(
        full.is_superset(&names(&[
            "UniffiStructuralEquality",
            "RuntimeUniffiRustFutureContinuationCallback",
            "UniffiRustCallStatus",
            "UniffiGcObject",
            "UniffiResult",
        ])),
        "{full:?}"
    );
    assert_eq!(
        check_imports("plain", AbiFlavor::Jsi),
        names(&["UniffiRustCallStatus"])
    );
}

#[test]
fn napi_ffi_imports_what_it_uses() {
    assert_eq!(
        check_imports("full", AbiFlavor::Napi),
        names(&["UniffiRustCallStatus", "UniffiResult"])
    );
    assert_eq!(
        check_imports("plain", AbiFlavor::Napi),
        names(&["UniffiRustCallStatus"])
    );
}

#[cfg(feature = "wasm")]
#[test]
fn wasm_ffi_imports_what_it_uses() {
    assert_eq!(
        check_imports("full", AbiFlavor::Wasm),
        names(&["UniffiRustCallStatus", "UniffiResult"])
    );
    assert!(check_imports("plain", AbiFlavor::Wasm).is_empty());
}

#[cfg(feature = "wasm")]
#[test]
fn wasm2_ffi_imports_what_it_uses() {
    assert_eq!(
        check_imports("full", AbiFlavor::Wasm2),
        names(&["UniffiRustCallStatus", "UniffiResult"])
    );
    assert_eq!(
        check_imports("plain", AbiFlavor::Wasm2),
        names(&["UniffiRustCallStatus"])
    );
}
