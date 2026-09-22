/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

//! Tests for the JS-to-Rust call bodies that `macros.cpp` writes.

use super::*;

fn render(udl: &str) -> String {
    let mut ci = ComponentInterface::from_webidl(udl, "uniffi_test").expect("parse udl");
    ci.derive_ffi_funcs().expect("derive ffi funcs");
    let module = ModuleMetadata::new(ci.namespace());
    generate_cpp(&ci, &Config::default(), &module).expect("generate cpp")
}

/// The generated source from a function's definition to its closing brace.
fn fn_body<'a>(rendered: &'a str, name: &str) -> &'a str {
    let marker = format!("jsi::Value NativeUniffiTest::cpp_uniffi_uniffi_test_fn_func_{name}(");
    let start = rendered
        .find(&marker)
        .unwrap_or_else(|| panic!("function {name} not found"));
    let body = &rendered[start..];
    &body[..body.find("\n}\n").expect("function end not found")]
}

#[test]
fn borrowed_bytes_are_checked_first_and_read_in_the_call() {
    let rendered = render(
        r#"
        namespace uniffi_test {
            void mix(u32 first, [ByRef] bytes borrowed, bytes owned, [ByRef] bytes last);
        };
        "#,
    );
    let body = fn_body(&rendered, "mix");

    // Every `&[u8]` check comes before any other conversion.
    let conversions: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("::fromJs("))
        .collect();
    assert_eq!(conversions.len(), 4, "{body}");
    assert!(conversions[0].starts_with("auto arg1 = uniffi_jsi::Bridging<ForeignBytes>::fromJs("));
    assert!(conversions[1].starts_with("auto arg3 = uniffi_jsi::Bridging<ForeignBytes>::fromJs("));
    assert!(conversions[2].starts_with("auto arg0 = uniffi_jsi::Bridging<uint32_t>::fromJs("));
    assert!(conversions[3]
        .starts_with("auto arg2 = uniffi::uniffi_test::Bridging<RustBuffer>::fromJs("));

    // The pointers are read in the call, after every conversion.
    let call = body
        .rfind("uniffi_uniffi_test_fn_func_mix(")
        .expect("call not found");
    assert!(!body[call..].contains("fromJs("), "{body}");
    assert!(
        body[call..].contains(
            "arg0, \n            arg1.bytes(rt), \n            arg2, \n            arg3.bytes(rt), \n            &status\n        );"
        ),
        "{body}"
    );
}

#[test]
fn call_without_borrowed_bytes_is_unchanged() {
    let rendered = render(
        r#"
        namespace uniffi_test {
            void no_bytes(u32 value, bytes owned);
        };
        "#,
    );
    let body = fn_body(&rendered, "no_bytes");
    assert!(!body.contains("auto arg"), "{body}");
    assert!(
        body.contains(
            "uniffi_uniffi_test_fn_func_no_bytes(uniffi_jsi::Bridging<uint32_t>::fromJs(rt, callInvoker, args[0]), uniffi::uniffi_test::Bridging<RustBuffer>::fromJs(rt, callInvoker, args[1]), \n            &status\n        );"
        ),
        "{body}"
    );
}
