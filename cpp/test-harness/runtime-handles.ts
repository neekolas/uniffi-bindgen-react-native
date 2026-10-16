/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import { UniffiHandleMap } from "../../typescript/src/handle-map.ts";

// Bundled into the real Hermes lifetime harness. These are the production maps,
// using the native allocator installed by that harness, rather than mock hooks.
const callbacks = new UniffiHandleMap<string>();
const continuations = new UniffiHandleMap<string>("rust-future-continuation");
const futures = new UniffiHandleMap<string>("foreign-future-drop");
const callback = callbacks.insert("callback");
const continuation = continuations.insert("continuation");
const future = futures.insert("future");

function require(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

(globalThis as any).__ubrnTestHandles = {
  callback,
  continuation,
  future,
  checkLive(stale: bigint) {
    require(callback !== continuation &&
      callback !== future &&
      continuation !== future, "different maps reused a native handle");
    require(callbacks.get(callback) === "callback", "active callback was lost");
    require(continuations.get(continuation) ===
      "continuation", "active continuation was lost");
    require(futures.get(future) === "future", "active foreign future was lost");
    require(!callbacks.has(
      stale,
    ), "replacement map accepted an old runtime's handle");
    const clone = callbacks.clone(callback);
    require(clone !== callback &&
      callbacks.get(clone) === "callback", "native clone failed");
    callbacks.remove(clone);
  },
  checkStopped() {
    require(!callbacks.has(callback), "retired map still accepts a callback");
    let rejectedGet = false;
    let rejectedInsert = false;
    try {
      callbacks.get(callback);
    } catch {
      rejectedGet = true;
    }
    try {
      callbacks.insert("late");
    } catch {
      rejectedInsert = true;
    }
    require(rejectedGet &&
      rejectedInsert, "retired map used the old native allocator");
  },
  release() {
    callbacks.remove(callback);
    continuations.remove(continuation);
    futures.remove(future);
  },
};
