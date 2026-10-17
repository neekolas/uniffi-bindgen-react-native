#!/usr/bin/env bash
# Run actual Rust fixture future ownership tests with Hermes and AddressSanitizer.
# Usage: script HERMES_SRC HERMES_BUILD ARC_CPP ASYNC_CPP FIXTURE_LIB_DIR [POLICY_ARC_CPP]
set -euo pipefail
if [[ $# != 5 && $# != 6 ]]; then
  echo "Usage: $0 HERMES_SRC HERMES_BUILD ARC_CPP ASYNC_CPP FIXTURE_LIB_DIR [POLICY_ARC_CPP]" >&2
  exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
hermes=$1
hermes_build=$2
arc_cpp=$3
async_cpp=$4
fixture_lib=$5
build=$(mktemp -d "${TMPDIR:-/tmp}/ubrn-future-ownership.XXXXXX")
trap 'rm -rf "$build"' EXIT
target_flags=()
case "$(uname -s)" in
  Darwin) extension=dylib; target_flags=(-target "$(uname -m)-apple-macos14") ;;
  *) extension=so ;;
esac
entries=("native-future-ownership:$arc_cpp:arc_futures" "foreign-future-drop:$async_cpp:async_callbacks")
if [[ $# == 6 ]]; then
  entries+=("native-future-ownership:$6:arc_futures:policy")
fi
for entry in "${entries[@]}"; do
  IFS=: read -r test wrapper library policy <<< "$entry"
  policy_flags=()
  if [[ ${policy:-} == policy ]]; then
    policy_flags=(-DNATIVE_OBJECT_POLICY -I"$root/cpp/test-harness")
    # A separate consumer translation unit checks the emitted ABI header.
    "${CXX:-clang++}" "${target_flags[@]}" -std=c++20 -fsyntax-only -x c++ \
      -I"$hermes/API/jsi" -I"$root/cpp/stubs" -I"$root/cpp/includes" \
      -include "$(dirname "$wrapper")/arc_futures_ffi.hpp" /dev/null
  fi
  "${CXX:-clang++}" "${target_flags[@]}" "${policy_flags[@]}" -std=c++20 -fsanitize=address -g \
    -DGENERATED_WRAPPER=\""$wrapper"\" \
    -I"$hermes/API" -I"$hermes/API/jsi" -I"$hermes/public" \
    -I"$root/cpp/stubs" -I"$root/cpp/includes" \
    "$root/cpp/test-harness/$test.cpp" \
    "$hermes_build/API/hermes/libhermes.$extension" \
    "$hermes_build/jsi/libjsi.$extension" "$fixture_lib/lib$library.$extension" \
    -Wl,-rpath,"$hermes_build/API/hermes" -Wl,-rpath,"$hermes_build/jsi" \
    -Wl,-rpath,"$fixture_lib" -o "$build/$test"
  "$build/$test"
done
