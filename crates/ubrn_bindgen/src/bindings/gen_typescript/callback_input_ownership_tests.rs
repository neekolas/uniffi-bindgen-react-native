/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{api_module::TsApiModule, generate_api_code_from_ir, Config};
use crate::AbiFlavor;
use std::{
    collections::HashSet,
    io::Write,
    process::{Command, Stdio},
};
use uniffi_bindgen::pipeline::{general, initial::UniffiMetaConverter};
use uniffi_meta::{
    CallbackInterfaceMetadata, FnParamMetadata, Metadata, NamespaceMetadata, TraitMethodMetadata,
    Type,
};

fn render(flavor: AbiFlavor) -> String {
    let mut converter = UniffiMetaConverter::default();
    converter
        .add_metadata_item(Metadata::Namespace(NamespaceMetadata {
            crate_name: "callback_inputs".into(),
            name: "callback_inputs".into(),
        }))
        .unwrap();
    converter
        .add_metadata_item(Metadata::CallbackInterface(CallbackInterfaceMetadata {
            module_path: "callback_inputs".into(),
            name: "Listener".into(),
            docstring: None,
        }))
        .unwrap();
    for (index, is_async) in [false, true].into_iter().enumerate() {
        converter
            .add_metadata_item(Metadata::TraitMethod(TraitMethodMetadata {
                module_path: "callback_inputs".into(),
                trait_name: "Listener".into(),
                index: index as u32,
                name: if is_async { "later" } else { "now" }.into(),
                orig_name: None,
                is_async,
                inputs: [
                    ("first", Type::String),
                    ("second", Type::String),
                    ("number", Type::Int32),
                ]
                .into_iter()
                .map(|(name, ty)| FnParamMetadata {
                    name: name.into(),
                    ty,
                    by_ref: false,
                    optional: false,
                    default: None,
                })
                .collect(),
                return_type: None,
                throws: None,
                takes_self_by_arc: false,
                checksum: Some(0),
                docstring: None,
            }))
            .unwrap();
    }
    converter
        .add_metadata_item(Metadata::TraitMethod(TraitMethodMetadata {
            module_path: "callback_inputs".into(),
            trait_name: "Listener".into(),
            index: 2,
            name: "nested".into(),
            orig_name: None,
            is_async: false,
            inputs: vec![FnParamMetadata {
                name: "nested".into(),
                ty: Type::Map {
                    key_type: Box::new(Type::String),
                    value_type: Box::new(Type::Sequence {
                        inner_type: Box::new(Type::Bytes),
                    }),
                },
                by_ref: false,
                optional: false,
                default: None,
            }],
            return_type: None,
            throws: None,
            takes_self_by_arc: false,
            checksum: Some(0),
            docstring: None,
        }))
        .unwrap();
    let mut root = general::pipeline("react-native")
        .execute(converter.try_into_initial_ir().unwrap())
        .unwrap();
    let namespace = root.namespaces.shift_remove("callback_inputs").unwrap();
    let module = TsApiModule::from_general(
        &Config::default(),
        &namespace,
        flavor,
        vec![],
        &HashSet::new(),
    )
    .unwrap();
    generate_api_code_from_ir(module).unwrap()
}

#[test]
fn callback_input_ownership_generated_jsi_boundary() {
    // Execute actual emitted code, rather than merely checking that a finally
    // token is present. Requires Node and the repository TypeScript dependency.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            include_str!("callback_input_ownership_test.mjs"),
        ])
        .env("UBRN_TEST_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node is required for the generated callback boundary test");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(render(AbiFlavor::Jsi).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn callback_input_ownership_preserves_player_and_wasm() {
    let mut flavors = vec![AbiFlavor::Napi];
    #[cfg(feature = "wasm")]
    flavors.extend([AbiFlavor::Wasm, AbiFlavor::Wasm2]);
    for flavor in flavors.drain(..) {
        let code = render(flavor);
        assert!(!code.contains("rustbuffer_free(first)"));
        assert!(!code.contains("rustbuffer_free(second)"));
        assert!(code.contains("FfiConverterString.lift(first)"));
        assert!(code.contains("FfiConverterString.lift(second)"));
    }
}
