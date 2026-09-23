/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// Checks `lib.d.ts`, the hand-written types of `lib.js`.
//
// The TypeScript compiler API reads what `lib.d.ts` declares, so there is no
// second list of names to keep in step. Each test compares those names with
// what `lib.js` gives at runtime.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const libDts = fileURLToPath(new URL("../lib.d.ts", import.meta.url));

/** The value exports of `lib.d.ts`, and the keys of its `FfiType`. */
function declared() {
  const program = ts.createProgram([libDts], {
    strict: true,
    noEmit: true,
    target: ts.ScriptTarget.ES2022,
    lib: ["lib.es2022.d.ts"],
    types: [],
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
  });
  const checker = program.getTypeChecker();
  const file = program.getSourceFile(libDts);
  const exports = checker.getExportsOfModule(checker.getSymbolAtLocation(file));
  const isValue = (symbol) => {
    const target =
      symbol.flags & ts.SymbolFlags.Alias
        ? checker.getAliasedSymbol(symbol)
        : symbol;
    return (target.flags & ts.SymbolFlags.Value) !== 0;
  };
  const ffiType = exports.find((symbol) => symbol.getName() === "FfiType");
  assert.ok(ffiType, "lib.d.ts does not declare FfiType");
  return {
    values: exports.filter(isValue).map((symbol) => symbol.getName()),
    ffiTypeKeys: checker
      .getTypeOfSymbolAtLocation(ffiType, file)
      .getProperties()
      .map((property) => property.getName()),
  };
}

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
  assert.deepEqual(Object.keys(FfiType).sort(), declared().ffiTypeKeys.sort());
});
