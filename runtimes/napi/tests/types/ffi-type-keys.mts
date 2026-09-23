/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// The keys of `FfiType`. `tsc` checks this list against `lib.d.ts`, and
// `lib-types.test.mjs` checks it against `lib.js` at runtime. So `lib.d.ts`
// and `lib.js` cannot declare different keys.
import type { FfiType } from "../../lib.js";

type FfiTypeKey = keyof typeof FfiType;

// Does not compile if the list has a key that `lib.d.ts` does not declare.
export const FFI_TYPE_KEYS = [
  "UInt8",
  "Int8",
  "UInt16",
  "Int16",
  "UInt32",
  "Int32",
  "UInt64",
  "Int64",
  "Float32",
  "Float64",
  "Handle",
  "RustBuffer",
  "ForeignBytes",
  "RustCallStatus",
  "VoidPointer",
  "Void",
  "Callback",
  "Struct",
  "Reference",
  "MutReference",
] as const satisfies readonly FfiTypeKey[];

// Does not compile if `lib.d.ts` declares a key that is not in the list.
type MissingKey = Exclude<FfiTypeKey, (typeof FFI_TYPE_KEYS)[number]>;
export const allKeysListed: [MissingKey] extends [never] ? true : MissingKey =
  true;
