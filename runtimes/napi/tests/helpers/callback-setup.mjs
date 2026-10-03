/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readdirSync } from "node:fs";
import { libPath } from "./lib-path.mjs";
import { pollUntil } from "./poll.mjs";

const root = new URL("../../", import.meta.url);
const files = readdirSync(root).filter((name) => name.endsWith(".node"));
assert.equal(files.length, 1, "test exactly one built addon");
const addon = createRequire(import.meta.url)(new URL(files[0], root).pathname);
assert.equal(typeof addon.__testFailNextCallbackSetup, "function");
const type = (tag) => ({ tag });
const Void = type("Void"), UInt64 = type("UInt64"), Int8 = type("Int8");
const callbackType = (name) => ({ tag: "Callback", name });
function open(continuation) {
  const module = addon.UniffiNativeModule.open(libPath("uniffi_napi_test_lib"));
  const name = continuation ? "RustFutureContinuationCallback" : "simple_callback";
  const native = module.register({
    symbols: {
      rustbuffer_alloc: "uniffi_test_rustbuffer_alloc",
      rustbuffer_free: "uniffi_test_rustbuffer_free",
      rustbuffer_from_bytes: "uniffi_test_rustbuffer_from_bytes",
    },
    structs: {},
    callbacks: { [name]: { args: [continuation ? type("Handle") : UInt64, Int8],
      ret: Void, hasRustCallStatus: false } },
    functions: { uniffi_test_fn_call_callback: {
      args: [callbackType(name), UInt64, Int8], ret: Void, hasRustCallStatus: true } },
  });
  return { module, native };
}
const stats = () => addon.__testCallbackSetupCounts();
const delta = (before) => stats().map((value, i) => value - before[i]);
const call = (native, callback) => {
  const status = { code: 0 };
  native.uniffi_test_fn_call_callback(callback, 42n, 7, status);
  assert.equal(status.code, 0, "successful native callback status");
};
const mode = process.argv[2];
const keepAlive = setInterval(() => {}, 1000);
try {
  if (mode === "reference-gc") {
    assert.equal(typeof globalThis.gc, "function", "run the fixture with --expose-gc");
    const { module, native } = open(true);
    let weak;
    function fail() {
      const callback = () => assert.fail("failed callback ran");
      weak = new WeakRef(callback);
      addon.__testFailNextCallbackSetup("wake-name");
      assert.throws(() => call(native, callback), { code: "InvalidArg" });
    }
    fail();
    let collected = false;
    for (let i = 0; i < 100; i++) {
      await new Promise((resolve) => setTimeout(resolve, 1));
      globalThis.gc();
      if (weak.deref() === undefined) { collected = true; break; }
    }
    assert.equal(collected, true, "rollback must release the actual JS callback reference");
    module.unload();
    console.log("PASS reference-gc");
  } else if (mode === "sync-result") {
    const module = addon.UniffiNativeModule.open(libPath("uniffi_napi_test_lib"));
    const cb = (name) => callbackType(name);
    const native = module.register({
      symbols: { rustbuffer_alloc: "uniffi_test_rustbuffer_alloc", rustbuffer_free: "uniffi_test_rustbuffer_free", rustbuffer_from_bytes: "uniffi_test_rustbuffer_from_bytes" },
      structs: { TestVTable: [{ name: "get_value", type: cb("get_value") }, { name: "free", type: cb("free") }] },
      callbacks: { get_value: { args: [UInt64], ret: type("Int32"), hasRustCallStatus: true }, free: { args: [UInt64], ret: Void, hasRustCallStatus: true } },
      functions: {
        uniffi_test_fn_init_vtable: { args: [{ tag: "Reference", inner: { tag: "Struct", name: "TestVTable" } }], ret: Void, hasRustCallStatus: true },
        uniffi_test_fn_use_vtable: { args: [UInt64], ret: type("Int32"), hasRustCallStatus: true },
      },
    });
    const before = stats();
    const status = { code: 0 };
    native.uniffi_test_fn_init_vtable({ get_value: (handle, status) => { status.code = 0; return Number(handle) * 10; }, free: (_handle, status) => { status.code = 0; } }, status);
    assert.equal(status.code, 0);
    assert.equal(native.uniffi_test_fn_use_vtable(7n, status), 70, "synchronous callback result stays intact");
    assert.equal(status.code, 0);
    module.unload(); addon.__testCloseWakeQueues();
    await pollUntil(() => delta(before)[8] === 2, "successful ordinary TSFNs must finalize on close");
    console.log(JSON.stringify({ mode, counts: delta(before), result: 70 }));
    console.log("PASS sync-result");
  } else {
  const continuation = mode.startsWith("wake-");
  const { module, native } = open(continuation);
  const before = stats();
  const owners = module.__testModuleOwners();
  const handles = module.__testEnvironmentHandles();
  let calls = 0;
  const callback = (handle, value) => {
    assert.equal(handle, 42n); assert.equal(value, 7); calls++;
  };
  if (mode !== "success") {
    addon.__testFailNextCallbackSetup(mode);
    let failure;
    try { call(native, callback); } catch (error) { failure = error; }
    assert.ok(failure, "the selected live N-API setup operation must fail");
    if (mode === "reference") assert.match(failure.message, /Failed to create reference/);
    else if (mode === "wake-unref") assert.match(failure.message, /Cannot unref future wake queue/);
    else assert.equal(failure.code, "InvalidArg", "preserve the native setup error status");
    assert.equal(calls, 0, "failed setup must not invoke native callback");
    assert.equal(module.__testModuleOwners(), owners, "failed setup must release Module owner");
    assert.equal(module.__testEnvironmentHandles(), handles, "failed setup must not register a handle");
    if (mode === "ordinary-unref") {
      await pollUntil(() => delta(before)[8] === 1, "failed setup TSFN must finalize");
    }
    if (mode === "wake-unref") await pollUntil(() => delta(before)[10] === 1, "failed wake setup must finalize the real aborted TSFN");
    const expected = mode === "reference" ? [0,0,0,0,0,1,0,0,0,0,0]
      : [1,1,1,1,0,1,Number(mode.endsWith("unref")),Number(mode === "wake-unref"),Number(mode === "ordinary-unref"),0,Number(mode === "wake-unref")];
    assert.deepEqual(delta(before), expected, "setup must delete reference and drop owned state exactly once");
    console.log(JSON.stringify({ mode, error: failure.message, code: failure.code, counts: delta(before), owners, handles }));
  }
  // The same JS function must remain usable after setup failed.
  call(native, callback);
  if (continuation) {
    assert.equal(calls, 0, "continuation must not run on the Rust poll stack");
    await pollUntil(() => calls === 1, "successful continuation delivery");
  } else assert.equal(calls, 1, "ordinary callback stays synchronous");
  call(native, callback);
  if (continuation) await pollUntil(() => calls === 2, "cached continuation delivery");
  else assert.equal(calls, 2, "cached ordinary callback stays synchronous");
  assert.equal(module.__testEnvironmentHandles(), handles + 1, "success registers one cached handle");
  const successful = delta(before);
  assert.equal(successful[4], 1, "success transfers exactly one setup Box");
  assert.equal(successful[9], 0, "no reference deletion failed");
  module.unload();
  addon.__testCloseWakeQueues();
  assert.equal(module.__testEnvironmentHandles(), 0, "owned handles leave the environment on close");
  console.log(JSON.stringify({ mode, successful, calls, unloadedOwners: module.__testModuleOwners() }));
  await pollUntil(() => delta(before)[continuation ? 10 : 8] === successful[6], "all created TSFNs must finalize exactly once");
  console.log(`PASS ${mode}`);
  }
} finally { clearInterval(keepAlive); }
