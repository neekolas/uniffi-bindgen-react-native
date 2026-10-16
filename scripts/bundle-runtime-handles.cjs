/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
const fs = require("node:fs");
const path = require("node:path");
const { createRequire } = require("node:module");
const ts = require("typescript");
// esbuild is a dependency of the existing tsx development dependency. Resolve
// through tsx so this works with both hoisted and isolated dependency layouts.
const { build } = createRequire(require.resolve("tsx"))("esbuild");

async function main() {
  const output = process.argv[2];
  if (!output)
    throw new Error("Usage: node scripts/bundle-runtime-handles.cjs OUTPUT.js");
  const bundled = await build({
    entryPoints: [
      path.join(__dirname, "../cpp/test-harness/runtime-handles.ts"),
    ],
    bundle: true,
    write: false,
    format: "iife",
    target: "es2015",
  });
  // Hermes relies on the application's transform for class syntax. Downlevel
  // the bundle with the existing TypeScript compiler, as Metro would do.
  const result = ts.transpileModule(bundled.outputFiles[0].text, {
    compilerOptions: {
      target: ts.ScriptTarget.ES5,
      module: ts.ModuleKind.None,
    },
  });
  fs.writeFileSync(output, result.outputText);
}
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
