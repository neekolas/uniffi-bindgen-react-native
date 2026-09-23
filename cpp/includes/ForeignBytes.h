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
#include <optional>

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
//     converted, and no JS runs between it and the Rust call.
//
// `bytes(rt)` must not throw: when it runs, owned `RustBuffer` arguments are
// already converted, and a throw would leak them. If the buffer was detached
// or shrunk after step 1, the view no longer holds the bytes, so `bytes(rt)`
// gives an empty slice. (Hermes throws from `size()` and `data()` for a
// detached buffer; `bytes(rt)` catches that.) Plain JS cannot detach an
// `ArrayBuffer` on Hermes today, so only native code can cause this.
//
// JS must not change, detach or resize the buffer while Rust runs, for
// example from a callback that Rust calls. Rust reads the memory directly.
//
// See docs/src/idioms/common-types.md for the behaviour on each flavor.
class BorrowedBytes {
public:
  // An empty argument. It holds no buffer.
  BorrowedBytes() = default;

  BorrowedBytes(jsi::ArrayBuffer buffer, size_t offset, int32_t length)
      : buffer_(std::move(buffer)), offset_(offset), length_(length) {}

  ForeignBytes bytes(jsi::Runtime &rt) const noexcept {
    if (!buffer_.has_value() || length_ == 0) {
      return ForeignBytes{0, nullptr};
    }
    try {
      auto size = buffer_->size(rt);
      auto *data = buffer_->data(rt);
      if (data == nullptr || offset_ > size ||
          static_cast<size_t>(length_) > size - offset_) {
        return ForeignBytes{0, nullptr};
      }
      return ForeignBytes{length_, data + offset_};
    } catch (...) {
      return ForeignBytes{0, nullptr};
    }
  }

private:
  std::optional<jsi::ArrayBuffer> buffer_;
  size_t offset_ = 0;
  int32_t length_ = 0;
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
          length < 0 || std::floor(length) != length) {
        throw jsi::JSError(rt, "A `&[u8]` argument has an invalid byteOffset "
                               "or byteLength");
      }
      if (length > std::numeric_limits<int32_t>::max()) {
        throw jsi::JSError(rt,
                           "A `&[u8]` argument is longer than i32::MAX bytes");
      }
      if (length == 0) {
        // Also a detached view: JS reports `byteLength` 0 for it. Do not read
        // the buffer, because Hermes throws for a detached buffer.
        return BorrowedBytes();
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
