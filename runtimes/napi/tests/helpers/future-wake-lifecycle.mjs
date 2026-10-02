/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readdirSync } from "node:fs";
import { Worker, parentPort, workerData } from "node:worker_threads";
import lib from "../../lib.js";
import { libPath } from "./lib-path.mjs";

const { UniffiNativeModule, FfiType } = lib;
const root = new URL("../../", import.meta.url);
const files = readdirSync(root).filter((name) => name.endsWith(".node"));
assert.equal(files.length, 1, "test exactly one built native addon");
const addon = createRequire(import.meta.url)(new URL(files[0], root).pathname);
const stats = addon.__testWakeCounts;
assert.equal(typeof stats, "function", "build the test-hooks fixture first");
const delta = (before) => stats().map((value, i) => value - before[i]);
function open() {
  const module = UniffiNativeModule.open(libPath("uniffi_napi_test_lib"));
  const native = module.register({
    symbols: {
      rustbuffer_alloc: "uniffi_test_rustbuffer_alloc",
      rustbuffer_free: "uniffi_test_rustbuffer_free",
      rustbuffer_from_bytes: "uniffi_test_rustbuffer_from_bytes",
    },
    structs: {},
    callbacks: {
      RustFutureContinuationCallback: {
        args: [FfiType.Handle, FfiType.Int8],
        ret: FfiType.Void,
        hasRustCallStatus: false,
      },
    },
    functions: {
      uniffi_test_fn_call_callback: {
        args: [
          FfiType.Callback("RustFutureContinuationCallback"),
          FfiType.UInt64,
          FfiType.Int8,
        ],
        ret: FfiType.Void,
        hasRustCallStatus: true,
      },
      uniffi_test_save_wake: {
        args: [FfiType.Callback("RustFutureContinuationCallback")],
        ret: FfiType.Void,
        hasRustCallStatus: false,
      },
      uniffi_test_invoke_saved_wake: {
        args: [],
        ret: FfiType.Void,
        hasRustCallStatus: false,
      },
    },
  });
  return { module, native };
}
const call = (native, callback) =>
  native.uniffi_test_fn_call_callback(callback, 1n, 0, { code: 0 });
const tick = () => new Promise((resolve) => setTimeout(resolve, 1));
async function until(predicate) {
  for (let tries = 0; tries < 5000; tries++) {
    if (predicate()) return;
    await tick();
  }
  assert.fail("queue did not reach its expected state");
}
if (workerData === "held" || workerData === "aborted") {
  const { native } = open();
  const callback = () => {
    throw new Error("held worker delivered a wake");
  };
  for (let i = 0; i < 160; i++) call(native, callback);
  native.uniffi_test_save_wake(callback);
  if (workerData === "aborted") addon.__testCloseWakeQueues();
  parentPort.postMessage("queued");
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0);
  throw new Error("held worker resumed");
}
const keepAlive = setInterval(() => {}, 1000);
try {
  const mode = process.argv[2];
  const live = open();
  let received = 0;
  const callback = () => received++;
  if (mode === "unload") {
    const before = stats();
    const doomed = open();
    let late = 0;
    call(doomed.native, () => late++);
    doomed.module.unload();
    call(live.native, callback);
    await until(() => received === 1);
    assert.equal(late, 0, "a queued callback ran after module unload");
    assert.deepEqual(delta(before), [2, 2, 0, 1, 1, 2, 0]);
    console.log(
      JSON.stringify({ phase: "unload-and-live", counts: delta(before) }),
    );
  } else if (mode === "ownership") {
    let before = stats();
    for (let i = 0; i < 3200; i++) call(live.native, callback);
    await until(() => received === 3200);
    assert.equal(delta(before)[0], 3200);
    assert.equal(
      delta(before)[5],
      3200,
      "drained wake allocations must be freed",
    );
    console.log(JSON.stringify({ phase: "drained", counts: delta(before) }));
    before = stats();
    for (let round = 0; round < 20; round++) {
      const worker = new Worker(new URL(import.meta.url), {
        workerData: "held",
      });
      await new Promise((resolve, reject) => {
        worker.once("message", resolve);
        worker.once("error", reject);
        worker.once("exit", () =>
          reject(new Error("worker exited before queue hold")),
        );
      });
      assert.equal(await worker.terminate(), 1);
    }
    const closed = delta(before);
    console.log(
      JSON.stringify({
        phase: "teardown",
        created: closed[0],
        dropped: closed[5],
      }),
    );
    assert.equal(closed[0], 3200);
    assert.equal(
      closed[5],
      3200,
      "worker teardown must free every wake allocation",
    );
    assert.equal(closed[1], 3200);
    assert.equal(closed[2], 0);
    assert.equal(closed[3] + closed[4], 3200);
    // Node may drain native callbacks before its cleanup hook during terminate.
    // Explicitly run that same close path with queued data to cover null env.
    before = stats();
    const aborted = new Worker(new URL(import.meta.url), {
      workerData: "aborted",
    });
    await new Promise((resolve, reject) => {
      aborted.once("message", resolve);
      aborted.once("error", reject);
    });
    assert.equal(await aborted.terminate(), 1);
    console.log(
      JSON.stringify({ phase: "null-env-abort", counts: delta(before) }),
    );
    assert.equal(
      delta(before)[5],
      160,
      "null-env teardown must free accepted payloads",
    );
    assert.deepEqual(delta(before), [160, 160, 0, 0, 160, 160, 160]);
    before = stats();
    live.native.uniffi_test_invoke_saved_wake();
    assert.deepEqual(
      delta(before),
      [1, 0, 1, 0, 0, 1, 0],
      "late wake on closed environment must be rejected and freed",
    );
    call(live.native, callback);
    await until(() => received === 3201);
    before = stats();
    const limited = open();
    addon.__testLimitNextWakeQueue();
    let accepted = 0;
    const limitedCallback = () => accepted++;
    call(limited.native, limitedCallback);
    call(limited.native, limitedCallback);
    assert.deepEqual(
      delta(before),
      [2, 1, 1, 0, 0, 1, 0],
      "napi queue-full rejection must return payload ownership",
    );
    await until(() => accepted === 1);
    assert.deepEqual(delta(before), [2, 1, 1, 1, 0, 2, 0]);
    console.log(
      JSON.stringify({
        phase: "queue-full-and-delivery",
        counts: delta(before),
      }),
    );
  } else {
    assert.fail(`unknown mode ${mode}`);
  }
  console.log(`PASS ${mode}`);
} finally {
  clearInterval(keepAlive);
}
