/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#pragma once
#include "UniffiCallInvoker.h"
#include <cstdint>
#include <memory>
namespace native_policy {
using Invoker = const std::shared_ptr<uniffi_runtime::UniffiCallInvoker> &;
void started(Invoker, uint64_t);
void completed(Invoker, uint64_t, uint64_t, int8_t);
void freed(Invoker, uint64_t);
void discard(uint64_t);
}
