/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
import { testRoundTripThroughForeign } from "@/generated/coverall";
import { test } from "@/asserts";
import { TypeScriptGetters, testGracefulShutdown } from "./coverall_common";

test("Round trip through foreign: Rust calls roundTripObject on TypeScript implementation", (t) => {
  const tsGetters = new TypeScriptGetters();
  // This calls tsGetters.roundTripObject(coveralls) inside Rust and asserts the result.
  // It will throw if roundTripObject returns the wrong object.
  testRoundTripThroughForeign(tsGetters);
});

testGracefulShutdown();
