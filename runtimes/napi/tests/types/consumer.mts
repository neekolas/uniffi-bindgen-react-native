/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// A project that uses `@ubjs/node`. `lib-types.test.mjs` compiles this file
// with `skipLibCheck: false` and without `@types/node`, so `tsc` also checks
// the published `.d.ts` files. This file does not run.
import lib, {
  FfiType,
  ResolveLibPathError,
  UniffiNativeModule,
  resolveLibPath,
  type FfiTypeDesc,
  type ResolveLibPathOptions,
} from "../../lib.js";

// The generated napi `-ffi.ts` reads the names from the default import.
const {
  UniffiNativeModule: DefaultModule,
  FfiType: defaultFfiType,
  resolveLibPath: defaultResolveLibPath,
} = lib;
const sameFfiType: typeof FfiType = defaultFfiType;
const sameModule: typeof UniffiNativeModule = DefaultModule;
const sameResolve: typeof resolveLibPath = defaultResolveLibPath;

const descs: FfiTypeDesc[] = [
  FfiType.UInt8,
  FfiType.Void,
  FfiType.Callback("callback"),
  FfiType.Struct("struct"),
  FfiType.Reference(FfiType.RustBuffer),
  FfiType.MutReference(FfiType.Handle),
];

// @ts-expect-error: "Bogus" is not an FFI type tag.
const bogusDesc: FfiTypeDesc = { tag: "Bogus" };

// @ts-expect-error: `FfiType` has no "Bogus" member.
const bogusType = FfiType.Bogus;

const options: ResolveLibPathOptions = {
  crateName: "my_crate",
  callerUrl: "file:///app/index.js",
};
const path: string = resolveLibPath(options);

const error = new ResolveLibPathError({
  message: "not found",
  mode: "colocated",
  crateName: "my_crate",
  attempted: [path],
});
const attempted: string[] = error.attempted;
const isError: Error = error;

const registered: object = UniffiNativeModule.open(path).register({});

export {
  sameFfiType,
  sameModule,
  sameResolve,
  descs,
  bogusDesc,
  bogusType,
  attempted,
  isError,
  registered,
};
