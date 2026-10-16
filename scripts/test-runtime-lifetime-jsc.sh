#!/usr/bin/env bash
# Run the production C++ invoker against React Native's real JSI/JSC runtime.
# Usage: scripts/test-runtime-lifetime-jsc.sh /path/to/react-native
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
rn="${1:?Expected the directory containing the React Native package.json}"
if [[ "$(uname -s)" != Darwin ]]; then
  echo 'JavaScriptCore test runner requires macOS; use the Hermes CMake target elsewhere.' >&2
  exit 1
fi
build="$(mktemp -d)"
trap 'rm -rf "$build"' EXIT
"${UBRN_TEST_CXX:-/usr/bin/clang++}" \
  -std=c++20 -mmacosx-version-min=14.0 -fsanitize=address -g -DUBRN_TEST_JSC \
  -I"$root/cpp/includes" -I"$root/cpp/stubs" \
  -I"$rn/ReactCommon" -I"$rn/ReactCommon/jsi" \
  "$root/cpp/test-harness/runtime-lifetime.cpp" \
  "$rn/ReactCommon/jsi/jsi/jsi.cpp" "$rn/ReactCommon/jsc/JSCRuntime.cpp" \
  -framework JavaScriptCore -o "$build/runtime-lifetime-test"
"$build/runtime-lifetime-test"
