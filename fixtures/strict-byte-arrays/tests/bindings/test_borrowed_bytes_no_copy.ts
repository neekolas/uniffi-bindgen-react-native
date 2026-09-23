/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// Shows that JSI and napi pass a `&[u8]` argument to Rust as a pointer into
// the JS buffer, with no copy.
//
// To run:
//   cargo test -p uniffi-fixture-strict-byte-arrays -- jsi::test_borrowed
//   cargo test -p uniffi-fixture-strict-byte-arrays -- napi::test_borrowed
//
// `borrowedBytesDistance` returns how far apart Rust sees the start of its two
// arguments. Two overlapping views of one buffer are 3 bytes apart in JS. A
// copy of each view would be a separate allocation, and both copies are live
// during the call, so two copies cannot be 3 bytes apart.
//
// The wasm and wasm2 flavours copy the bytes into wasm memory, so this test
// does not run there.
import { borrowedBytesDistance } from "@/generated/uniffi_strict_byte_arrays";
import { test } from "@/asserts";
import "@/polyfills";

test("a borrowed argument points into the JS buffer", (t) => {
  const backing = new Uint8Array(16).fill(1);
  t.assertEqual(3, Number(borrowedBytesDistance(backing, backing.subarray(3))));
  t.assertEqual(
    -5,
    Number(borrowedBytesDistance(backing.subarray(7, 9), backing.subarray(2))),
  );
});
