/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import { test } from "node:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import assert from "node:assert/strict";
import lib from "../lib.js";
import { libPath } from "./helpers/lib-path.mjs";

const { UniffiNativeModule, FfiType } = lib;

test("future wake returns while JS cannot drain its queue", () => {
  const child = spawnSync(
    process.execPath,
    [join(import.meta.dirname, "helpers/future-continuation.mjs")],
    { timeout: 10_000, killSignal: "SIGKILL", encoding: "utf8" },
  );
  assert.equal(child.error, undefined, child.error?.message);
  assert.equal(child.status, 0, child.stderr);
  assert.match(
    child.stdout,
    /^PASS 640 locked wakes, queued READY and worker teardown$/m,
  );
});

function register(callbackName, definition) {
  const module = UniffiNativeModule.open(libPath("uniffi_napi_test_lib"));
  return module.register({
    symbols: {
      rustbuffer_alloc: "uniffi_test_rustbuffer_alloc",
      rustbuffer_free: "uniffi_test_rustbuffer_free",
      rustbuffer_from_bytes: "uniffi_test_rustbuffer_from_bytes",
    },
    structs: {},
    callbacks: { [callbackName]: definition },
    functions: {
      uniffi_test_fn_call_callback: {
        args: [FfiType.Callback(callbackName), FfiType.UInt64, FfiType.Int8],
        ret: FfiType.Void,
        hasRustCallStatus: true,
      },
    },
  });
}

const continuation = {
  args: [FfiType.Handle, FfiType.Int8],
  ret: FfiType.Void,
  hasRustCallStatus: false,
};

test("ordinary void callbacks still return after JS runs", () => {
  const native = register("ordinaryCallback", continuation);
  let received;
  native.uniffi_test_fn_call_callback(
    (handle, value) => {
      received = [handle, value];
    },
    7n,
    -3,
    { code: 0 },
  );
  assert.deepEqual(received, [7n, -3]);
});

for (const [label, change] of [
  ["borrowed input", { args: [FfiType.VoidPointer, FfiType.Int8] }],
  ["return value", { ret: FfiType.UInt64 }],
  ["out pointer", { outReturn: true }],
  ["call status", { hasRustCallStatus: true }],
]) {
  test(`future continuation rejects ${label}`, () => {
    const native = register("RustFutureContinuationCallback", {
      ...continuation,
      ...change,
    });
    assert.throws(
      () => native.uniffi_test_fn_call_callback(() => {}, 1n, 0, { code: 0 }),
      /Invalid RustFutureContinuationCallback signature/,
    );
  });
}
