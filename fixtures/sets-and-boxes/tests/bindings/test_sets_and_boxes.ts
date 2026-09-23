/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// To run:
//   cargo test -p uniffi-fixture-sets-and-boxes -- jsi      (needs Hermes)
//   cargo test -p uniffi-fixture-sets-and-boxes -- napi
//   cargo test -p uniffi-fixture-sets-and-boxes -- wasm     (needs wasm-bindgen)
//   cargo test -p uniffi-fixture-sets-and-boxes -- wasm2    (needs wasm-bindgen)

import {
  Counter,
  Entry,
  type Folder,
  type LinkedNode,
  Shape,
  type ShapeRef,
  type Point,
  type TreeNode,
  addOneBoxed,
  asyncAddOneBoxed,
  asyncIdentityStringSet,
  folderFileCount,
  identityHolder,
  identityI64Set,
  identityLabelled,
  identityFolder,
  identityLinked,
  identityMaybeBoxedPoint,
  identityNested,
  identityPointSet,
  identitySetMap,
  identityShapeRef,
  identityStringSet,
  identityTagList,
  identityTags,
  identityTree,
  identityU32Set,
  linkedFrom,
  linkedSum,
  nextBoxedCounter,
  pointSetLen,
  shapeDotCount,
  shoutBoxed,
  stringSetContains,
  stringSetFrom,
  swapBoxedPoint,
  tagsLen,
  treeSum,
} from "@/generated/uniffi_sets_and_boxes";
import { Asserts, asyncTest, test } from "@/asserts";
import { stringify } from "@/hermes";

// Rust does not keep the order of a `HashSet`, so compare the items in a
// sorted order.
function sortedItems<T>(set: Set<T>): string[] {
  return [...set].map((item) => stringify(item)).sort();
}

function assertSetEqual<T>(t: Asserts, actual: Set<T>, expected: Set<T>) {
  t.assertTrue(actual instanceof Set, "not a Set");
  t.assertEqual(actual.size, expected.size, "wrong size");
  t.assertEqual(sortedItems(actual), sortedItems(expected));
}

test("Set of u32 round trips", (t) => {
  const value = new Set([0, 1, 2, 3, 4294967295]);
  assertSetEqual(t, identityU32Set(value), value);
  assertSetEqual(t, identityU32Set(new Set()), new Set<number>());
});

test("Set of i64 round trips, and bigints compare by value", (t) => {
  const value = new Set([BigInt("-9223372036854775808"), BigInt(0), BigInt(7)]);
  assertSetEqual(t, identityI64Set(value), value);
  // A JS `Set` compares bigints by value.
  t.assertEqual(new Set([BigInt(7), BigInt(7)]).size, 1);
});

test("Set of strings round trips", (t) => {
  const value = new Set(["", "a", "b", "🦊 unicode"]);
  const result = identityStringSet(value);
  assertSetEqual(t, result, value);
  for (const item of value) {
    t.assertTrue(result.has(item), `missing ${item}`);
  }
});

test("Rust removes duplicates when it makes a set", (t) => {
  const result = stringSetFrom(["a", "b", "a", "c", "b"]);
  assertSetEqual(t, result, new Set(["a", "b", "c"]));
  t.assertTrue(stringSetContains(new Set(["x", "y"]), "y"));
  t.assertFalse(stringSetContains(new Set(["x", "y"]), "z"));
});

test("A JS Set of records compares by reference, Rust compares by value", (t) => {
  const a: Point = { x: 1, y: 2 };
  const b: Point = { x: 1, y: 2 };
  const jsSet = new Set([a, b, { x: 3, y: 4 }]);
  t.assertEqual(jsSet.size, 3);
  t.assertEqual(pointSetLen(jsSet), 2);
  assertSetEqual(
    t,
    identityPointSet(jsSet),
    new Set([
      { x: 1, y: 2 },
      { x: 3, y: 4 },
    ]),
  );
});

test("Nested Option<Vec<HashSet<String>>> round trips", (t) => {
  t.assertEqual(identityNested(undefined), undefined);
  t.assertEqual(identityNested([])?.length, 0);
  const value = [new Set(["a", "b"]), new Set<string>(), new Set(["c"])];
  const result = identityNested(value);
  t.assertNotNull(result);
  t.assertEqual(result!.length, value.length);
  for (let i = 0; i < value.length; i++) {
    assertSetEqual(t, result![i], value[i]);
  }
});

test("HashMap<String, HashSet<u32>> round trips", (t) => {
  const value = new Map([
    ["empty", new Set<number>()],
    ["some", new Set([1, 2, 3])],
  ]);
  const result = identitySetMap(value);
  t.assertEqual(result.size, 2);
  assertSetEqual(t, result.get("empty")!, new Set<number>());
  assertSetEqual(t, result.get("some")!, new Set([1, 2, 3]));
});

test("A custom type over a set, and over that custom type, round trips", (t) => {
  const value = new Set(["red", "green"]);
  assertSetEqual(t, identityTags(value), value);
  t.assertEqual(tagsLen(value), 2);
  assertSetEqual(t, identityTagList(value), value);
});

test("A record with set fields round trips", (t) => {
  const withScores = {
    name: "n",
    labels: new Set(["x", "y"]),
    scores: new Set([-1, 0, 1]),
  };
  const result = identityLabelled(withScores);
  t.assertEqual(result.name, "n");
  assertSetEqual(t, result.labels, withScores.labels);
  assertSetEqual(t, result.scores!, withScores.scores);

  const withoutScores = identityLabelled({
    name: "m",
    labels: new Set<string>(),
  });
  t.assertEqual(withoutScores.labels.size, 0);
  t.assertEqual(withoutScores.scores, undefined);
});

test("Box<T> as an argument and a return value", (t) => {
  t.assertEqual(addOneBoxed(41), 42);
  t.assertEqual(addOneBoxed(-2147483648), -2147483647);
  t.assertEqual(shoutBoxed("hello"), "HELLO");
  t.assertEqual(swapBoxedPoint({ x: 1, y: 2 }), { x: 2, y: 1 });
  t.assertEqual(identityMaybeBoxedPoint(undefined), undefined);
  t.assertEqual(identityMaybeBoxedPoint({ x: 5, y: 6 }), { x: 5, y: 6 });
});

test("Box<Arc<Object>> passes an object handle", (t) => {
  const counter = new Counter(1);
  const next = nextBoxedCounter(counter);
  t.assertEqual(next.value(), 2);
  t.assertEqual(nextBoxedCounter(next).value(), 3);
});

test("Recursive record through Option<Box<Self>> round trips", (t) => {
  const list = linkedFrom([1, 2, 3]);
  t.assertNotNull(list);
  t.assertEqual(list!.value, 1);
  t.assertEqual(list!.next?.value, 2);
  t.assertEqual(list!.next?.next?.value, 3);
  t.assertEqual(list!.next?.next?.next, undefined);
  t.assertEqual(linkedSum(list!), 6);
  t.assertEqual(linkedFrom([]), undefined);

  const byHand: LinkedNode = { value: 10, next: { value: 20 } };
  t.assertEqual(identityLinked(byHand), byHand);
  t.assertEqual(linkedSum(byHand), 30);

  const holder = { head: byHand };
  t.assertEqual(identityHolder(holder), holder);
});

test("A long recursive record crosses the FFI", (t) => {
  const n = 500;
  const values = Array.from({ length: n }, (_, i) => i + 1);
  const list = linkedFrom(values)!;
  const expected = (n * (n + 1)) / 2;
  t.assertEqual(linkedSum(list), expected);
  t.assertEqual(linkedSum(identityLinked(list)), expected);
});

test("Recursive record through Vec<Self> round trips", (t) => {
  const tree: TreeNode = {
    value: 1,
    children: [
      { value: 2, children: [] },
      { value: 3, children: [{ value: 4, children: [] }] },
    ],
  };
  t.assertEqual(treeSum(tree), 10);
  t.assertEqual(identityTree(tree), tree);
});

// The generated module loads only if each custom type in a cycle comes after
// the converter of its builtin type.
test("A custom type over a record in a cycle round trips", (t) => {
  const inner: Folder = {
    name: "inner",
    entries: [Entry.File.new({ name: "b" })],
  };
  const folder: Folder = {
    name: "root",
    entries: [
      Entry.File.new({ name: "a" }),
      Entry.Folder.new({ folder: inner }),
      Entry.Link.new({ target: inner }),
    ],
  };
  t.assertEqual(folderFileCount(folder), 3);

  const result = identityFolder(folder);
  t.assertEqual(folderFileCount(result), 3);
  t.assertEqual(result.name, "root");
  t.assertEqual(result.entries.length, 3);
  const [file, sub, link] = result.entries;
  t.assertTrue(Entry.File.instanceOf(file));
  t.assertTrue(Entry.Folder.instanceOf(sub));
  if (Entry.Link.instanceOf(link)) {
    t.assertEqual(link.inner.target.name, "inner");
    t.assertEqual(link.inner.target.entries.length, 1);
  } else {
    t.fail("not a Link");
  }
});

test("A custom type over an enum in a cycle round trips", (t) => {
  const shape: ShapeRef = Shape.Group.new({
    children: new Map<string, ShapeRef>([
      ["dot", Shape.Dot.new()],
      ["framed", Shape.Framed.new({ content: Shape.Dot.new() })],
      ["empty", Shape.Framed.new({})],
    ]),
  });
  t.assertEqual(shapeDotCount(shape), 2);

  const result = identityShapeRef(shape);
  t.assertEqual(shapeDotCount(result), 2);
  if (Shape.Group.instanceOf(result)) {
    const children = result.inner.children;
    t.assertEqual(children.size, 3);
    t.assertTrue(Shape.Dot.instanceOf(children.get("dot")!));
    const framed = children.get("framed")!;
    if (Shape.Framed.instanceOf(framed)) {
      t.assertTrue(Shape.Dot.instanceOf(framed.inner.content!));
    } else {
      t.fail("not Framed");
    }
    const empty = children.get("empty")!;
    t.assertTrue(Shape.Framed.instanceOf(empty));
    if (Shape.Framed.instanceOf(empty)) {
      t.assertEqual(empty.inner.content, undefined);
    }
  } else {
    t.fail("not a Group");
  }
});

(async () => {
  await asyncTest("Async functions with a set and a box", async (t) => {
    const value = new Set(["a", "b"]);
    assertSetEqual(t, await asyncIdentityStringSet(value), value);
    t.assertEqual(await asyncAddOneBoxed(1), 2);
    t.end();
  });
})();
