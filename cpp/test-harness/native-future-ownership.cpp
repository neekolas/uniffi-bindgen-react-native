/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#include <cassert>
#include <deque>
#include <iostream>
#include <set>
#include <hermes/hermes.h>
#ifndef GENERATED_WRAPPER
#error Set GENERATED_WRAPPER to generated arc_futures.cpp
#endif
#include GENERATED_WRAPPER

class Queue : public facebook::react::CallInvoker {
public:
  std::deque<facebook::react::CallFunc> tasks;
  void invokeAsync(facebook::react::CallFunc &&f) noexcept override { tasks.push_back(std::move(f)); }
  void invokeSync(facebook::react::CallFunc &&) override { std::abort(); }
  void drain(facebook::jsi::Runtime &rt) {
    while (!tasks.empty()) { auto f = std::move(tasks.front()); tasks.pop_front(); f(rt); }
  }
};
static void eval(jsi::Runtime &rt, const std::string &source) {
  rt.evaluateJavaScript(std::make_shared<jsi::StringBuffer>(source), "native-future-ownership.js");
}
static uint32_t live() {
  RustCallStatus status{};
  auto count = uniffi_arc_futures_fn_func_get_live_objects(&status);
  assert(status.code == 0);
  return count;
}
static void setup(jsi::Runtime &rt, std::shared_ptr<Queue> queue) {
  NativeArcFutures::registerModule(rt, queue);
  eval(rt, R"JS(
    var m = NativeArcFutures;
    var token = function() { return __ubrnHandleAllocator.allocate('rust-future-continuation'); };
    var continuation = function() {};
  )JS");
}
static void readyResultIsDiscarded(const char *factory, const char *type, unsigned expected) {
  assert(live() == 0);
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  setup(*rt, queue);
  eval(*rt, std::string("var handle = m.ubrn_uniffi_arc_futures_fn_func_") + factory + "();"
       "var poll = m.ubrn_ffi_arc_futures_rust_future_poll_" + type + ";"
       "var cancel = m.ubrn_ffi_arc_futures_rust_future_cancel_" + type + ";"
       "var complete = m.ubrn_ffi_arc_futures_rust_future_complete_" + type + ";"
       "var free = m.ubrn_ffi_arc_futures_rust_future_free_" + type + ";"
       "poll(handle, continuation, token());");
  assert(live() == expected); // Rust lowered a READY result, JS has not completed it.
  NativeArcFutures::unregisterModule(*rt);
  assert(live() == 0);
  eval(*rt, R"JS(
    var rejected = 0;
    try { poll(handle, continuation, BigInt(0)); } catch (e) { rejected++; }
    try { cancel(handle); } catch (e) { rejected++; }
    try { complete(handle, {}); } catch (e) { rejected++; }
    if (rejected !== 3) throw new Error('retired future reached native Rust');
    free(handle); free(handle);
  )JS");
  queue->drain(*rt);
  rt.reset(); // Destroy an actual generated module and its runtime.
  assert(live() == 0);
}
static void completedResultRemainsOwnedByCaller() {
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  setup(*rt, queue);
  eval(*rt, R"JS(
    var handle = m.ubrn_uniffi_arc_futures_fn_func_make_object();
    m.ubrn_ffi_arc_futures_rust_future_poll_u64(handle, continuation, token());
    var object = m.ubrn_ffi_arc_futures_rust_future_complete_u64(handle, {});
    m.ubrn_ffi_arc_futures_rust_future_free_u64(handle);
  )JS");
  assert(live() == 1);
  eval(*rt, "m.ubrn_uniffi_arc_futures_fn_free_simpleobject(object, {})");
  assert(live() == 0);
  NativeArcFutures::unregisterModule(*rt);
  queue->drain(*rt);
}
static void reentrantRetirementDoesNotFreeRunningPoll() {
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  setup(*rt, queue);
  rt->global().setProperty(*rt, "retireRuntime", jsi::Function::createFromHostFunction(*rt,
      jsi::PropNameID::forAscii(*rt, "retireRuntime"), 0,
      [](jsi::Runtime &rt, const jsi::Value &, const jsi::Value *, size_t) {
        NativeArcFutures::unregisterModule(rt);
        return jsi::Value::undefined();
      }));
  eval(*rt, R"JS(
    var releaseBuffer = m.rustbuffer_free;
    var releaseObject = m.ubrn_uniffi_arc_futures_fn_free_simpleobject;
    var calls = 0;
    m.ubrn_uniffi_arc_futures_fn_init_callback_vtable_simplecallback({
      uniffi_free: function(handle) { __ubrnHandleAllocator.release(handle); },
      uniffi_clone: function(handle) { return handle; },
      on_update: function(handle, previous, current) {
        releaseBuffer(previous); releaseBuffer(current);
        calls++;
        retireRuntime();
        return {code: 0};
      }
    });
    var callback = __ubrnHandleAllocator.allocate('callback');
    var create = m.ubrn_uniffi_arc_futures_fn_func_make_object_with_callback(callback);
    m.ubrn_ffi_arc_futures_rust_future_poll_u64(create, continuation, token());
    var object = m.ubrn_ffi_arc_futures_rust_future_complete_u64(create, {});
    m.ubrn_ffi_arc_futures_rust_future_free_u64(create);
    var cloned = m.ubrn_uniffi_arc_futures_fn_clone_simpleobject(object, {});
    var update = m.ubrn_uniffi_arc_futures_fn_method_simpleobject_update(cloned, new Uint8Array([110,101,119]));
    m.ubrn_ffi_arc_futures_rust_future_poll_void(update, continuation, token());
    if (calls !== 1) throw new Error('Rust did not call the reentrant JS callback');
    releaseObject(object, {});
  )JS");
  assert(live() == 0);
  queue->drain(*rt);
}
#ifdef NATIVE_OBJECT_POLICY
namespace native_policy {
static std::set<uint64_t> pending;
static uint64_t original = 0, retained = 0;
static unsigned completions = 0, frees = 0, discards = 0;
void started(Invoker, uint64_t handle) { assert(pending.insert(handle).second); }
void completed(Invoker, uint64_t handle, uint64_t pointer, int8_t status) {
  if (!pending.erase(handle) || status != 0) return;
  ++completions;
  RustCallStatus callStatus{};
  retained = uniffi_arc_futures_fn_clone_simpleobject(pointer, &callStatus);
  original = pointer;
  assert(callStatus.code == 0 && retained != 0);
}
void freed(Invoker, uint64_t handle) { ++frees; pending.erase(handle); }
void discard(uint64_t pointer) {
  ++discards;
  RustCallStatus status{};
  uniffi_arc_futures_fn_free_simpleobject(pointer, &status);
  assert(status.code == 0);
}
}
static void policyRetainsBeforeJsStatusAndDiscardsUnreturnedResults() {
  auto rt = facebook::hermes::makeHermesRuntime();
  auto queue = std::make_shared<Queue>();
  setup(*rt, queue);
  eval(*rt, R"JS(
    var handle = m.ubrn_uniffi_arc_futures_fn_func_make_object();
    m.ubrn_ffi_arc_futures_rust_future_poll_u64(handle, continuation, token());
  )JS");
  assert(native_policy::pending.size() == 1 && live() == 1);
  eval(*rt, R"JS(
    var failed = false;
    try { m.ubrn_ffi_arc_futures_rust_future_complete_u64(handle, undefined); }
    catch (e) { failed = true; }
    if (!failed) throw new Error('status lifting must fail before JS adoption');
    m.ubrn_ffi_arc_futures_rust_future_free_u64(handle);
  )JS");
  assert(native_policy::completions == 1 && native_policy::frees == 1);
  assert(native_policy::pending.empty() && native_policy::retained != 0);
  RustCallStatus status{};
  uniffi_arc_futures_fn_free_simpleobject(native_policy::original, &status);
  assert(live() == 1); // The policy retained the object before JavaScript threw.
  uniffi_arc_futures_fn_free_simpleobject(native_policy::retained, &status);
  assert(live() == 0);
  eval(*rt, R"JS(
    var lateFree = m.ubrn_ffi_arc_futures_rust_future_free_u64;
    handle = m.ubrn_uniffi_arc_futures_fn_func_make_object();
    m.ubrn_ffi_arc_futures_rust_future_poll_u64(handle, continuation, token());
  )JS");
  assert(live() == 1);
  NativeArcFutures::unregisterModule(*rt);
  assert(native_policy::discards == 1 && live() == 0);
  eval(*rt, "lateFree(handle)");
  assert(native_policy::pending.empty());
  queue->drain(*rt);
}
#endif
int main() {
#ifdef NATIVE_OBJECT_POLICY
  policyRetainsBeforeJsStatusAndDiscardsUnreturnedResults();
#else
  readyResultIsDiscarded("make_object", "u64", 1);
  readyResultIsDiscarded("make_nested_objects", "rust_buffer", 2);
  readyResultIsDiscarded("throw_object", "void", 1);
  completedResultRemainsOwnedByCaller();
  reentrantRetirementDoesNotFreeRunningPoll();
#endif
  std::cout << "real Rust native future ownership tests passed\n";
}
