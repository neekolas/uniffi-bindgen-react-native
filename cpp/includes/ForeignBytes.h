/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
#pragma once

#include "Bridging.h"
#include "UniffiCallInvoker.h"
#include <cmath>
#include <jsi/jsi.h>
#include <limits>

struct ForeignBytes {
  int32_t len;
  const uint8_t *data;
};

namespace uniffi_jsi {
using namespace facebook;
using CallInvoker = uniffi_runtime::UniffiCallInvoker;

// A `&[u8]` argument: a JS `Uint8Array` that Rust borrows for one call.
//
// The generated code uses it in two steps:
//
//  1. `Bridging<ForeignBytes>::fromJs` checks the value and keeps a strong
//     reference to its `ArrayBuffer`. This runs before any other argument is
//     converted, so a failed check cannot leak an owned `RustBuffer`.
//  2. `bytes(rt)` reads the pointer. This runs after all the arguments are
//     converted, and no JS runs between it and the Rust call. So JS cannot
//     detach or resize the buffer while Rust holds the pointer.
//
// Conversions between the two steps can run JS. If that JS detaches or
// shrinks the buffer, the view no longer holds the bytes, and `bytes(rt)`
// gives an empty slice. JS also reports `byteLength` 0 for such a view.
class BorrowedBytes {
public:
  BorrowedBytes(jsi::ArrayBuffer buffer, size_t offset, int32_t length)
      : buffer_(std::move(buffer)), offset_(offset), length_(length) {}

  ForeignBytes bytes(jsi::Runtime &rt) const {
    if (length_ == 0) {
      return ForeignBytes{0, nullptr};
    }
    auto size = buffer_.size(rt);
    auto *data = buffer_.data(rt);
    if (data == nullptr || offset_ > size ||
        static_cast<size_t>(length_) > size - offset_) {
      return ForeignBytes{0, nullptr};
    }
    return ForeignBytes{length_, data + offset_};
  }

private:
  jsi::ArrayBuffer buffer_;
  size_t offset_;
  int32_t length_;
};

template <> struct Bridging<ForeignBytes> {
  static BorrowedBytes fromJs(jsi::Runtime &rt, std::shared_ptr<CallInvoker>,
                              const jsi::Value &value) {
    try {
      auto uint8ArrayCtor = rt.global().getPropertyAsFunction(rt, "Uint8Array");
      if (!value.isObject() ||
          !value.getObject(rt).instanceOf(rt, uint8ArrayCtor)) {
        throw jsi::JSError(rt, "A `&[u8]` argument must be a Uint8Array");
      }
      auto view = value.getObject(rt);
      if (!view.getPropertyAsObject(rt, "buffer").isArrayBuffer(rt)) {
        // A SharedArrayBuffer. Other threads can write to it during the call,
        // so Rust borrows a copy. The copy is a new Uint8Array; `BorrowedBytes`
        // keeps its buffer alive until the call returns.
        view = uint8ArrayCtor.callAsConstructor(rt, value).getObject(rt);
      }
      auto offset = view.getProperty(rt, "byteOffset").asNumber();
      auto length = view.getProperty(rt, "byteLength").asNumber();
      if (!std::isfinite(offset) || offset < 0 ||
          std::floor(offset) != offset || !std::isfinite(length) ||
          length < 0 || std::floor(length) != length ||
          length > std::numeric_limits<int32_t>::max()) {
        throw jsi::JSError(rt, "A `&[u8]` argument has an invalid byteOffset "
                               "or byteLength");
      }
      auto buffer = view.getPropertyAsObject(rt, "buffer").getArrayBuffer(rt);
      if (offset + length > static_cast<double>(buffer.size(rt))) {
        throw jsi::JSError(rt, "A `&[u8]` argument is outside its ArrayBuffer");
      }
      return BorrowedBytes(std::move(buffer), static_cast<size_t>(offset),
                           static_cast<int32_t>(length));
    } catch (const std::logic_error &e) {
      throw jsi::JSError(rt, e.what());
    }
  }
};

} // namespace uniffi_jsi
