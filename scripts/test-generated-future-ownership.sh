#!/usr/bin/env bash
# Run actual Rust fixture future ownership tests with Hermes and AddressSanitizer.
# Usage: script HERMES_SRC HERMES_BUILD ARC_CPP ASYNC_CPP FIXTURE_LIB_DIR
set -euo pipefail
if [[ $# != 5 ]]; then
  echo "Usage: $0 HERMES_SRC HERMES_BUILD ARC_CPP ASYNC_CPP FIXTURE_LIB_DIR" >&2
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
for entry in "native-future-ownership:$arc_cpp:arc_futures" "foreign-future-drop:$async_cpp:async_callbacks"; do
  IFS=: read -r test wrapper library <<< "$entry"
  "${CXX:-clang++}" "${target_flags[@]}" -std=c++20 -fsanitize=address -g \
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
