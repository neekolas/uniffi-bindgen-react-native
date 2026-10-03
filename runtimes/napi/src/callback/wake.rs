/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
//! Owned scalar notifications for the Rust future continuation.

use std::{ffi::c_void, ptr, sync::Arc};

use napi::{sys, Env, NapiRaw};

use super::{on_js_thread, CallbackUserData};

// created, accepted, rejected, delivered, discarded, dropped, null-environment.
#[cfg(feature = "test-hooks")]
static COUNTS: [std::sync::atomic::AtomicU32; 7] =
    [const { std::sync::atomic::AtomicU32::new(0) }; 7];
#[cfg(feature = "test-hooks")]
static NEXT_QUEUE_LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub(super) fn count(index: usize) {
    #[cfg(feature = "test-hooks")]
    COUNTS[index].fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    #[cfg(not(feature = "test-hooks"))]
    let _ = index;
}

#[cfg(feature = "test-hooks")]
#[napi_derive::napi(js_name = "__testWakeCounts", skip_typescript)]
pub fn test_wake_counts() -> Vec<u32> {
    COUNTS
        .iter()
        .map(|counter| counter.load(std::sync::atomic::Ordering::SeqCst))
        .collect()
}

#[cfg(feature = "test-hooks")]
#[napi_derive::napi(js_name = "__testLimitNextWakeQueue", skip_typescript)]
pub fn test_limit_next_wake_queue() {
    NEXT_QUEUE_LIMIT.store(1, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(feature = "test-hooks")]
#[napi_derive::napi(js_name = "__testCloseWakeQueues", skip_typescript)]
pub fn test_close_wake_queues(env: Env) {
    for handle in crate::env_state(env.raw()).close() {
        unsafe {
            sys::napi_release_threadsafe_function(
                handle.0,
                sys::ThreadsafeFunctionReleaseMode::abort,
            );
        }
    }
}

struct WakePayload {
    args: Vec<u8>,
    user_data: *const c_void,
}

impl Drop for WakePayload {
    fn drop(&mut self) {
        count(5);
    }
}

/// Node transfers each accepted payload here, including on environment teardown.
unsafe extern "C" fn deliver(
    env: sys::napi_env,
    _function: sys::napi_value,
    _context: *mut c_void,
    data: *mut c_void,
) {
    if data.is_null() {
        return;
    }
    // Recover ownership before checking env: Node passes null during teardown.
    let payload = unsafe { Box::from_raw(data.cast::<WakePayload>()) };
    let ud = unsafe { &*payload.user_data.cast::<CallbackUserData>() };
    if env.is_null() || ud.env_state.is_shutting_down() || ud.module.is_unloading() {
        count(4);
        if env.is_null() {
            count(6);
        }
        return;
    }
    count(3);
    on_js_thread(payload.args.as_ptr(), ptr::null_mut(), payload.user_data);
}

pub(super) fn create(
    env: &Env,
    state: &Arc<crate::EnvState>,
) -> napi::Result<sys::napi_threadsafe_function> {
    #[cfg(feature = "test-hooks")]
    super::setup_tests::fail_before(super::setup_tests::WAKE_NAME, env)?;
    let name = env.create_string("uniffi_future_wake")?;
    let mut raw = ptr::null_mut();
    let queue_limit = 0;
    #[cfg(feature = "test-hooks")]
    let queue_limit = {
        let _ = queue_limit;
        NEXT_QUEUE_LIMIT.swap(0, std::sync::atomic::Ordering::SeqCst)
    };
    // The environment owns the sole thread count and releases it at cleanup.
    // A custom call_js callback needs no JS function or allocated context.
    napi::check_status!(unsafe {
        sys::napi_create_threadsafe_function(
            env.raw(),
            ptr::null_mut(),
            ptr::null_mut(),
            {
                #[cfg(feature = "test-hooks")]
                if super::setup_tests::take_failure(super::setup_tests::WAKE_CREATE) {
                    ptr::null_mut()
                } else {
                    name.raw()
                }
                #[cfg(not(feature = "test-hooks"))]
                name.raw()
            },
            queue_limit,
            1,
            ptr::null_mut(),
            {
                #[cfg(feature = "test-hooks")]
                {
                    Some(super::setup_tests::wake_finalized)
                }
                #[cfg(not(feature = "test-hooks"))]
                {
                    None
                }
            },
            ptr::null_mut(),
            Some(deliver),
            &mut raw,
        )
    })?;
    #[cfg(feature = "test-hooks")]
    super::setup_tests::count(6);
    let status = unsafe { sys::napi_unref_threadsafe_function(env.raw(), raw) };
    #[cfg(feature = "test-hooks")]
    let status = if status == sys::Status::napi_ok
        && super::setup_tests::take_failure(super::setup_tests::WAKE_UNREF)
    {
        // Keep the real unref and handle. Inject only its recoverable error branch.
        unsafe { super::setup_tests::invalid_status(env) }
    } else {
        status
    };
    if status != sys::Status::napi_ok {
        unsafe {
            sys::napi_release_threadsafe_function(raw, sys::ThreadsafeFunctionReleaseMode::abort)
        };
        #[cfg(feature = "test-hooks")]
        super::setup_tests::count(7);
        return Err(napi::Error::from_reason("Cannot unref future wake queue"));
    }
    unsafe { state.register_tsfn(raw) };
    Ok(raw)
}

pub(super) fn enqueue(ud: &CallbackUserData, args: *const u8, user_data: *const c_void) {
    // The signature guard permits only copied scalar input and no output.
    let args = unsafe { std::slice::from_raw_parts(args, ud.arg_layout.total_size) }.to_vec();
    count(0);
    let payload = Box::new(WakePayload { args, user_data });
    if ud.env_state.is_shutting_down() || ud.module.is_unloading() {
        count(2);
        return;
    }
    let data = Box::into_raw(payload);
    let status = unsafe { ud.env_state.enqueue_wake(ud.wake_tsfn, data.cast()) };
    if status == sys::Status::napi_ok {
        count(1);
    } else {
        // A rejected call did not take ownership, so reclaim it here.
        count(2);
        unsafe { drop(Box::from_raw(data)) };
    }
}
