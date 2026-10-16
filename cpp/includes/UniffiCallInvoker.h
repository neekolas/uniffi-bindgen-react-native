/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
#pragma once
#include <ReactCommon/CallInvoker.h>
#include <algorithm>
#include <condition_variable>
#include <jsi/jsi.h>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

namespace uniffi_runtime {
namespace jsi = facebook::jsi;
namespace react = facebook::react;
using UniffiCallFunc = std::function<void(jsi::Runtime &)>;

/** Dispatches work to one JS runtime generation.
 *
 * Construct and invalidate on the JS thread. The owner must call invalidate()
 * before destroying the runtime. stop() may be called from any thread to reject
 * new work and release workers whose queued calls have not started. A running
 * call must finish before its worker can return: it may reference that worker's
 * native output arguments.
 */
class UniffiCallInvoker {
  enum class Phase { Pending, Running, Complete, Cancelled };
  struct Task {
    explicit Task(UniffiCallFunc f) : func(std::move(f)) {}
    UniffiCallFunc func;
    Phase phase = Phase::Pending;
    std::string error;
    std::condition_variable completed;
  };
  struct State {
    std::mutex mutex;
    bool stopped = false;
    std::vector<std::shared_ptr<Task>> tasks;
  };

  std::shared_ptr<react::CallInvoker> callInvoker_;
  const std::thread::id threadId_;
  std::shared_ptr<State> state_ = std::make_shared<State>();

  static void run(const std::shared_ptr<State> &state,
                  const std::shared_ptr<Task> &task, jsi::Runtime *rt) {
    {
      std::lock_guard<std::mutex> lock(state->mutex);
      if (task->phase != Phase::Pending) {
        return;
      }
      task->phase = Phase::Running;
    }
    // Never hold the state lock while invoking JS or destroying its captures.
    // Both operations can call back into native code.
    std::string error;
    try {
      task->func(*rt);
    } catch (const std::exception &exception) {
      // A JSError can own a JS value. Destroy it on this thread, and forward
      // only its message to the waiting native thread.
      error = exception.what();
      if (error.empty()) {
        error = "JavaScript callback failed";
      }
    } catch (...) {
      error = "JavaScript callback failed";
    }
    task->func = nullptr;
    {
      std::lock_guard<std::mutex> lock(state->mutex);
      task->error = std::move(error);
      task->phase = Phase::Complete;
      auto &tasks = state->tasks;
      tasks.erase(std::remove(tasks.begin(), tasks.end(), task), tasks.end());
    }
    task->completed.notify_all();
  }

  std::shared_ptr<Task> enqueue(jsi::Runtime *rt, UniffiCallFunc func) {
    auto task = std::make_shared<Task>(std::move(func));
    auto state = state_;
    {
      std::lock_guard<std::mutex> lock(state->mutex);
      if (state->stopped) {
        return nullptr;
      }
      state->tasks.push_back(task);
    }
    // Capture task storage by value. stop() can release the waiting worker
    // before the React Native queue runs (or discards) this closure.
    callInvoker_->invokeAsync(
        std::function<void()>([state, task, rt] { run(state, task, rt); }));
    return task;
  }

public:
  explicit UniffiCallInvoker(std::shared_ptr<react::CallInvoker> callInvoker)
      : callInvoker_(std::move(callInvoker)),
        threadId_(std::this_thread::get_id()) {}

  /** Returns false when the generation stopped before the callback started. */
  bool invokeBlocking(jsi::Runtime &rt, UniffiCallFunc func) {
    return invokeBlocking(&rt, std::move(func));
  }
  bool invokeBlocking(jsi::Runtime *rt, UniffiCallFunc func) {
    if (std::this_thread::get_id() == threadId_) {
      {
        std::lock_guard<std::mutex> lock(state_->mutex);
        if (state_->stopped) {
          return false;
        }
      }
      func(*rt);
      return true;
    }
    auto task = enqueue(rt, std::move(func));
    if (!task) {
      return false;
    }
    std::unique_lock<std::mutex> lock(state_->mutex);
    task->completed.wait(lock, [&] {
      return task->phase == Phase::Complete || task->phase == Phase::Cancelled;
    });
    if (!task->error.empty()) {
      throw std::runtime_error(task->error);
    }
    return task->phase == Phase::Complete;
  }

  /** Returns false when the generation has already stopped. */
  bool invokeNonBlocking(jsi::Runtime &rt, UniffiCallFunc func) {
    return invokeNonBlocking(&rt, std::move(func));
  }
  bool invokeNonBlocking(jsi::Runtime *rt, UniffiCallFunc func) {
    return enqueue(rt, std::move(func)) != nullptr;
  }

  /** Thread-safe cancellation. Does not destroy any queued JS captures. */
  void stop() {
    std::lock_guard<std::mutex> lock(state_->mutex);
    state_->stopped = true;
    for (const auto &task : state_->tasks) {
      if (task->phase == Phase::Pending) {
        task->phase = Phase::Cancelled;
        task->completed.notify_all();
      }
    }
  }

  /** JS-thread cleanup, required before destroying this generation's runtime.
   */
  void invalidate() {
    if (std::this_thread::get_id() != threadId_) {
      throw std::logic_error(
          "UniffiCallInvoker::invalidate requires the JS thread");
    }
    stop();
    std::vector<std::shared_ptr<Task>> cancelled;
    {
      std::lock_guard<std::mutex> lock(state_->mutex);
      auto &tasks = state_->tasks;
      for (auto iterator = tasks.begin(); iterator != tasks.end();) {
        if ((*iterator)->phase == Phase::Cancelled) {
          cancelled.push_back(*iterator);
          iterator = tasks.erase(iterator);
        } else {
          ++iterator;
        }
      }
    }
    for (const auto &task : cancelled) {
      task->func = nullptr;
    }
  }
};
} // namespace uniffi_runtime
