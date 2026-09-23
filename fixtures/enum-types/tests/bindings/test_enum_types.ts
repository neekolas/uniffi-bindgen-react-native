/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
// To run:
//   cargo test -p uniffi-fixture-enum-types -- jsi
//   cargo test -p uniffi-fixture-enum-types -- wasm

import { test } from "@/asserts";
import {
  Animal,
  AnimalAssociatedType,
  AnimalAssociatedType_Tags,
  AnimalLargeUInt,
  AnimalNamedAssociatedType,
  AnimalNamedAssociatedType_Tags,
  AnimalNoReprInt,
  AnimalObject,
  AnimalRenamedByToml,
  AnimalRecord,
  AnimalSignedInt,
  AnimalUInt,
  getAnimal,
  identityEnumWithAssociatedType,
  identityEnumWithNamedAssociatedType,
  CollidingVariants,
  CollidingVariants_Tags,
  identityCollidingVariants,
  AnimalObjectInterface,
  OptionalFields,
  OptionalFields_Tags,
  identityOptionalFields,
  NoReprColor,
  ReprColor,
  ChainedError,
  ChainedError_Tags,
  Expr,
  ExprGroup,
  Expr_Tags,
  IntList,
  IntList_Tags,
  Detour,
  Route,
  Route_Tags,
  identityRoute,
  routeLength,
  failWithChain,
  identityExpr,
  identityExprGroup,
  identityIntList,
  intListSum,
  makeIntList,
} from "@/generated/enum_types";

test("Enum discriminant", (t) => {
  t.assertEqual(Animal.Dog, 0);
  t.assertEqual(Animal.Cat, 1);
  t.assertEqual(getAnimal(undefined), Animal.Dog);
  t.assertEqual(AnimalNoReprInt.Dog, 0);
  t.assertEqual(AnimalNoReprInt.Cat, 1);
  t.assertEqual(AnimalUInt.Dog, 3);
  t.assertEqual(AnimalUInt.Cat, 4);
  t.assertEqual(
    AnimalLargeUInt.Dog,
    (BigInt("4294967295") + BigInt("3")).toString(),
  );
  t.assertEqual(
    AnimalLargeUInt.Cat,
    (BigInt("4294967295") + BigInt("4")).toString(),
  );
  t.assertEqual(AnimalSignedInt.Dog, -3);
  t.assertEqual(AnimalSignedInt.Cat, -2);
  t.assertEqual(AnimalSignedInt.Koala, -1);
  t.assertEqual(AnimalSignedInt.Wallaby, 0);
  t.assertEqual(AnimalSignedInt.Wombat, 1);

  // Both enums are `Color` in Rust. Only `ReprColor` has a repr type.
  t.assertEqual(ReprColor.Red, 1);
  t.assertEqual(ReprColor.Green, 2);
  t.assertEqual(NoReprColor.Red, 0);
  t.assertEqual(NoReprColor.Green, 1);

  // Renamed in uniffi.toml.
  t.assertEqual(AnimalRenamedByToml.Dog, 7);
  t.assertEqual(AnimalRenamedByToml.Cat, 8);
});

test("Roundtripping enums with values", (t) => {
  function assertEqual(
    left: AnimalAssociatedType,
    right: AnimalAssociatedType,
  ): void {
    t.assertEqual(left.tag, right.tag);
    switch (left.tag) {
      case AnimalAssociatedType_Tags.Cat:
        return;
      case AnimalAssociatedType_Tags.Dog:
        if (AnimalAssociatedType.Dog.instanceOf(right)) {
          t.assertEqual(left.inner[0].record(), right.inner[0].record());
        } else {
          t.fail(`${right} is not a Dog`);
        }
    }
  }
  const values = [
    AnimalAssociatedType.Cat.new(),
    AnimalAssociatedType.Dog.new(new AnimalObject(1)),
    AnimalAssociatedType.Dog.new(new AnimalObject(2)),
  ];

  for (const v of values) {
    t.assertTrue(AnimalAssociatedType.instanceOf(v as any));
    assertEqual(v, identityEnumWithAssociatedType(v));
  }
});

test("Roundtripping enums with name values", (t) => {
  function assertEqual(
    left: AnimalNamedAssociatedType,
    right: AnimalNamedAssociatedType,
  ): void {
    t.assertEqual(left.tag, right.tag);
    switch (left.tag) {
      case AnimalNamedAssociatedType_Tags.Cat:
        return;
      case AnimalNamedAssociatedType_Tags.Dog:
        if (AnimalNamedAssociatedType.Dog.instanceOf(right)) {
          t.assertEqual(left.inner.value.record(), right.inner.value.record());
        } else {
          t.fail(`${right} is not a Dog`);
        }
    }
  }
  const values = [
    AnimalNamedAssociatedType.Cat.new(),
    AnimalNamedAssociatedType.Dog.new({ value: new AnimalObject(1) }),
    AnimalNamedAssociatedType.Dog.new({ value: new AnimalObject(2) }),
  ];

  for (const v of values) {
    t.assertTrue(AnimalNamedAssociatedType.instanceOf(v as any));
    assertEqual(v, identityEnumWithNamedAssociatedType(v));
  }
});

test("Variant naming cam collide with existing types", (t) => {
  {
    const record = AnimalRecord.create({ value: 5 });
    const variant1 = CollidingVariants.AnimalRecord.new(record);
    const variant2 = new CollidingVariants.AnimalRecord(record);

    t.assertEqual(variant1.tag, CollidingVariants_Tags.AnimalRecord);
    t.assertEqual(variant2.tag, CollidingVariants_Tags.AnimalRecord);
    t.assertEqual(variant1, variant2);
    t.assertEqual(identityCollidingVariants(variant1), variant2);

    t.assertTrue(CollidingVariants.instanceOf(variant1));
    t.assertTrue(CollidingVariants.AnimalRecord.instanceOf(variant1));
  }
  {
    const obj = new AnimalObject(1);
    const variant1 = CollidingVariants.AnimalObject.new(obj);
    const variant2 = new CollidingVariants.AnimalObject(obj);

    t.assertEqual(variant1.tag, CollidingVariants_Tags.AnimalObject);
    t.assertEqual(variant2.tag, CollidingVariants_Tags.AnimalObject);
    t.assertEqual(variant1, variant2);
    t.assertEqual(identityCollidingVariants(variant1), variant2);

    t.assertTrue(CollidingVariants.instanceOf(variant1));
    t.assertTrue(CollidingVariants.AnimalObject.instanceOf(variant1));
  }

  {
    const obj = new AnimalObject(1);
    const variant1 = CollidingVariants.AnimalObjectInterface.new(obj);
    const variant2 = new CollidingVariants.AnimalObjectInterface(obj);

    t.assertEqual(variant1.tag, CollidingVariants_Tags.AnimalObjectInterface);
    t.assertEqual(variant2.tag, CollidingVariants_Tags.AnimalObjectInterface);
    t.assertEqual(variant1, variant2);
    t.assertEqual(identityCollidingVariants(variant1), variant2);

    t.assertTrue(CollidingVariants.instanceOf(variant1));
    t.assertTrue(CollidingVariants.AnimalObjectInterface.instanceOf(variant1));
  }
  {
    const animal = Animal.Dog;
    const variant1 = CollidingVariants.Animal.new(animal);
    const variant2 = new CollidingVariants.Animal(animal);

    t.assertEqual(variant1.tag, CollidingVariants_Tags.Animal);
    t.assertEqual(variant2.tag, CollidingVariants_Tags.Animal);
    t.assertEqual(variant1, variant2);
    t.assertEqual(identityCollidingVariants(variant1), variant2);

    t.assertTrue(CollidingVariants.instanceOf(variant1));
    t.assertTrue(CollidingVariants.Animal.instanceOf(variant1));
  }

  {
    const variant1 = CollidingVariants.CollidingVariants.new();
    const variant2 = new CollidingVariants.CollidingVariants();

    t.assertEqual(variant1.tag, CollidingVariants_Tags.CollidingVariants);
    t.assertEqual(variant2.tag, CollidingVariants_Tags.CollidingVariants);
    t.assertEqual(variant1, variant2);
    t.assertEqual(identityCollidingVariants(variant1), variant2);

    t.assertTrue(CollidingVariants.instanceOf(variant1));
    t.assertTrue(CollidingVariants.CollidingVariants.instanceOf(variant1));
  }
});

test("Variant with Option fields accepts omitted keys and undefined", (t) => {
  // Omitting optional keys is a compile-time check (would fail tsc if `?:` was lost).
  const v1 = OptionalFields.Named.new({ required: "hello" });
  t.assertEqual(v1.tag, OptionalFields_Tags.Named);
  if (OptionalFields.Named.instanceOf(v1)) {
    t.assertEqual(v1.inner.required, "hello");
    t.assertEqual(v1.inner.maybeString, undefined);
    t.assertEqual(v1.inner.maybeRecord, undefined);
  } else {
    t.fail("expected Named");
  }

  // Explicit `undefined` is also accepted.
  const v2 = new OptionalFields.Named({
    required: "world",
    maybeString: undefined,
    maybeRecord: undefined,
  });
  t.assertEqual(identityOptionalFields(v2).tag, OptionalFields_Tags.Named);

  // Supplying values roundtrips.
  const v3 = OptionalFields.Named.new({
    required: "r",
    maybeString: "s",
    maybeRecord: AnimalRecord.create({ value: 7 }),
  });
  const roundTripped = identityOptionalFields(v3);
  if (OptionalFields.Named.instanceOf(roundTripped)) {
    t.assertEqual(roundTripped.inner.required, "r");
    t.assertEqual(roundTripped.inner.maybeString, "s");
    t.assertEqual(roundTripped.inner.maybeRecord?.value, 7);
  } else {
    t.fail("expected Named");
  }
});

// This tests the generated Typescript and serves as an example of how to
// to use enums with values.
//
// In each of the variants, we switch match on the `variant.tag`. Typescript then
// infers the type of `variant.inner`.
//
// In this particular example, each of the variants has a tuple value, of length 1.
// If the types aren't inferred correctly, then Typescript would error at compile time.
function testPatternMatching(variant: CollidingVariants) {
  switch (variant.tag) {
    case CollidingVariants_Tags.AnimalRecord: {
      const record: AnimalRecord = variant.inner[0];
      break;
    }
    case CollidingVariants_Tags.AnimalObject: {
      const object: AnimalObjectInterface = variant.inner[0];
      break;
    }
    case CollidingVariants_Tags.AnimalObjectInterface: {
      const object: AnimalObjectInterface = variant.inner[0];
      break;
    }
    case CollidingVariants_Tags.Animal: {
      const animal: Animal = variant.inner[0];
      break;
    }
  }
}

// This tests the generated Typescript and serves as an example of how to
// to use enums with values.
//
// In each of the variants, we switch match on the `variant.tag`. Typescript then
// infers the type of `variant.inner`.
//
// In this particular example, Cat has no associated variables. Dog an associated tuple
// of length 1, and of type `AnimalObjectInterface`.
function testPatternMatching3(variant: AnimalAssociatedType) {
  switch (variant.tag) {
    case AnimalAssociatedType_Tags.Cat: {
      // const none: undefined = variant.inner;
      break;
    }
    case AnimalAssociatedType_Tags.Dog: {
      const dog: AnimalObjectInterface = variant.inner[0];
      break;
    }
  }
}

// This tests the generated Typescript and serves as an example of how to
// to use enums with values.
//
// In each of the variants, we switch match on the `variant.tag`. Typescript then
// infers the type of `variant.inner`.
//
// In this particular example, Cat has no associated variables. Dog has an object
// with one named value, of type `AnimalObjectInterface`.
function testPatternMatching2(variant: AnimalNamedAssociatedType) {
  switch (variant.tag) {
    case AnimalNamedAssociatedType_Tags.Cat: {
      // const cat: AnimalObjectInterface = variant.inner.value;
      break;
    }
    case AnimalNamedAssociatedType_Tags.Dog: {
      const dog: AnimalObjectInterface = variant.inner.value;
      break;
    }
  }
}

// `IntList` is a recursive enum: `Cons` holds a `Box<IntList>`.
function intListToArray(list: IntList): number[] {
  const out: number[] = [];
  let current = list;
  while (current.tag === IntList_Tags.Cons) {
    const [head, tail] = current.inner;
    out.push(head);
    current = tail;
  }
  return out;
}

test("Recursive enum: made in JS, read in Rust", (t) => {
  const list = IntList.Cons.new(
    1,
    IntList.Cons.new(2, IntList.Cons.new(3, IntList.Nil.new())),
  );
  t.assertEqual(list.tag, IntList_Tags.Cons);
  t.assertEqual(intListToArray(list), [1, 2, 3]);
  t.assertEqual(intListSum(list), 6);
  t.assertEqual(intListSum(IntList.Nil.new()), 0);
});

test("Recursive enum: made in Rust, read in JS", (t) => {
  t.assertEqual(intListToArray(makeIntList([1, 2, 3, 4, 5])), [1, 2, 3, 4, 5]);
  t.assertTrue(IntList.Nil.instanceOf(makeIntList([])));
  t.assertTrue(IntList.instanceOf(makeIntList([])));
});

test("Recursive enum: round trips through Rust", (t) => {
  const list = IntList.Cons.new(10, IntList.Cons.new(20, IntList.Nil.new()));
  t.assertEqual(intListToArray(identityIntList(list)), [10, 20]);
  t.assertEqual(intListToArray(identityIntList(IntList.Nil.new())), []);
});

test("Recursive enum: a long list crosses the FFI", (t) => {
  const n = 500;
  const values = Array.from({ length: n }, (_, i) => i + 1);
  const list = makeIntList(values);
  t.assertEqual(intListSum(list), (n * (n + 1)) / 2);
  t.assertEqual(intListToArray(identityIntList(list)), values);
});

// A type check only: a value narrowed to one variant can go where the
// variant class is the type.
function testRecursiveNarrowing(list: IntList): number {
  if (list.tag === IntList_Tags.Cons) {
    const cons: InstanceType<typeof IntList.Cons> = list;
    return cons.inner[0];
  }
  const nil: InstanceType<typeof IntList.Nil> = list;
  return nil.tag === IntList_Tags.Nil ? 0 : -1;
}

test("Recursive enum: narrowing", (t) => {
  t.assertEqual(testRecursiveNarrowing(makeIntList([7])), 7);
  t.assertEqual(testRecursiveNarrowing(makeIntList([])), 0);
});

test("Every enum in a cycle round trips", (t) => {
  const route = Route.Link.new({
    next: Route.Detour.new(
      Detour.Via.new({ next: Route.Link.new({ next: Route.End.new() }) }),
    ),
  });
  t.assertEqual(routeLength(route), 3);
  const result = identityRoute(route);
  t.assertEqual(routeLength(result), 3);
  t.assertEqual(result.tag, Route_Tags.Link);
  if (result.tag === Route_Tags.Link) {
    const next = result.inner[0].next;
    t.assertEqual(next?.tag, Route_Tags.Detour);
    if (next?.tag === Route_Tags.Detour) {
      t.assertTrue(Detour.Via.instanceOf(next.inner[0]));
    }
  }
  t.assertEqual(routeLength(identityRoute(Route.End.new())), 0);
});

// `Expr` and `ExprGroup` refer to each other, and `Expr` refers to itself.
function makeExpr(): Expr {
  return Expr.Group.new({
    label: "outer",
    items: [
      Expr.Num.new(1),
      Expr.Negate.new({ expr: Expr.Num.new(5) }),
      Expr.Group.new({ label: "inner", items: [Expr.Num.new(10)] }),
    ],
  });
}

test("Enum and record cycle: round trips through Rust", (t) => {
  const expr = makeExpr();
  t.assertEqual(Expr.eval(expr), 6);

  const result = identityExpr(expr);
  t.assertEqual(result.tag, Expr_Tags.Group);
  t.assertTrue(result.equals(expr));
  t.assertEqual(result.toString(), expr.toString());
  t.assertEqual(result.hashCode(), expr.hashCode());
  t.assertEqual(Expr.eval(result), 6);
  t.assertFalse(result.equals(Expr.Num.new(6)));

  if (result.tag === Expr_Tags.Group) {
    const [group] = result.inner;
    t.assertEqual(group.label, "outer");
    t.assertEqual(group.items.length, 3);
    const negate = group.items[1];
    t.assertEqual(negate.tag, Expr_Tags.Negate);
    if (negate.tag === Expr_Tags.Negate) {
      t.assertEqual(Expr.eval(negate.inner.expr), 5);
    }
  }
});

test("Enum and record cycle: the record round trips through Rust", (t) => {
  const group: ExprGroup = {
    label: "g",
    items: [makeExpr(), Expr.Num.new(-1)],
  };
  const result = identityExprGroup(group);
  t.assertEqual(result.label, "g");
  t.assertEqual(result.items.length, 2);
  t.assertTrue(result.items[0].equals(group.items[0]));
  t.assertEqual(Expr.eval(result.items[0]), 6);
  t.assertEqual(Expr.eval(result.items[1]), -1);
});

test("Recursive error enum", (t) => {
  t.assertThrows(ChainedError.Root.instanceOf, () => failWithChain(0));
  t.assertThrows(ChainedError.Wrapped.instanceOf, () => failWithChain(2));
  try {
    failWithChain(2);
    t.fail("No error was thrown");
  } catch (e: any) {
    t.assertTrue(ChainedError.Wrapped.instanceOf(e));
    const outer = e as InstanceType<typeof ChainedError.Wrapped>;
    t.assertEqual(outer.inner.depth, 2);
    const middle = outer.inner.cause;
    t.assertEqual(middle.tag, ChainedError_Tags.Wrapped);
    if (middle.tag === ChainedError_Tags.Wrapped) {
      t.assertEqual(middle.inner.depth, 1);
      const root = middle.inner.cause;
      t.assertEqual(root.tag, ChainedError_Tags.Root);
      if (root.tag === ChainedError_Tags.Root) {
        t.assertEqual(root.inner.message, "the cause");
      }
    }
  }
});
