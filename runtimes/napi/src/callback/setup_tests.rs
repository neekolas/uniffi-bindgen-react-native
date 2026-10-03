/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
//! Private setup-failure controls. Each fault uses a real N-API error in a live env.
use napi::{sys, Env};
use std::sync::atomic::{AtomicU32, Ordering};

pub(super) const REFERENCE: u32 = 1;
pub(super) const WAKE_NAME: u32 = 2;
pub(super) const WAKE_CREATE: u32 = 3;
pub(super) const WAKE_UNREF: u32 = 4;
pub(super) const ORDINARY_FUNCTION: u32 = 5;
pub(super) const ORDINARY_CREATE: u32 = 6;
pub(super) const ORDINARY_UNREF: u32 = 7;
static NEXT_FAILURE: AtomicU32 = AtomicU32::new(0);
// ref-created/deleted, state-created/rolled-back/committed, fault calls,
// TSFN-created, wake-aborted, ordinary-finalized, reference-delete-failed, wake-finalized.
static COUNTS: [AtomicU32; 11] = [const { AtomicU32::new(0) }; 11];
pub(super) fn count(index: usize) {
    COUNTS[index].fetch_add(1, Ordering::SeqCst);
}
pub(super) fn take_failure(stage: u32) -> bool {
    if NEXT_FAILURE
        .compare_exchange(stage, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        count(5);
        true
    } else {
        false
    }
}
#[napi_derive::napi(js_name = "__testFailNextCallbackSetup", skip_typescript)]
pub fn fail_next(stage: String) -> napi::Result<()> {
    let id = match stage.as_str() {
        "reference" => REFERENCE,
        "wake-name" => WAKE_NAME,
        "wake-create" => WAKE_CREATE,
        "wake-unref" => WAKE_UNREF,
        "ordinary-function" => ORDINARY_FUNCTION,
        "ordinary-create" => ORDINARY_CREATE,
        "ordinary-unref" => ORDINARY_UNREF,
        _ => return Err(napi::Error::from_reason("Unknown setup failure stage")),
    };
    NEXT_FAILURE.store(id, Ordering::SeqCst);
    Ok(())
}
#[napi_derive::napi(js_name = "__testCallbackSetupCounts", skip_typescript)]
pub fn counts() -> Vec<u32> {
    COUNTS.iter().map(|n| n.load(Ordering::SeqCst)).collect()
}

/// The ordinary wrapper owns its dispatch closure, not our setup Box.
/// Force a real N-API error before the wrapper call; this is a routing control,
/// not proof of the third-party wrapper's own failed-create finalizer.
pub(super) fn fail_before(stage: u32, env: &Env) -> napi::Result<()> {
    if !take_failure(stage) {
        return Ok(());
    }
    let status = unsafe {
        match stage {
            WAKE_NAME => {
                sys::napi_create_string_utf8(env.raw(), std::ptr::null(), 1, std::ptr::null_mut())
            }
            ORDINARY_FUNCTION => sys::napi_create_function(
                env.raw(),
                std::ptr::null(),
                1,
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ),
            ORDINARY_CREATE => sys::napi_create_threadsafe_function(
                env.raw(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                1,
                std::ptr::null_mut(),
                None,
                std::ptr::null_mut(),
                None,
                std::ptr::null_mut(),
            ),
            ORDINARY_UNREF => invalid_status(env),
            _ => unreachable!("stage has a direct fault call"),
        }
    };
    napi::check_status!(status)
}
pub(super) struct FinalizerCount;
impl Drop for FinalizerCount {
    fn drop(&mut self) {
        count(8);
    }
}

/// A valid TSFN unref has no recoverable failure in the supported live env.
/// Use a real InvalidArg status to exercise the caller's rollback branch.
pub(super) unsafe fn invalid_status(env: &Env) -> sys::napi_status {
    let mut reference = std::ptr::null_mut();
    sys::napi_create_reference(env.raw(), std::ptr::null_mut(), 1, &mut reference)
}

/// The raw wake TSFN has no userdata. Count its actual Node finalizer separately.
pub(super) unsafe extern "C" fn wake_finalized(
    _env: sys::napi_env,
    _data: *mut std::ffi::c_void,
    _hint: *mut std::ffi::c_void,
) {
    count(10);
}
