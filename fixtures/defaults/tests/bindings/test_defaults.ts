/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// To run:
//   cargo test -p uniffi-fixture-defaults -- jsi      (needs Hermes)
//   cargo test -p uniffi-fixture-defaults -- napi
//   cargo test -p uniffi-fixture-defaults -- wasm     (needs wasm-bindgen)
//   cargo test -p uniffi-fixture-defaults -- wasm2    (needs wasm-bindgen)

import theModule, {
  BareDefaults,
  Color,
  SetDefaults,
  Formatter,
  Greeter,
  Settings,
  TestCase,
  echoBareArg,
  echoBareDefaults,
  echoBareObjArg,
  echoBareRecordArg,
  echoBool,
  echoColor,
  echoI32,
  echoOptionNone,
  echoOptionSome,
  echoBareSetArg,
  echoSetArg,
  echoSetDefaults,
  echoSettings,
  echoString,
  echoTestCase,
  join,
  useFormatter,
} from "@/generated/uniffi_defaults";
import { Asserts, test, xtest } from "@/asserts";
import "@/polyfills";

// Initialize the module so that callback-interface vtables are registered
// with Rust before any callback-bearing function is called.
theModule.initialize();

test("function arg default: i32", (t) => {
  t.assertEqual(42, echoI32());
  t.assertEqual(7, echoI32(7));
});

test("function arg default: String", (t) => {
  t.assertEqual("hello", echoString());
  t.assertEqual("world", echoString("world"));
});

test("function arg default: bool", (t) => {
  t.assertEqual(true, echoBool());
  t.assertEqual(false, echoBool(false));
});

test("function arg default: Option<T> = None", (t) => {
  t.assertEqual(undefined, echoOptionNone());
  t.assertEqual(3, echoOptionNone(3));
});

test("function arg default: Option<T> = Some(7)", (t) => {
  t.assertEqual(7, echoOptionSome());
  t.assertEqual(11, echoOptionSome(11));
});

test("function arg default: enum variant (round-trip)", (t) => {
  t.assertEqual(Color.Blue, echoColor(Color.Blue));
});

// uniffi-rs' DefaultValue parser rejects path expressions like Color::Green,
// so echo_color cannot carry a default value. If uniffi-rs gains support, flip
// this xtest to test.
xtest("function arg default: enum variant", (t) => {
  // t.assertEqual(Color.Green, echoColor());
});

test("constructor arg defaults", (t) => {
  const g = new Greeter();
  t.assertEqual("hi world", g.greet());
});

test("method arg default", (t) => {
  const g = new Greeter("hello");
  t.assertEqual("hello world", g.greet());
  t.assertEqual("hello there", g.greet("there"));
});

test("record field defaults: only required field provided", (t) => {
  const s = Settings.create({ required: "go" });
  t.assertEqual(10, s.retries);
  t.assertEqual(undefined, s.label);
  t.assertEqual("go", s.required);
});

test("record field defaults: roundtrip", (t) => {
  const s = Settings.create({ required: "go", retries: 3, label: "hello" });
  const out = echoSettings(s);
  t.assertEqual(3, out.retries);
  t.assertEqual("hello", out.label);
  t.assertEqual("go", out.required);
});

test("enum variant named field default: i32", (t: Asserts) => {
  const v = TestCase.One.new({});
  t.assertEqual(100, v.inner.numValue);
  const out = echoTestCase(v);
  t.assertInstanceOf(out, TestCase.One.instanceOf);
  t.assertEqual(100, out.inner.numValue);
});

test("enum variant named field default: Option<T>=None", (t: Asserts) => {
  const v = TestCase.Two.new({ required: "r" });
  t.assertEqual(undefined, v.inner.maybeLabel);
  t.assertEqual("r", v.inner.required);
  const out = echoTestCase(v);
  t.assertInstanceOf(out, TestCase.Two.instanceOf);
  t.assertEqual(undefined, out.inner.maybeLabel);
  t.assertEqual("r", out.inner.required);
});

test("enum variant tuple field default: i32", (t: Asserts) => {
  const v = TestCase.Three.new();
  t.assertEqual(5, v.inner[0]);
  const out = echoTestCase(v);
  t.assertInstanceOf(out, TestCase.Three.instanceOf);
  t.assertEqual(5, out.inner[0]);
});

test("ordering: defaulted arg after non-defaulted", (t) => {
  t.assertEqual("hi!", join("hi"));
  t.assertEqual("hi?", join("hi", "?"));
});

test("bare default on record field (i32 → T::default())", (t) => {
  const v = BareDefaults.create({ required: "r" });
  t.assertEqual(0, v.n);
  const out = echoBareDefaults(v);
  t.assertEqual(0, out.n);
  t.assertEqual("r", out.required);
});

test("bare default on function arg (i32 → T::default())", (t) => {
  t.assertEqual(0, echoBareArg());
  t.assertEqual(7, echoBareArg(7));
});

test("bare default on Record-typed arg → RecordName.create({})", (t) => {
  const out = echoBareRecordArg();
  t.assertEqual(1, out.a);
  t.assertEqual("x", out.b);
});

test("bare default on Object-typed arg → new ClassName()", (t) => {
  const out = echoBareObjArg();
  t.assertEqual("z", out.label());
});

// The width=2 default on Formatter.format is not visible to JS callers:
// arg_list_protocol strips defaults from protocol declarations, and Rust's
// use_formatter passes width=4 explicitly. The default is not exercised here.
test("trait method default (callback from JS)", (t) => {
  const f: Formatter = {
    format(value: number, width: number) {
      return value.toString().padStart(width, "0");
    },
  };
  t.assertEqual("0007", useFormatter(f));
});

test("record field defaults: sets", (t) => {
  const v = SetDefaults.create({ required: "r" });
  t.assertTrue(v.names instanceof Set);
  t.assertEqual(v.names.size, 0);
  t.assertTrue(v.numbers instanceof Set);
  t.assertEqual(v.numbers.size, 0);
  t.assertTrue(v.maybeTags instanceof Set);
  t.assertEqual(v.maybeTags!.size, 0);

  const out = echoSetDefaults(v);
  t.assertEqual(out.names.size, 0);
  t.assertEqual(out.numbers.size, 0);
  t.assertEqual(out.maybeTags!.size, 0);
  t.assertEqual(out.required, "r");

  const full = echoSetDefaults(
    SetDefaults.create({
      required: "r",
      names: new Set(["a"]),
      numbers: new Set([1, 2]),
      maybeTags: undefined,
    }),
  );
  t.assertTrue(full.names.has("a"));
  t.assertEqual(full.numbers.size, 2);
  t.assertEqual(full.maybeTags, undefined);
});

test("function arg defaults: sets", (t) => {
  const numbers = echoSetArg();
  t.assertTrue(numbers instanceof Set);
  t.assertEqual(numbers.size, 0);
  t.assertTrue(echoSetArg(new Set([3])).has(3));

  const names = echoBareSetArg();
  t.assertTrue(names instanceof Set);
  t.assertEqual(names.size, 0);
  t.assertTrue(echoBareSetArg(new Set(["x"])).has("x"));
});
