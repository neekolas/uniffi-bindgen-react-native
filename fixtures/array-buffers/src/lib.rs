/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

// Sibling to the `strict-byte-arrays` fixture: that one forces `Vec<u8>` to
// be emitted as `Uint8Array` (`strictByteArrays = true`); this one leaves the
// default TypeScript config in place, so `Vec<u8>` is emitted as the plain
// `ArrayBuffer` that most fixtures (and the FfiConverterArrayBuffer cursor
// path) use.

// The wasm2 player calls `__ubrn_alloc`/`__ubrn_free`, which live in
// `uniffi-runtime-wasm`; naming it here keeps its exports in the module.
#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

#[uniffi::export]
/// This makes the byte array in rust, and the test in JS will compare it there.
///
/// This eliminates the possibility of two symmetrical bugs in each of the lift and
/// lower for the roundtrip tests– this just uses the Rust lower, and the Typescript
/// lift.
pub fn well_known_bytes() -> Vec<u8> {
    vec![1, 2, 3, 255]
}

#[uniffi::export]
/// This uses a byte array to pass an argument and return, so it uses lift/lower methods.
pub fn identity_bytes(bytes: Vec<u8>) -> Vec<u8> {
    bytes
}

#[uniffi::export]
/// This uses an option to force the lift/lower machinery to use read and write
/// directly from the Option lift and lower, not from the byte array lift and lower.
pub fn identity_bytes_forced_read(bytes: Option<Vec<u8>>) -> Option<Vec<u8>> {
    bytes
}

// --- `&[u8]` arguments -------------------------------------------------------
//
// Rust borrows these bytes for one call. JSI and napi pass a pointer into the
// JS buffer; wasm and wasm2 pass a copy in wasm memory.

#[uniffi::export]
pub fn borrowed_bytes_checksum(bytes: &[u8]) -> u32 {
    bytes.iter().map(|byte| u32::from(*byte)).sum()
}

/// Declared in `array_buffers.udl` as `[ByRef] bytes`.
pub fn udl_borrowed_bytes_checksum(bytes: &[u8]) -> u32 {
    borrowed_bytes_checksum(bytes)
}

#[uniffi::export]
pub fn copy_borrowed_bytes(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

#[uniffi::export]
pub fn concat_borrowed_bytes(first: &[u8], second: &[u8]) -> Vec<u8> {
    [first, second].concat()
}

#[uniffi::export]
pub fn mix_owned_and_borrowed_bytes(first: &[u8], owned: Vec<u8>, last: &[u8]) -> Vec<u8> {
    [first, owned.as_slice(), last].concat()
}

#[derive(uniffi::Object)]
pub struct BorrowedBytes {
    prefix: Vec<u8>,
}

#[uniffi::export]
impl BorrowedBytes {
    #[uniffi::constructor]
    pub fn new(prefix: &[u8]) -> Self {
        Self {
            prefix: prefix.to_vec(),
        }
    }

    pub fn append(&self, bytes: &[u8]) -> Vec<u8> {
        [self.prefix.as_slice(), bytes].concat()
    }
}

#[uniffi::export(callback_interface)]
pub trait BytesSink: Send + Sync {
    fn receive(&self, bytes: Vec<u8>);
}

/// A `&[u8]` argument together with a callback interface argument. The
/// callback runs JS while Rust still borrows the bytes.
#[uniffi::export]
pub fn send_borrowed_bytes(bytes: &[u8], sink: Box<dyn BytesSink>) -> u32 {
    sink.receive(bytes.to_vec());
    borrowed_bytes_checksum(bytes)
}

uniffi::include_scaffolding!("array_buffers");
