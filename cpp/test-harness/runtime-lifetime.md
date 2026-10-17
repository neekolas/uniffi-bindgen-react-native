# Runtime lifetime regression

`runtime-lifetime.cpp` uses the production invoker and handle registry with a real
JSI runtime. It checks queued cancellation, destruction of JS captures on their JS
thread, running callback output lifetime, error delivery, stale handle ownership,
late frees, token reuse after the last in-flight lease, 128 runtime replacements,
and exactly-once cleanup of pending foreign and Rust futures.

After configuring the existing Hermes test harness, build and run:

```sh
cmake --build build/test-runner --target runtime-lifetime-test
ctest --test-dir build/test-runner -R runtime-lifetime --output-on-failure
```

Hermes also tests the actual BigInt allocator host functions. To exercise the
production TypeScript handle maps against those functions, install the existing
JavaScript development dependencies and build the probe:

```sh
node scripts/bundle-runtime-handles.cjs /tmp/runtime-handles.js
build/test-runner/runtime-lifetime-test /tmp/runtime-handles.js
```

The probe creates callbacks, Rust future continuations, and foreign-future drop
handles in one runtime. It checks that the maps reject work after invalidation,
then creates a replacement runtime and verifies that old handles and late frees
do not affect its maps. The bundler uses the existing `tsx` and `typescript`
dependencies and lowers class syntax for Hermes.

On macOS, a React Native package source tree provides a JavaScriptCore runtime.
This path enables AddressSanitizer without building Hermes:

```sh
scripts/test-runtime-lifetime-jsc.sh /path/to/node_modules/react-native
```

React Native's JavaScriptCore adapter does not implement the JSI BigInt API, so
this variant calls the native registry directly for handle tests. The invoker
still runs against real JavaScriptCore. Use Hermes for the host-function and
TypeScript probe checks.

The owner must invalidate on the JS thread before destroying the runtime.
Off-thread `stop()` rejects new work and releases workers waiting for queued
callbacks. `invalidate()` destroys the queued JS captures. A callback already
running on the JS thread finishes before its native caller can return.

These tests replace actual JSI runtimes in a controlled queue. The application's
ReactHost or platform reload lifecycle requires its separate integration test.

JSI generation rejects callback arguments, return values, and errors that contain
object or callback handles, including handles inside records, enums, containers,
and custom types. Those schemas need typed native discard cleanup before they
can safely survive runtime teardown. External record and enum schemas must be
available for this check. Value-only recursive schemas are supported. This guard
applies only to JSI generation; Node and Web generation are unchanged.

`native-future-ownership.cpp` and `foreign-future-drop.cpp` use generated bindings
and the actual `arc-futures` and `async-callbacks` Rust fixture libraries. Build
those fixtures and generate their JSI wrappers, then run:

```sh
scripts/test-generated-future-ownership.sh /path/to/hermes /path/to/hermes-build \
  /path/to/generated/arc_futures.cpp /path/to/generated/async_callbacks.cpp \
  /path/to/fixture-libraries
```

Use a C++ compiler that matches the Hermes build (for example,
`CXX=/usr/bin/clang++` for an Xcode-built Hermes on macOS). The script enables
AddressSanitizer. The Arc fixture counts live objects. Tests
check unconsumed READY direct objects, nested record/sequence/optional objects,
object-valued errors, caller ownership after completion, late native future
operations, and retirement reentered from a Rust poll's synchronous JS callback.
The foreign-future test cancels real Rust oneshot receivers and verifies that
native drop creates no error RustBuffer. It also sends an unavailable-runtime
failure through the real Rust callback ABI and checks the typed error returned
by Rust. The generated TypeScript callback-input test separately checks input
lifting and cleanup before user code, on lift failure, and before async waiting.

To check the optional native-object policy, generate a second Arc wrapper with
`fixtures/arc-futures/native-policy.toml` and pass its `.cpp` as the script's
optional sixth argument. This compiles the emitted ABI header as a separate
consumer, retains an actual Rust object before a JS status conversion throws,
and routes an unreturned READY object through the configured discard hook.
