/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
ubrn_macros::build_foreign_language_testcases! {
    "tests/bindings/test_coverall.ts" => [Wasm, Napi, Wasm2],
}

mod jsi {
    #[test]
    fn test_coverall() {
        ubrn_fixture_testing::jsi::run_test_with_features(
            env!("CARGO_PKG_NAME"),
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/bindings/test_coverall_jsi.ts"
            ),
            env!("CARGO_TARGET_TMPDIR"),
            &["jsi-fixture"],
        );
    }
}
