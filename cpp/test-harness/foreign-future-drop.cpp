/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#include <cassert>
#include <deque>
#include <iostream>
#include <hermes/hermes.h>
namespace jsi = facebook::jsi;
#include "UniffiJsiTypes.h"
extern "C" RustBuffer ffi_async_callbacks_rustbuffer_from_bytes(ForeignBytes, RustCallStatus *);
#define ffi_async_callbacks_rustbuffer_from_bytes tracked_rustbuffer_from_bytes
#ifndef GENERATED_WRAPPER
#error Set GENERATED_WRAPPER to generated async_callbacks.cpp
#endif
#include GENERATED_WRAPPER
#undef ffi_async_callbacks_rustbuffer_from_bytes
static unsigned callbackBufferAllocations = 0;
extern "C" RustBuffer tracked_rustbuffer_from_bytes(ForeignBytes bytes, RustCallStatus *status) {
  ++callbackBufferAllocations;
  return ffi_async_callbacks_rustbuffer_from_bytes(bytes, status);
}
class Queue : public facebook::react::CallInvoker {
public:
  std::deque<facebook::react::CallFunc> tasks;
  void invokeAsync(facebook::react::CallFunc &&f) noexcept override { tasks.push_back(std::move(f)); }
  void invokeSync(facebook::react::CallFunc &&) override { std::abort(); }
  void drain(jsi::Runtime &rt) {
    while (!tasks.empty()) { auto f = std::move(tasks.front()); tasks.pop_front(); f(rt); }
  }
};
static void eval(jsi::Runtime &rt, const std::string &source) {
  rt.evaluateJavaScript(std::make_shared<jsi::StringBuffer>(source), "foreign-future-drop.js");
}
static void setup(jsi::Runtime &rt, std::shared_ptr<Queue> queue) {
  NativeAsyncCallbacks::registerModule(rt, queue);
  eval(rt, R"JS(
    var m = NativeAsyncCallbacks;
    var calls = 0;
    var drops = 0;
    var token = function() { return __ubrnHandleAllocator.allocate('rust-future-continuation'); };
    var continuation = function() {};
    var pending = function(handle, delay, value, complete, data) {
      calls++;
      globalThis.finish = function() { complete(data, {return_value: new Uint8Array([111,107]), call_status: {code:0}}); };
      return {handle: __ubrnHandleAllocator.allocate('foreign-future-drop'), free: function(handle) { drops++; __ubrnHandleAllocator.release(handle); }};
    };
    m.ubrn_uniffi_async_callbacks_fn_init_callback_vtable_asyncparser({
      uniffi_free: function(handle) { __ubrnHandleAllocator.release(handle); },
      uniffi_clone: function(handle) { return handle; },
      as_string: pending, try_from_string: pending, delay: pending, try_delay: pending
    });
  )JS");
}
static void rustReceiverDropDoesNotAllocateError() {
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  setup(*rt, queue);
  auto generation = uniffi_runtime::callbackGeneration(*rt);
  unsigned before = callbackBufferAllocations;
  for (unsigned i = 0; i < 64; ++i) {
    eval(*rt, R"JS(
      var callback = __ubrnHandleAllocator.allocate('callback');
      var future = m.ubrn_uniffi_async_callbacks_fn_func_as_string_using_trait(callback, 0, 42);
      m.ubrn_ffi_async_callbacks_rust_future_poll_rust_buffer(future, continuation, token());
    )JS");
    assert(generation->pending.size() == 1); // Real Rust oneshot receiver is awaiting JS.
    eval(*rt, "m.ubrn_ffi_async_callbacks_rust_future_free_rust_buffer(future)");
    assert(generation->pending.empty());
    queue->drain(*rt);
    eval(*rt, "finish(); finish()"); // Late results cannot consume the real Rust sender twice.
  }
  assert(callbackBufferAllocations == before);
  eval(*rt, "if (calls !== 64 || drops !== 64) throw new Error('foreign callback lifecycle did not balance')");
  NativeAsyncCallbacks::unregisterModule(*rt);
}
static void unavailableRuntimeProducesRealTypedFailure() {
  auto old = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  setup(*old, queue);
  auto oldGeneration = uniffi_runtime::callbackGeneration(*old);
  auto stale = uniffi_runtime::HandleRegistry::instance().allocate(oldGeneration, "callback");
  NativeAsyncCallbacks::unregisterModule(*old);
  old.reset();
  auto rt = facebook::hermes::makeHermesRuntime();
  setup(*rt, queue);
  rt->global().setProperty(*rt, "stale", jsi::BigInt::fromUint64(*rt, stale));
  eval(*rt, R"JS(
    var future = m.ubrn_uniffi_async_callbacks_fn_func_try_from_string_using_trait(stale, 0, new Uint8Array([52,50]));
    m.ubrn_ffi_async_callbacks_rust_future_poll_i32(future, continuation, token());
    var status = {};
    m.ubrn_ffi_async_callbacks_rust_future_complete_i32(future, status);
    if (status.code !== 1 || status.errorBuf[3] !== 2) throw new Error('Rust did not receive ParserError.UnexpectedError');
    if (calls !== 0) throw new Error('stale callback reached the replacement runtime');
    m.ubrn_ffi_async_callbacks_rust_future_free_i32(future);
  )JS");
  queue->drain(*rt);
  NativeAsyncCallbacks::unregisterModule(*rt);
}
int main() {
  rustReceiverDropDoesNotAllocateError();
  unavailableRuntimeProducesRealTypedFailure();
  std::cout << "real Rust foreign future drop tests passed\n";
}
