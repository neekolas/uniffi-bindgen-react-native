/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */
#pragma once
#include "UniffiCallInvoker.h"
#include <atomic>
#include <cstdint>
#include <functional>
#include <limits>
#include <unordered_map>
#include <utility>

namespace uniffi_runtime {
// One generation owns JS values. invalidate() is called on its JS thread before
// runtime destruction; native handle tombstones never retain those values.
struct CallbackGeneration;
inline void retireCallbackGeneration(CallbackGeneration *generation);

struct PendingCompletion {
  virtual ~PendingCompletion() = default;
  virtual void cancel() noexcept = 0;
  virtual void abandon() noexcept { cancel(); }
};

// A Rust poll may synchronously reenter JS and retire its generation. Hold a
// lease across every FFI operation so retirement never frees a future in use.
class NativeFuture final {
  std::mutex mutex_;
  size_t leases_ = 0;
  bool retired_ = false;
  bool completed_ = false;
  std::function<void(bool)> dispose_;
  void release() noexcept {
    std::function<void(bool)> dispose;
    bool completed = false;
    {
      std::lock_guard<std::mutex> lock(mutex_);
      --leases_;
      if (retired_ && leases_ == 0) {
        dispose = std::move(dispose_);
        completed = completed_;
      }
    }
    if (dispose) dispose(completed);
  }
public:
  explicit NativeFuture(std::function<void(bool)> dispose)
      : dispose_(std::move(dispose)) {}
  class Lease {
    std::shared_ptr<NativeFuture> future_;
  public:
    explicit Lease(std::shared_ptr<NativeFuture> future = {})
        : future_(std::move(future)) {}
    Lease(const Lease &) = delete;
    Lease(Lease &&) = default;
    ~Lease() { if (future_) future_->release(); }
    explicit operator bool() const { return bool(future_); }
    void completed() {
      std::lock_guard<std::mutex> lock(future_->mutex_);
      future_->completed_ = true;
    }
  };
  static Lease acquire(const std::shared_ptr<NativeFuture> &future) {
    std::lock_guard<std::mutex> lock(future->mutex_);
    if (future->retired_ || future->completed_) return Lease();
    ++future->leases_;
    return Lease(future);
  }
  void retire() noexcept {
    std::function<void(bool)> dispose;
    bool completed = false;
    {
      std::lock_guard<std::mutex> lock(mutex_);
      retired_ = true;
      if (leases_ == 0) {
        dispose = std::move(dispose_);
        completed = completed_;
      }
    }
    if (dispose) dispose(completed);
  }
};

struct CallbackGeneration {
  facebook::jsi::Runtime *runtime;
  std::shared_ptr<UniffiCallInvoker> invoker;
  std::unordered_map<const void *, facebook::jsi::Value> callbacks;
  std::vector<std::function<void()>> clearModules;
  std::mutex pendingMutex;
  std::unordered_map<const void *, std::shared_ptr<PendingCompletion>> pending;
  std::unordered_map<uint64_t, std::shared_ptr<NativeFuture>> rustFutures;
  // Generation membership and active are accessed only on the JS thread.
  NativeFuture::Lease acquireFuture(uint64_t handle) {
    auto found = rustFutures.find(handle);
    if (!active || found == rustFutures.end()) return NativeFuture::Lease();
    return NativeFuture::acquire(found->second);
  }
  void retireFuture(uint64_t handle) {
    auto found = rustFutures.find(handle);
    if (found == rustFutures.end()) return;
    auto future = std::move(found->second);
    rustFutures.erase(found);
    future->retire();
  }
  bool active = true; // JS-thread only
  CallbackGeneration(facebook::jsi::Runtime &rt,
                     std::shared_ptr<UniffiCallInvoker> ci)
      : runtime(&rt), invoker(std::move(ci)) {}
  void invalidate() {
    if (!active)
      return;
    active = false;
    retireCallbackGeneration(this);
    invoker->invalidate();
    callbacks.clear();
    auto modules = std::move(clearModules);
    for (auto &clear : modules) clear();
    auto futures = std::move(rustFutures);
    for (auto &future : futures)
      future.second->retire();
    std::unordered_map<const void *, std::shared_ptr<PendingCompletion>>
        cancelled;
    {
      std::lock_guard<std::mutex> lock(pendingMutex);
      cancelled.swap(pending);
    }
    for (auto &item : cancelled)
      item.second->cancel();
  }
};

class HandleRegistry {
  struct Entry {
    std::weak_ptr<CallbackGeneration> owner;
    std::string kind;
    std::shared_ptr<PendingCompletion> completion;
    uint64_t references = 1;
    uint64_t leases = 0;
  };
  std::mutex mutex_;
  std::unordered_map<uint64_t, Entry> entries_;
  std::vector<uint64_t> available_;
  uint64_t next_ = 1;
  void collect(std::unordered_map<uint64_t, Entry>::iterator it) {
    if (it->second.references == 0 && it->second.leases == 0) {
      available_.push_back(it->first);
      entries_.erase(it);
    }
  }

public:
  static HandleRegistry &instance() {
    static HandleRegistry registry;
    return registry;
  }
  class Lease {
    HandleRegistry *registry_ = nullptr;
    uint64_t token_ = 0;

  public:
    std::shared_ptr<CallbackGeneration> owner;
    Lease() = default;
    Lease(HandleRegistry &registry, uint64_t token,
          std::shared_ptr<CallbackGeneration> generation)
        : registry_(&registry), token_(token), owner(std::move(generation)) {}
    Lease(const Lease &) = delete;
    Lease &operator=(const Lease &) = delete;
    Lease(Lease &&other) noexcept
        : registry_(std::exchange(other.registry_, nullptr)),
          token_(other.token_), owner(std::move(other.owner)) {}
    ~Lease() {
      if (registry_)
        registry_->releaseLease(token_);
    }
    explicit operator bool() const { return owner != nullptr; }
  };
  uint64_t allocate(std::shared_ptr<CallbackGeneration> owner,
                    std::string kind) {
    std::lock_guard<std::mutex> lock(mutex_);
    uint64_t token;
    if (!available_.empty()) {
      token = available_.back();
      available_.pop_back();
    } else {
      if (next_ == std::numeric_limits<uint64_t>::max())
        throw std::overflow_error("UniFFI callback handles exhausted");
      token = next_;
      next_ += 2;
    }
    entries_.emplace(token, Entry{std::move(owner), std::move(kind)});
    return token;
  }
  Lease acquire(uint64_t token, const char *kind) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = entries_.find(token);
    if (it == entries_.end() || it->second.references == 0 ||
        it->second.kind != kind)
      return {};
    ++it->second.leases;
    return Lease(*this, token, it->second.owner.lock());
  }
  bool owns(uint64_t token, const std::shared_ptr<CallbackGeneration> &owner) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = entries_.find(token);
    return it != entries_.end() && it->second.references != 0 &&
           it->second.owner.lock() == owner;
  }
  void attach(uint64_t token, std::shared_ptr<PendingCompletion> completion) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = entries_.find(token);
    if (it != entries_.end())
      it->second.completion = std::move(completion);
  }
  void cancel(uint64_t token) {
    std::shared_ptr<PendingCompletion> completion;
    {
      std::lock_guard<std::mutex> lock(mutex_);
      auto it = entries_.find(token);
      if (it != entries_.end())
        completion = std::move(it->second.completion);
    }
    if (completion)
      completion->abandon();
  }
  uint64_t retain(uint64_t token) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = entries_.find(token);
    if (it == entries_.end() || it->second.references == 0)
      return 0;
    ++it->second.references;
    return token;
  }
  void release(uint64_t token) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = entries_.find(token);
    if (it == entries_.end() || it->second.references == 0)
      return;
    --it->second.references;
    collect(it);
  }
  void releaseLease(uint64_t token) {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = entries_.find(token);
    if (it == entries_.end())
      return;
    --it->second.leases;
    collect(it);
  }
};

// A no-output callback owns one retirement. It survives queue cancellation,
// and recognizes the corresponding JS map release without dropping twice.
class HandleRetirement {
  static inline thread_local HandleRetirement *current_ = nullptr;
  HandleRetirement *previous_ = nullptr;
  uint64_t token_;
  HandleRegistry::Lease lease_;
  bool released_ = false;

public:
  HandleRetirement(uint64_t token, HandleRegistry::Lease lease)
      : token_(token), lease_(std::move(lease)) {}
  ~HandleRetirement() {
    if (!released_)
      HandleRegistry::instance().release(token_);
  }
  template <class F> void run(F &&function) {
    previous_ = current_;
    current_ = this;
    try {
      function();
    } catch (...) {
      current_ = previous_;
      throw;
    }
    current_ = previous_;
  }
  static void releaseFromJs(uint64_t token) {
    for (auto scope = current_; scope; scope = scope->previous_) {
      if (scope->token_ == token && !scope->released_) {
        scope->released_ = true;
        break;
      }
    }
    HandleRegistry::instance().release(token);
  }
};

template <class Callback, class Result>
struct ForeignCompletion final : PendingCompletion {
  std::atomic<bool> consumed{false};
  Callback callback;
  uint64_t data;
  std::function<Result()> failure;
  std::weak_ptr<CallbackGeneration> owner;
  ForeignCompletion(Callback cb, uint64_t d, std::function<Result()> fail,
                    std::shared_ptr<CallbackGeneration> generation)
      : callback(cb), data(d), failure(std::move(fail)), owner(generation) {}
  bool complete(Result result) noexcept {
    if (consumed.exchange(true))
      return false;
    callback(data, result);
    if (auto generation = owner.lock()) {
      std::lock_guard<std::mutex> lock(generation->pendingMutex);
      generation->pending.erase(this);
    }
    return true;
  }
  void cancel() noexcept override { complete(failure()); }
  void abandon() noexcept override {
    // The Rust receiver is being dropped. Consume its sender without allocating
    // a failure buffer that the discarded channel value could not release.
    complete(Result{});
  }

};

inline auto &runtimeGenerations() {
  static std::unordered_map<facebook::jsi::Runtime *,
                            std::weak_ptr<CallbackGeneration>>
      values;
  return values; // All access is serialized by runtimeGenerationMutex().
}
inline std::mutex &runtimeGenerationMutex() {
  static std::mutex mutex;
  return mutex;
}
inline void retireCallbackGeneration(CallbackGeneration *generation) {
  std::lock_guard<std::mutex> lock(runtimeGenerationMutex());
  auto it = runtimeGenerations().find(generation->runtime);
  if (it != runtimeGenerations().end() && it->second.lock().get() == generation)
    runtimeGenerations().erase(it);
}
inline std::shared_ptr<CallbackGeneration>
callbackGeneration(facebook::jsi::Runtime &rt) {
  std::lock_guard<std::mutex> lock(runtimeGenerationMutex());
  auto it = runtimeGenerations().find(&rt);
  if (it == runtimeGenerations().end())
    throw std::logic_error("UniFFI runtime is not registered");
  auto owner = it->second.lock();
  if (!owner || !owner->active)
    throw std::logic_error("UniFFI runtime is stopped");
  return owner;
}
inline void invalidateCallbacks(
    facebook::jsi::Runtime &rt,
    const std::shared_ptr<UniffiCallInvoker> &expected = nullptr) {
  std::shared_ptr<CallbackGeneration> owner;
  {
    std::lock_guard<std::mutex> lock(runtimeGenerationMutex());
    auto it = runtimeGenerations().find(&rt);
    if (it == runtimeGenerations().end())
      return;
    owner = it->second.lock();
  }
  if (owner && (!expected || owner->invoker == expected))
    owner->invalidate();
}

// Host functions and callback slots all capture the same runtime generation.
// No process-global function ever points at a replacement runtime.
inline std::shared_ptr<CallbackGeneration>
installHandleAllocator(facebook::jsi::Runtime &rt,
                       std::shared_ptr<UniffiCallInvoker> invoker) {
  namespace jsi = facebook::jsi;
  std::shared_ptr<CallbackGeneration> generation;
  {
    std::lock_guard<std::mutex> generationLock(runtimeGenerationMutex());
    if (auto existing = runtimeGenerations()[&rt].lock()) {
      if (existing->active)
        return existing;
    }
    generation = std::make_shared<CallbackGeneration>(rt, std::move(invoker));
    runtimeGenerations()[&rt] = generation;
  }
  jsi::Object allocator(rt);
  allocator.setProperty(
      rt, "allocate",
      jsi::Function::createFromHostFunction(
          rt, jsi::PropNameID::forAscii(rt, "allocate"), 1,
          [generation](jsi::Runtime &rt, const jsi::Value &,
                       const jsi::Value *args, size_t count) {
            if (!generation->active || count != 1 || !args[0].isString())
              throw jsi::JSError(rt, "Invalid callback allocation");
            auto kind = args[0].asString(rt).utf8(rt);
            if (kind != "callback" && kind != "rust-future-continuation" &&
                kind != "foreign-future-drop")
              throw jsi::JSError(rt, "Unknown callback handle kind");
            return jsi::Value(
                jsi::BigInt::fromUint64(rt, HandleRegistry::instance().allocate(
                                                generation, std::move(kind))));
          }));
  allocator.setProperty(
      rt, "release",
      jsi::Function::createFromHostFunction(
          rt, jsi::PropNameID::forAscii(rt, "release"), 1,
          [generation](jsi::Runtime &rt, const jsi::Value &,
                       const jsi::Value *args, size_t count) {
            if (count != 1 || !args[0].isBigInt())
              throw jsi::JSError(rt, "Invalid callback handle");
            auto token = args[0].asBigInt(rt).asUint64(rt);
            if (HandleRegistry::instance().owns(token, generation))
              HandleRetirement::releaseFromJs(token);
            return jsi::Value::undefined();
          }));
  allocator.setProperty(
      rt, "owns",
      jsi::Function::createFromHostFunction(
          rt, jsi::PropNameID::forAscii(rt, "owns"), 1,
          [generation](jsi::Runtime &rt, const jsi::Value &,
                       const jsi::Value *args, size_t count) {
            return jsi::Value(
                generation->active && count == 1 && args[0].isBigInt() &&
                HandleRegistry::instance().owns(
                    args[0].asBigInt(rt).asUint64(rt), generation));
          }));
  rt.global().setProperty(rt, "__ubrnHandleAllocator", allocator);
  return generation;
}
} // namespace uniffi_runtime
