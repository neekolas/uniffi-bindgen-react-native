/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// Checks `lib.d.ts`, the hand-written types of `lib.js`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { FFI_TYPE_KEYS } from "./types/ffi-type-keys.mts";

const require = createRequire(import.meta.url);

test("a consumer compiles against lib.d.ts with skipLibCheck false", () => {
  const tsc = require.resolve("typescript/bin/tsc");
  const tsconfig = fileURLToPath(
    new URL("./types/tsconfig.json", import.meta.url),
  );
  const result = spawnSync(process.execPath, [tsc, "--project", tsconfig], {
    encoding: "utf8",
  });
  assert.equal(
    result.status,
    0,
    `tsc failed:\n${result.stdout}${result.stderr}`,
  );
});

test("lib.d.ts and lib.js declare the same FfiType keys", () => {
  const { FfiType } = require("../lib.js");
  assert.deepEqual(Object.keys(FfiType).sort(), [...FFI_TYPE_KEYS].sort());
});
