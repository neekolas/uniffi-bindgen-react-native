/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import { test } from "node:test";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import assert from "node:assert/strict";

for (const mode of ["unload", "ownership"]) {
  test(`future wake ${mode}`, () => {
    const child = spawnSync(
      process.execPath,
      [join(import.meta.dirname, "helpers/future-wake-lifecycle.mjs"), mode],
      {
        timeout: 30_000,
        killSignal: "SIGKILL",
        encoding: "utf8",
      },
    );
    assert.equal(child.error, undefined, child.error?.message);
    assert.equal(child.status, 0, child.stdout + child.stderr);
    assert.match(child.stdout, new RegExp(`^PASS ${mode}$`, "m"));
    console.log(child.stdout.trim());
  });
}
