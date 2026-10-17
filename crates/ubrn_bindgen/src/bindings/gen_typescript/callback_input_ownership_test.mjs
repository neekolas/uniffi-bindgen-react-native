/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { createRequire } from "node:module";
const ts = createRequire(path.join(process.env.UBRN_TEST_ROOT, "package.json"))(
  "typescript",
);
const transpile = (source) =>
  ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ES2022,
    },
  }).outputText;

const generated = fs.readFileSync(0, "utf8");
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "ubrn-callback-inputs-"));
try {
  // Compile the production runtime with the repository TypeScript dependency.
  // Exercise the real callback map and string converter.
  const sources = path.join(process.env.UBRN_TEST_ROOT, "typescript/src");
  for (const name of fs
    .readdirSync(sources)
    .filter((name) => name.endsWith(".ts"))) {
    const js = transpile(fs.readFileSync(path.join(sources, name), "utf8"));
    fs.writeFileSync(
      path.join(temp, name.replace(/\.ts$/, ".mjs")),
      js.replace(/(["']\.\/[^"']+)\.ts(["'])/g, "$1.mjs$2"),
    );
  }
  const runtime = await import(
    pathToFileURL(path.join(temp, "ffi-converters.mjs"))
  );
  const { FfiConverterCallback } = await import(
    pathToFileURL(path.join(temp, "callbacks.mjs"))
  );
  const encoder = new TextEncoder();
  const string = runtime.uniffiCreateFfiConverterString({
    bytesToString: (bytes) =>
      new TextDecoder("utf-8", { fatal: true }).decode(bytes),
    stringToBytes: (value) => encoder.encode(value),
    stringByteLength: (value) => encoder.encode(value).length,
  });
  const calls = [
    ...generated.matchAll(/const uniffiMakeCall = ([\s\S]*?)\n            };/g),
  ];
  assert.equal(
    calls.length,
    3,
    "must exercise sync, async, and nested-byte generated callbacks",
  );
  for (const [index, match] of calls.slice(0, 2).entries()) {
    const asyncCall = index === 1;
    const callbackConverterName = match[0].match(
      /(\w+)\.lift\(uniffiHandle\)/,
    )[1];
    const source = transpile(match[0] + "\nreturn uniffiMakeCall;");
    const create = new Function(
      callbackConverterName,
      "FfiConverterString",
      "FfiConverterInt32",
      "uniffiHandle",
      "first",
      "second",
      "number",
      "nativeModule",
      source,
    );
    for (const scenario of [
      "success",
      "first-lift-throws",
      "second-lift-throws",
      "stale-handle",
      "user-throws",
      "pending",
    ]) {
      if (scenario === "pending" && !asyncCall) continue;
      const first =
        scenario === "first-lift-throws"
          ? new Uint8Array([255])
          : encoder.encode("first");
      const second =
        scenario === "second-lift-throws"
          ? new Uint8Array([255])
          : encoder.encode("second");
      const freed = [];
      let called = 0;
      let resolve;
      const callback = (...args) => {
        called++;
        assert.deepEqual(
          freed,
          [first, second],
          `${scenario}: release inputs before user code`,
        );
        assert.deepEqual(args.slice(0, 3), ["first", "second", 7]);
        if (scenario === "user-throws") throw new Error("user failed");
        if (scenario === "pending")
          return new Promise((done) => {
            resolve = done;
          });
      };
      const converter = new FfiConverterCallback();
      const handle = converter.lower({ now: callback, later: callback });
      if (scenario === "stale-handle") converter.drop(handle);
      const makeCall = create(
        converter,
        string,
        runtime.FfiConverterInt32,
        handle,
        first,
        second,
        7,
        () => ({
          rustbuffer_free(buffer) {
            assert.ok(
              buffer === first || buffer === second,
              "only RustBuffer inputs may be freed",
            );
            assert.ok(
              !freed.includes(buffer),
              "each input is freed exactly once",
            );
            freed.push(buffer);
            // Model native memory becoming unusable immediately after release.
            buffer.fill(0);
          },
        }),
      );
      if (
        [
          "first-lift-throws",
          "second-lift-throws",
          "stale-handle",
          "user-throws",
        ].includes(scenario)
      ) {
        if (asyncCall)
          await assert.rejects(makeCall(new AbortController().signal));
        else assert.throws(makeCall);
        assert.equal(called, scenario === "user-throws" ? 1 : 0);
      } else {
        const result = makeCall(new AbortController().signal);
        assert.deepEqual(
          freed,
          [first, second],
          "release must be synchronous even while promise is pending",
        );
        if (resolve) resolve();
        await result;
        assert.equal(called, 1);
      }
      assert.deepEqual(
        freed,
        [first, second],
        `${scenario}: all inputs freed once`,
      );
      converter.drop(handle);
    }
  }
  // The third callback carries Map<string, Vec<bytes>> in one RustBuffer.
  // Exercise both supported bytes representations with the production nested
  // converters. Retain the result beyond the callback and poison its source
  // during the generated cleanup, before user code gets control.
  const nestedCall = calls[2][0];
  const callbackConverterName = nestedCall.match(
    /(\w+)\.lift\(uniffiHandle\)/,
  )[1];
  const nestedConverterName = nestedCall.match(/(\w+)\.lift\(nested\)/)[1];
  const createNested = new Function(
    callbackConverterName,
    nestedConverterName,
    "uniffiHandle",
    "nested",
    "nativeModule",
    transpile(nestedCall + "\nreturn uniffiMakeCall;"),
  );
  for (const [representation, bytesConverter] of [
    ["Uint8Array", runtime.FfiConverterUint8Array],
    ["ArrayBuffer", runtime.FfiConverterArrayBuffer],
  ]) {
    const nestedConverter = new runtime.FfiConverterMap(
      string,
      new runtime.FfiConverterArray(bytesConverter),
    );
    const bytes = (values) => {
      const view = new Uint8Array(values);
      return representation === "ArrayBuffer" ? view.buffer : view;
    };
    const expected = new Map([
      ["payloads", [bytes([17, 31, 47]), bytes([59, 71])]],
      ["empty", []],
    ]);
    // A nonzero offset catches reads that accidentally address the whole
    // backing allocation instead of the RustBuffer view.
    const source = nestedConverter.lower(
      expected,
      (size) => new Uint8Array(new ArrayBuffer(size + 16), 8, size),
    );
    let frees = 0;
    let retained;
    const converter = new FfiConverterCallback();
    const handle = converter.lower({
      nested(value) {
        assert.equal(frees, 1, "nested source is freed before user code");
        retained = value;
      },
    });
    const makeCall = createNested(
      converter,
      nestedConverter,
      handle,
      source,
      () => ({
        rustbuffer_free(buffer) {
          assert.equal(buffer, source);
          assert.equal(++frees, 1, "nested source is freed exactly once");
          buffer.fill(0xa5);
        },
      }),
    );
    makeCall();
    await Promise.resolve();
    assert.deepEqual(
      retained,
      expected,
      `${representation}: retained nested bytes survive source release`,
    );
    for (const value of retained.get("payloads")) {
      const backing = representation === "ArrayBuffer" ? value : value.buffer;
      assert.notEqual(
        backing,
        source.buffer,
        `${representation}: lifted bytes own their backing memory`,
      );
    }
    assert.ok(
      source.every((byte) => byte === 0xa5),
      "source was actually poisoned",
    );
    converter.drop(handle);
  }
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
