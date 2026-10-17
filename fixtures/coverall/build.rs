/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

fn main() {
    const FULL_UDL: &str = "src/coverall.udl";
    const JSI_UDL: &str = "src/coverall_jsi.udl";
    for path in [FULL_UDL, JSI_UDL] {
        println!("cargo:rerun-if-changed={path}");
    }

    // JSI rejects owned-object callback payloads until typed cleanup is available.
    // Keep every other declaration identical to the full fixture.
    let mut expected = std::fs::read_to_string(FULL_UDL)
        .unwrap()
        .replace("\r\n", "\n");
    for declaration in [
        "    void test_round_trip_through_foreign(Getters getters);\n",
        "    Coveralls round_trip_object(Coveralls coveralls);\n",
    ] {
        assert_eq!(expected.matches(declaration).count(), 1);
        expected = expected.replace(declaration, "");
    }
    let actual = std::fs::read_to_string(JSI_UDL)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(actual, expected);

    let udl = if cfg!(feature = "jsi-fixture") {
        JSI_UDL
    } else {
        FULL_UDL
    };
    uniffi::generate_scaffolding(udl).unwrap();
}
