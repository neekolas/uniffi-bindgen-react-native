/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import { test } from "node:test";
import { spawnSync } from "node:child_process";
import assert from "node:assert/strict";

for (const mode of ["reference", "wake-name", "wake-create", "wake-unref",
  "ordinary-function", "ordinary-create", "ordinary-unref", "reference-gc", "sync-result", "success"]) {
  test(`callback setup ${mode}`, () => {
    const child = spawnSync(process.execPath,
      ["--expose-gc", new URL("./helpers/callback-setup.mjs", import.meta.url).pathname, mode],
      { timeout: 30_000, killSignal: "SIGKILL", encoding: "utf8" });
    assert.equal(child.error, undefined, child.error?.message);
    assert.equal(child.status, 0, child.stdout + child.stderr);
    assert.match(child.stdout, new RegExp(`^PASS ${mode}$`, "m"));
    console.log(child.stdout.trim());
  });
}
