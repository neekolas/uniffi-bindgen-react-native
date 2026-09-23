/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
const native = require("./index.js");
const {
  resolveLibPath,
  ResolveLibPathError,
} = require("./typescript/dist/resolve-lib.js");

const FfiType = {
  UInt8: { tag: "UInt8" },
  Int8: { tag: "Int8" },
  UInt16: { tag: "UInt16" },
  Int16: { tag: "Int16" },
  UInt32: { tag: "UInt32" },
  Int32: { tag: "Int32" },
  UInt64: { tag: "UInt64" },
  Int64: { tag: "Int64" },
  Float32: { tag: "Float32" },
  Float64: { tag: "Float64" },
  Handle: { tag: "Handle" },
  RustBuffer: { tag: "RustBuffer" },
  ForeignBytes: { tag: "ForeignBytes" },
  RustCallStatus: { tag: "RustCallStatus" },
  VoidPointer: { tag: "VoidPointer" },
  Void: { tag: "Void" },
  Callback: (name) => ({ tag: "Callback", name }),
  Struct: (name) => ({ tag: "Struct", name }),
  Reference: (inner) => ({ tag: "Reference", inner }),
  MutReference: (inner) => ({ tag: "MutReference", inner }),
};

module.exports = {
  ...native,
  resolveLibPath,
  ResolveLibPathError,
  FfiType,
};

// Node ESM finds the named exports of this file with cjs-module-lexer. The
// lexer does not see names in the `...native` spread above, so without the
// first line, `import { UniffiNativeModule } from "@ubjs/node"` fails. The
// lexer finds the other three names in the object above. Their lines are a
// safety measure, so that each value that `lib.d.ts` declares has an
// explicit assignment.
module.exports.UniffiNativeModule = native.UniffiNativeModule;
module.exports.resolveLibPath = resolveLibPath;
module.exports.ResolveLibPathError = ResolveLibPathError;
module.exports.FfiType = FfiType;
