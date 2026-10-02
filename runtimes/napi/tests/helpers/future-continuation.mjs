/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import assert from "node:assert/strict";
import { Worker, parentPort, workerData } from "node:worker_threads";
import lib from "../../lib.js";
import { libPath } from "./lib-path.mjs";

const { UniffiNativeModule, FfiType } = lib;
const module = UniffiNativeModule.open(libPath("uniffi_napi_test_lib"));
const callbackName = "RustFutureContinuationCallback";
const native = module.register({
  symbols: {
    rustbuffer_alloc: "uniffi_test_rustbuffer_alloc",
    rustbuffer_free: "uniffi_test_rustbuffer_free",
    rustbuffer_from_bytes: "uniffi_test_rustbuffer_from_bytes",
  },
  structs: {},
  callbacks: {
    [callbackName]: {
      args: [FfiType.Handle, FfiType.Int8],
      ret: FfiType.Void,
      hasRustCallStatus: false,
    },
  },
  functions: {
    uniffi_test_start_locked_wake: {
      args: [FfiType.Callback(callbackName), FfiType.UInt64],
      ret: FfiType.Void,
      hasRustCallStatus: false,
    },
    uniffi_test_read_after_wake: {
      args: [],
      ret: FfiType.UInt64,
      hasRustCallStatus: false,
    },
    uniffi_test_fn_call_callback: {
      args: [FfiType.Callback(callbackName), FfiType.UInt64, FfiType.Int8],
      ret: FfiType.Void,
      hasRustCallStatus: true,
    },
  },
});

if (workerData === "queued-wake-teardown") {
  for (let handle = 1n; handle <= 32n; handle++) {
    native.uniffi_test_fn_call_callback(
      () => {
        throw new Error("the held worker must not drain queued wakes");
      },
      handle,
      0,
      { code: 0 },
    );
  }
  parentPort.postMessage("queued");
  // Worker termination must release its queued notifications. Atomics.wait
  // holds JS delivery without keeping a native FFI call on the stack.
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0);
  throw new Error("worker unexpectedly resumed");
}

// Keep the process alive until the unreferenced TSFN delivers all callbacks.
const keepAlive = setInterval(() => {}, 1000);
try {
  const received = [];
  let finish;
  const callback = (handle, poll) => {
    received.push([handle, poll]);
    finish?.();
  };
  for (let cycle = 0; cycle < 20; cycle++) {
    const first = received.length;
    for (let call = 0; call < 32; call++) {
      const handle = BigInt(cycle * 32 + call + 1);
      native.uniffi_test_start_locked_wake(callback, handle);
      // Old dispatch waits for this JS thread while it holds RESOURCE.
      // This call then blocks forever on RESOURCE. The parent bounds the child.
      assert.equal(native.uniffi_test_read_after_wake(), handle);
    }
    assert.equal(
      received.length,
      first,
      "a wake ran JS before native calls returned",
    );
    await new Promise((resolve) => {
      finish = () => {
        if (received.length === first + 32) resolve();
      };
    });
    assert.deepEqual(
      received.slice(first),
      Array.from({ length: 32 }, (_, call) => [
        BigInt(cycle * 32 + call + 1),
        1,
      ]),
    );
  }
  const first = received.length;
  native.uniffi_test_fn_call_callback(callback, 900n, 0, { code: 0 });
  assert.equal(received.length, first, "same-thread READY delivery was inline");
  await new Promise((resolve) => {
    finish = resolve;
  });
  assert.deepEqual(received.at(-1), [900n, 0]);
  const worker = new Worker(new URL(import.meta.url), {
    workerData: "queued-wake-teardown",
  });
  await new Promise((resolve, reject) => {
    worker.once("message", (message) => {
      assert.equal(message, "queued");
      resolve();
    });
    worker.once("error", reject);
    worker.once("exit", () =>
      reject(new Error("worker exited before holding wakes")),
    );
  });
  assert.equal(await worker.terminate(), 1);
  native.uniffi_test_fn_call_callback(callback, 901n, 0, { code: 0 });
  await new Promise((resolve) => {
    finish = resolve;
  });
  assert.deepEqual(received.at(-1), [901n, 0]);
  console.log("PASS 640 locked wakes, queued READY and worker teardown");
} finally {
  clearInterval(keepAlive);
}
