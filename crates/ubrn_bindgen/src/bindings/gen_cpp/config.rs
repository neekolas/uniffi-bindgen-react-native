/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub(crate) struct CppConfig {
    /// Quoted includes needed by the native-object policy.
    pub includes: Vec<String>,
    /// Emit `<namespace>_ffi.hpp` from the component's native ABI metadata.
    pub emit_ffi_header: bool,
    pub native_object: Option<NativeObjectPolicy>,
}

/// Observe direct async object results before JavaScript can adopt them.
/// Completion/free functions are shared by all futures with the same ABI result;
/// policy hooks must ignore handles not registered by `future_started`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeObjectPolicy {
    pub object: String,
    pub future_started: String,
    pub future_completed: String,
    pub future_freed: String,
    pub discard: String,
}
