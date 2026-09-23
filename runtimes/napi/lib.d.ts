/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

// Types for `lib.js`, the entry point of this package.
//
// `index.d.ts` is made by NAPI-RS from the Rust code. It does not declare the
// names that `lib.js` adds to the native exports, so this file declares them.
// Keep it in step with `lib.js`.

export * from "./index";
export {
  resolveLibPath,
  ResolveLibPathError,
  type ResolveLibPathOptions,
  type ResolveMode,
  type TripleStyle,
} from "./typescript/dist/resolve-lib";

/** The description of an FFI type that `UniffiNativeModule.register` reads. */
export type FfiTypeDesc =
  | { readonly tag: "UInt8" }
  | { readonly tag: "Int8" }
  | { readonly tag: "UInt16" }
  | { readonly tag: "Int16" }
  | { readonly tag: "UInt32" }
  | { readonly tag: "Int32" }
  | { readonly tag: "UInt64" }
  | { readonly tag: "Int64" }
  | { readonly tag: "Float32" }
  | { readonly tag: "Float64" }
  | { readonly tag: "Handle" }
  | { readonly tag: "RustBuffer" }
  | { readonly tag: "ForeignBytes" }
  | { readonly tag: "RustCallStatus" }
  | { readonly tag: "VoidPointer" }
  | { readonly tag: "Void" }
  | { readonly tag: "Callback"; readonly name: string }
  | { readonly tag: "Struct"; readonly name: string }
  | { readonly tag: "Reference"; readonly inner: FfiTypeDesc }
  | { readonly tag: "MutReference"; readonly inner: FfiTypeDesc };

export declare const FfiType: {
  readonly UInt8: { readonly tag: "UInt8" };
  readonly Int8: { readonly tag: "Int8" };
  readonly UInt16: { readonly tag: "UInt16" };
  readonly Int16: { readonly tag: "Int16" };
  readonly UInt32: { readonly tag: "UInt32" };
  readonly Int32: { readonly tag: "Int32" };
  readonly UInt64: { readonly tag: "UInt64" };
  readonly Int64: { readonly tag: "Int64" };
  readonly Float32: { readonly tag: "Float32" };
  readonly Float64: { readonly tag: "Float64" };
  readonly Handle: { readonly tag: "Handle" };
  readonly RustBuffer: { readonly tag: "RustBuffer" };
  readonly ForeignBytes: { readonly tag: "ForeignBytes" };
  readonly RustCallStatus: { readonly tag: "RustCallStatus" };
  readonly VoidPointer: { readonly tag: "VoidPointer" };
  readonly Void: { readonly tag: "Void" };
  readonly Callback: (name: string) => {
    readonly tag: "Callback";
    readonly name: string;
  };
  readonly Struct: (name: string) => {
    readonly tag: "Struct";
    readonly name: string;
  };
  readonly Reference: (inner: FfiTypeDesc) => {
    readonly tag: "Reference";
    readonly inner: FfiTypeDesc;
  };
  readonly MutReference: (inner: FfiTypeDesc) => {
    readonly tag: "MutReference";
    readonly inner: FfiTypeDesc;
  };
};
