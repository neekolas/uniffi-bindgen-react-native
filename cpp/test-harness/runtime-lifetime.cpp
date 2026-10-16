/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
#include "UniffiCallInvoker.h"
#include "UniffiHandleRegistry.h"
#ifdef UBRN_TEST_JSC
#include <jsc/JSCRuntime.h>
#else
#include <hermes/hermes.h>
#endif
#include <chrono>
#include <deque>
#include <future>
#include <iostream>
#include <fstream>
#include <sstream>

using namespace std::chrono_literals;
namespace jsi = facebook::jsi;
using uniffi_runtime::UniffiCallInvoker;

static void require(bool condition, const char *message) {
  if (!condition) {
    std::cerr << message << std::endl;
    std::abort();
  }
}

static std::unique_ptr<jsi::Runtime> makeRuntime() {
#ifdef UBRN_TEST_JSC
  return facebook::jsc::makeJSCRuntime();
#else
  return facebook::hermes::makeHermesRuntime();
#endif
}

class Queue : public facebook::react::CallInvoker {
  std::mutex mutex;
  std::condition_variable available;
  std::deque<facebook::react::CallFunc> tasks;
public:
  void invokeAsync(facebook::react::CallFunc &&func) noexcept override {
    std::lock_guard<std::mutex> lock(mutex);
    tasks.push_back(std::move(func));
    available.notify_all();
  }
  void invokeSync(facebook::react::CallFunc &&) override { std::abort(); }
  void waitForTask() {
    std::unique_lock<std::mutex> lock(mutex);
    require(available.wait_for(lock, 2s, [&] { return !tasks.empty(); }),
            "native worker did not enqueue its callback");
  }
  void drain(jsi::Runtime &rt) {
    while (true) {
      facebook::react::CallFunc task;
      {
        std::lock_guard<std::mutex> lock(mutex);
        if (tasks.empty()) return;
        task = std::move(tasks.front());
        tasks.pop_front();
      }
      task(rt);
    }
  }
};

static void queuedCapturesAreReleasedOnJsThread() {
  auto rt = makeRuntime();
  auto queue = std::make_shared<Queue>();
  UniffiCallInvoker invoker(queue);
  auto jsThread = std::this_thread::get_id();
  bool released = false;
  auto value = std::shared_ptr<jsi::Value>(new jsi::Value(rt->global()),
    [&](jsi::Value *value) {
      require(std::this_thread::get_id() == jsThread,
              "JS capture was destroyed off the JS thread");
      delete value;
      released = true;
    });
  bool called = false;
  require(invoker.invokeNonBlocking(*rt, [value, &called](jsi::Runtime &runtime) {
    called = true;
    value->asObject(runtime).setProperty(runtime, "oldCallback", true);
  }), "active generation rejected work");
  value.reset();
  std::thread stopper([&] { invoker.stop(); });
  stopper.join();
  require(!released, "stop destroyed JS captures on the stopping thread");
  invoker.invalidate();
  require(released, "invalidate retained JS captures past runtime teardown");
  rt.reset();
  auto replacement = makeRuntime();
  queue->drain(*replacement);
  require(!called, "cancelled callback touched a destroyed runtime");
  require(!replacement->global().hasProperty(*replacement, "oldCallback"),
          "old generation callback reached replacement runtime");
}

static void pendingWorkerReturnsBeforeQueueDrains() {
  auto rt = makeRuntime();
  auto queue = std::make_shared<Queue>();
  UniffiCallInvoker invoker(queue);
  auto worker = std::async(std::launch::async, [&] {
    int output = 41;
    bool completed = invoker.invokeBlocking(*rt, [&](jsi::Runtime &runtime) {
      output = runtime.global().getProperty(runtime, "answer").asNumber();
    });
    require(output == 41, "cancelled call changed its native output");
    return completed;
  });
  queue->waitForTask();
  invoker.stop();
  require(worker.wait_for(2s) == std::future_status::ready,
          "stopped generation left a native worker blocked");
  require(!worker.get(), "cancelled call reported completion");
  invoker.invalidate();
  rt.reset();
  auto replacement = makeRuntime();
  queue->drain(*replacement);
}

static void runningWorkerKeepsItsOutputAlive() {
  auto rt = makeRuntime();
  rt->global().setProperty(*rt, "answer", 42);
  auto queue = std::make_shared<Queue>();
  UniffiCallInvoker invoker(queue);
  std::promise<void> entered;
  std::promise<void> stopped;
  auto stoppedFuture = stopped.get_future();
  auto worker = std::async(std::launch::async, [&] {
    int output = 0;
    bool completed = invoker.invokeBlocking(*rt, [&](jsi::Runtime &runtime) {
      entered.set_value();
      stoppedFuture.wait();
      output = runtime.global().getProperty(runtime, "answer").asNumber();
    });
    require(completed && output == 42, "running callback lost its output");
  });
  std::thread stopper([&] {
    entered.get_future().wait();
    invoker.stop();
    require(worker.wait_for(20ms) == std::future_status::timeout,
            "stop released a worker while JS still used its output");
    stopped.set_value();
  });
  queue->waitForTask();
  queue->drain(*rt);
  stopper.join();
  worker.get();
  invoker.invalidate();
}

static void errorsReleaseWorkersAndStoppedWorkIsRejected() {
  auto rt = makeRuntime();
  auto queue = std::make_shared<Queue>();
  UniffiCallInvoker invoker(queue);
  bool direct = false;
  require(invoker.invokeBlocking(*rt, [&](jsi::Runtime &) { direct = true; }),
          "direct call did not complete");
  require(direct, "direct call was not invoked");
  auto worker = std::async(std::launch::async, [&] {
    try {
      invoker.invokeBlocking(*rt, [](jsi::Runtime &runtime) {
        throw jsi::JSError(runtime, "lifetime-test-error");
      });
    } catch (const std::runtime_error &error) {
      return std::string(error.what()).find("lifetime-test-error") != std::string::npos;
    }
    return false;
  });
  queue->waitForTask();
  queue->drain(*rt);
  require(worker.wait_for(2s) == std::future_status::ready,
          "throwing JS callback left its native worker blocked");
  require(worker.get(), "native worker did not receive the callback error");
  invoker.invalidate();
  auto unexpected = [](jsi::Runtime &) { std::abort(); };
  require(!invoker.invokeBlocking(*rt, unexpected), "stopped direct call accepted");
  require(!invoker.invokeNonBlocking(*rt, unexpected), "stopped async call accepted");
  auto rejected = std::async(std::launch::async, [&] {
    return invoker.invokeBlocking(*rt, unexpected);
  });
  require(!rejected.get(), "stopped worker call accepted");
  invoker.invalidate();
}

// RN's JSC adapter does not implement the JSI BigInt host API. Exercise the
// same native registry there; Hermes additionally exercises the actual JS hooks.
static uint64_t allocateFromJs(jsi::Runtime &rt, const char *kind) {
#ifdef UBRN_TEST_JSC
  return uniffi_runtime::HandleRegistry::instance().allocate(
      uniffi_runtime::callbackGeneration(rt), kind);
#else
  auto allocator = rt.global().getPropertyAsObject(rt, "__ubrnHandleAllocator");
  auto allocate = allocator.getPropertyAsFunction(rt, "allocate");
  return allocate.call(rt, jsi::String::createFromUtf8(rt, kind)).asBigInt(rt).asUint64(rt);
#endif
}

static bool ownsFromJs(jsi::Runtime &rt, uint64_t handle) {
#ifdef UBRN_TEST_JSC
  try {
    return uniffi_runtime::HandleRegistry::instance().owns(
        handle, uniffi_runtime::callbackGeneration(rt));
  } catch (const std::logic_error &) { return false; }
#else
  auto allocator = rt.global().getPropertyAsObject(rt, "__ubrnHandleAllocator");
  return allocator.getPropertyAsFunction(rt, "owns").call(
      rt, jsi::BigInt::fromUint64(rt, handle)).getBool();
#endif
}

static void releaseFromJs(jsi::Runtime &rt, uint64_t handle) {
#ifdef UBRN_TEST_JSC
  if (ownsFromJs(rt, handle)) uniffi_runtime::HandleRegistry::instance().release(handle);
#else
  auto allocator = rt.global().getPropertyAsObject(rt, "__ubrnHandleAllocator");
  allocator.getPropertyAsFunction(rt, "release").call(rt, jsi::BigInt::fromUint64(rt, handle));
#endif
}

static void handlesRemainBoundToTheirOriginalRuntime() {
  using namespace uniffi_runtime;
  auto &registry = HandleRegistry::instance();
  auto oldRuntime = makeRuntime();
  auto oldQueue = std::make_shared<Queue>();
  auto oldInvoker = std::make_shared<UniffiCallInvoker>(oldQueue);
  auto oldOwner = installHandleAllocator(*oldRuntime, oldInvoker);
  std::weak_ptr<CallbackGeneration> retiredOwner = oldOwner;
  const auto callback = allocateFromJs(*oldRuntime, "callback");
  const auto continuation = allocateFromJs(*oldRuntime, "rust-future-continuation");
  const auto foreignFuture = allocateFromJs(*oldRuntime, "foreign-future-drop");
  require(callback % 2 == 1 && continuation % 2 == 1 && foreignFuture % 2 == 1,
          "foreign handles must be odd");
  require(callback != continuation && callback != foreignFuture && continuation != foreignFuture,
          "different callback maps received the same handle");
  require(ownsFromJs(*oldRuntime, callback), "active runtime does not own its callback");
  require(!registry.acquire(callback, "rust-future-continuation"),
          "callback token was accepted as a future continuation");
  invalidateCallbacks(*oldRuntime);
  require(!ownsFromJs(*oldRuntime, callback), "stopped runtime still accepts its callback");
  bool rejected = false;
  try { allocateFromJs(*oldRuntime, "callback"); }
  catch (const std::exception &) { rejected = true; }
  require(rejected, "stopped runtime allocated a new callback handle");
  oldRuntime.reset();
  oldOwner.reset();
  require(retiredOwner.expired(), "Rust-held callback tombstones retained the retired runtime");

  auto replacement = makeRuntime();
  auto newQueue = std::make_shared<Queue>();
  auto newInvoker = std::make_shared<UniffiCallInvoker>(newQueue);
  auto newOwner = installHandleAllocator(*replacement, newInvoker);
  const auto live = allocateFromJs(*replacement, "callback");
  require(live != callback && live != continuation && live != foreignFuture,
          "reload reused a handle that Rust still owns");
  require(!ownsFromJs(*replacement, callback), "new runtime adopted an old callback");
  // A new runtime must not be able to release an old runtime's handle.
  releaseFromJs(*replacement, callback);
  require(registry.retain(callback) == callback, "foreign release retired another runtime's handle");
  registry.release(callback); // balance the stale clone
  {
    auto inFlight = registry.acquire(callback, "callback");
    require(!inFlight.owner, "old callback routes to a runtime after its owner was destroyed");
    registry.release(callback);
    const auto duringFlight = allocateFromJs(*replacement, "callback");
    require(duringFlight != callback, "handle reused while an in-flight callback retained it");
    releaseFromJs(*replacement, duringFlight);
  }
  const auto reusable = allocateFromJs(*replacement, "callback");
  require(reusable == callback, "fully released callback token was not reusable");
  registry.release(continuation);
  registry.release(foreignFuture);
  require(retiredOwner.expired(), "late frees retained the retired runtime generation");
  require(ownsFromJs(*replacement, live), "late frees damaged the new runtime's callback");
  releaseFromJs(*replacement, live);
  releaseFromJs(*replacement, reusable);
  invalidateCallbacks(*replacement);
}

static void repeatedReloadsReuseReleasedTokens() {
  using namespace uniffi_runtime;
  uint64_t previous = 0;
  for (unsigned reload = 0; reload < 128; ++reload) {
    auto runtime = makeRuntime();
    auto queue = std::make_shared<Queue>();
    auto invoker = std::make_shared<UniffiCallInvoker>(queue);
    auto owner = installHandleAllocator(*runtime, invoker);
    const auto handle = allocateFromJs(*runtime, "callback");
    if (previous != 0) require(handle == previous, "reload leaks released callback identities");
    previous = handle;
    releaseFromJs(*runtime, handle);
    invalidateCallbacks(*runtime);
  }
}

struct CompletionCounts {
  std::atomic<unsigned> calls{0};
  std::atomic<unsigned> failures{0};
};

static void finishForeignFuture(uint64_t data, int result) {
  auto *counts = reinterpret_cast<CompletionCounts *>(data);
  counts->calls.fetch_add(1);
  if (result == -1) counts->failures.fetch_add(1);
}

static void teardownConsumesPendingFutureOwnershipOnce() {
  using namespace uniffi_runtime;
  auto runtime = makeRuntime();
  auto queue = std::make_shared<Queue>();
  auto owner = installHandleAllocator(*runtime, std::make_shared<UniffiCallInvoker>(queue));
  CompletionCounts completed;
  CompletionCounts cancelled;
  using Completion = ForeignCompletion<decltype(&finishForeignFuture), int>;
  auto first = std::make_shared<Completion>(&finishForeignFuture,
      reinterpret_cast<uint64_t>(&completed), [] { return -1; }, owner);
  auto second = std::make_shared<Completion>(&finishForeignFuture,
      reinterpret_cast<uint64_t>(&cancelled), [] { return -1; }, owner);
  owner->pending.emplace(first.get(), first);
  owner->pending.emplace(second.get(), second);
  unsigned freedRustFutures = 0;
  owner->rustFutures.emplace(2, std::make_shared<NativeFuture>([&](bool completed) {
    require(!completed, "pending future was marked completed");
    ++freedRustFutures;
  }));
  require(first->complete(42), "first future completion was rejected");
  first->cancel();
  invalidateCallbacks(*runtime);
  require(!second->complete(42), "late JavaScript success completed a retired future twice");
  second->cancel();
  invalidateCallbacks(*runtime);
  require(completed.calls == 1 && completed.failures == 0,
          "completed foreign future was completed again during teardown");
  require(cancelled.calls == 1 && cancelled.failures == 1,
          "pending foreign future was not failed exactly once during teardown");
  require(freedRustFutures == 1, "outgoing Rust future was not freed exactly once during teardown");
  require(owner->pending.empty() && owner->rustFutures.empty(),
          "teardown retained pending future ownership");
}

static void reentrantRetirementWaitsForNativeLease() {
  using namespace uniffi_runtime;
  auto runtime = makeRuntime();
  auto queue = std::make_shared<Queue>();
  auto owner = installHandleAllocator(*runtime, std::make_shared<UniffiCallInvoker>(queue));
  unsigned disposed = 0;
  owner->rustFutures.emplace(2, std::make_shared<NativeFuture>([&](bool completed) {
    require(completed, "completed result would have been consumed twice");
    ++disposed;
  }));
  {
    auto lease = owner->acquireFuture(2);
    require(bool(lease), "live future lease was rejected");
    owner->invalidate();
    require(disposed == 0, "reentrant retirement freed a running native future");
    require(!owner->acquireFuture(2), "retired generation allowed another native call");
    lease.completed();
  }
  require(disposed == 1, "final native lease did not release retired future");
  owner->retireFuture(2);
  require(disposed == 1, "late free released native future twice");
}

#ifndef UBRN_TEST_JSC
static void typescriptMapsUseNativeGenerationHooks(const char *scriptPath) {
  using namespace uniffi_runtime;
  std::ifstream input(scriptPath);
  require(input.good(), "TypeScript handle test bundle could not be opened");
  std::ostringstream contents;
  contents << input.rdbuf();
  const auto script = contents.str();
  auto oldRuntime = makeRuntime();
  auto oldQueue = std::make_shared<Queue>();
  auto oldOwner = installHandleAllocator(*oldRuntime, std::make_shared<UniffiCallInvoker>(oldQueue));
  oldRuntime->evaluateJavaScript(std::make_shared<jsi::StringBuffer>(script), scriptPath);
  uint64_t oldHandles[3];
  {
    auto probe = oldRuntime->global().getPropertyAsObject(*oldRuntime, "__ubrnTestHandles");
    oldHandles[0] = probe.getProperty(*oldRuntime, "callback").asBigInt(*oldRuntime).asUint64(*oldRuntime);
    oldHandles[1] = probe.getProperty(*oldRuntime, "continuation").asBigInt(*oldRuntime).asUint64(*oldRuntime);
    oldHandles[2] = probe.getProperty(*oldRuntime, "future").asBigInt(*oldRuntime).asUint64(*oldRuntime);
    probe.getPropertyAsFunction(*oldRuntime, "checkLive").call(*oldRuntime, jsi::BigInt::fromUint64(*oldRuntime, 0));
    invalidateCallbacks(*oldRuntime);
    probe.getPropertyAsFunction(*oldRuntime, "checkStopped").call(*oldRuntime);
  }
  oldRuntime.reset();
  oldOwner.reset();
  auto replacement = makeRuntime();
  auto queue = std::make_shared<Queue>();
  auto owner = installHandleAllocator(*replacement, std::make_shared<UniffiCallInvoker>(queue));
  replacement->evaluateJavaScript(std::make_shared<jsi::StringBuffer>(script), scriptPath);
  {
    auto probe = replacement->global().getPropertyAsObject(*replacement, "__ubrnTestHandles");
    auto check = probe.getPropertyAsFunction(*replacement, "checkLive");
    check.call(*replacement, jsi::BigInt::fromUint64(*replacement, oldHandles[0]));
    for (auto handle : oldHandles) HandleRegistry::instance().release(handle);
    check.call(*replacement, jsi::BigInt::fromUint64(*replacement, oldHandles[0]));
    probe.getPropertyAsFunction(*replacement, "release").call(*replacement);
  }
  invalidateCallbacks(*replacement);
}
#endif

int main(int argc, char **argv) {
  queuedCapturesAreReleasedOnJsThread();
  pendingWorkerReturnsBeforeQueueDrains();
  runningWorkerKeepsItsOutputAlive();
  errorsReleaseWorkersAndStoppedWorkIsRejected();
  handlesRemainBoundToTheirOriginalRuntime();
  repeatedReloadsReuseReleasedTokens();
  teardownConsumesPendingFutureOwnershipOnce();
  reentrantRetirementWaitsForNativeLease();
#ifndef UBRN_TEST_JSC
  if (argc > 1) typescriptMapsUseNativeGenerationHooks(argv[1]);
#endif
  std::cout << "runtime lifetime tests passed" << std::endl;
}
