//! A wake holds the same resource that the next native call needs.

use std::sync::{mpsc::sync_channel, Mutex};

use super::SimpleCallback;

static RESOURCE: Mutex<u64> = Mutex::new(0);

#[no_mangle]
pub extern "C" fn uniffi_test_start_locked_wake(callback: SimpleCallback, handle: u64) {
    let (started, ready) = sync_channel(1);
    std::thread::spawn(move || {
        let mut resource = RESOURCE.lock().unwrap();
        started.send(()).unwrap();
        callback(handle, 1);
        *resource = handle;
    });
    // The next JS call cannot acquire RESOURCE before the worker owns it.
    ready.recv().unwrap();
}

#[no_mangle]
pub extern "C" fn uniffi_test_read_after_wake() -> u64 {
    *RESOURCE.lock().unwrap()
}

static SAVED_WAKE: Mutex<Option<SimpleCallback>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn uniffi_test_save_wake(callback: SimpleCallback) {
    *SAVED_WAKE.lock().unwrap() = Some(callback);
}

#[no_mangle]
pub extern "C" fn uniffi_test_invoke_saved_wake() {
    let callback = *SAVED_WAKE.lock().unwrap();
    callback.unwrap()(902, 0);
}
