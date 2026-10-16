/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
import { UniffiHandleMap } from "../src/handle-map";
import {
  FfiConverterObjectWithCallbacks,
  type UniffiObjectFactory,
} from "../src/objects";
import { Asserts, test, asyncTest } from "../testing/asserts";
import {
  uniffiRustCallAsync,
  uniffiRustFutureHandleCount,
} from "../src/async-rust-call";
import { UniffiRustCaller } from "../src/rust-call";

function isStaleHandleError(e: any): boolean {
  return e instanceof Error && e.message.includes("handle map");
}

// uniffi 0.30 requires foreign handles to be odd (non-zero) values so that
// they can be distinguished from Rust Arc pointers, which are always even.

test("first handle is odd and non-zero", (t) => {
  const map = new UniffiHandleMap<string>();
  const h = map.insert("a");
  t.assertTrue(h !== BigInt(0), "handle must be non-zero");
  t.assertTrue(h % BigInt(2) === BigInt(1), `handle must be odd, got ${h}`);
});

test("all handles are odd", (t) => {
  const map = new UniffiHandleMap<number>();
  for (let i = 0; i < 10; i++) {
    const h = map.insert(i);
    t.assertTrue(
      h % BigInt(2) === BigInt(1),
      `handle ${h} for insert #${i} must be odd`,
    );
  }
});

test("handles are unique", (t) => {
  const map = new UniffiHandleMap<string>();
  const h1 = map.insert("a");
  const h2 = map.insert("b");
  const h3 = map.insert("c");
  t.assertNotEqual(h1, h2);
  t.assertNotEqual(h2, h3);
  t.assertNotEqual(h1, h3);
});

test("handles increment by 2", (t) => {
  const map = new UniffiHandleMap<string>();
  const h1 = map.insert("a");
  const h2 = map.insert("b");
  t.assertEqual(h2 - h1, BigInt(2), "consecutive handles must differ by 2");
});

test("get returns inserted value", (t) => {
  const map = new UniffiHandleMap<string>();
  const h = map.insert("hello");
  t.assertEqual(map.get(h), "hello");
});

test("get with unknown handle throws stale handle error", (t) => {
  const map = new UniffiHandleMap<string>();
  t.assertThrows(isStaleHandleError, () => map.get(BigInt(99)));
});

test("get after remove throws stale handle error", (t) => {
  const map = new UniffiHandleMap<string>();
  const h = map.insert("gone");
  map.remove(h);
  t.assertThrows(isStaleHandleError, () => map.get(h));
});

test("remove returns the value", (t) => {
  const map = new UniffiHandleMap<string>();
  const h = map.insert("value");
  t.assertEqual(map.remove(h), "value");
});

test("remove of already-removed handle returns undefined", (t) => {
  const map = new UniffiHandleMap<string>();
  const h = map.insert("value");
  map.remove(h);
  t.assertNull(map.remove(h));
});

test("has returns true for live handle, false after remove", (t) => {
  const map = new UniffiHandleMap<number>();
  const h = map.insert(42);
  t.assertTrue(map.has(h), "has should return true for live handle");
  map.remove(h);
  t.assertFalse(map.has(h), "has should return false after remove");
});

test("size tracks insertions and removals", (t) => {
  const map = new UniffiHandleMap<string>();
  t.assertEqual(map.size, 0);
  const h1 = map.insert("a");
  t.assertEqual(map.size, 1);
  const h2 = map.insert("b");
  t.assertEqual(map.size, 2);
  map.remove(h1);
  t.assertEqual(map.size, 1);
  map.remove(h2);
  t.assertEqual(map.size, 0);
});

// Clone semantics: uniffi 0.30 CallbackInterfaceClone creates a second handle
// pointing to the same object, so Rust can hold two independent references to
// the same JS callback object.

test("clone returns a new, distinct handle", (t) => {
  const map = new UniffiHandleMap<string>();
  const h1 = map.insert("shared");
  const h2 = map.clone(h1);
  t.assertNotEqual(h1, h2, "clone must return a different handle");
});

test("cloned handle is also odd", (t) => {
  const map = new UniffiHandleMap<string>();
  const h1 = map.insert("x");
  const h2 = map.clone(h1);
  t.assertTrue(h2 % BigInt(2) === BigInt(1), `cloned handle ${h2} must be odd`);
});

test("both original and cloned handle resolve to same object", (t) => {
  const obj = { value: 42 };
  const map = new UniffiHandleMap<typeof obj>();
  const h1 = map.insert(obj);
  const h2 = map.clone(h1);
  t.assertTrue(
    map.get(h1) === map.get(h2),
    "both handles must be same object reference",
  );
});

test("removing original handle does not invalidate the clone", (t) => {
  const map = new UniffiHandleMap<string>();
  const h1 = map.insert("shared");
  const h2 = map.clone(h1);
  map.remove(h1);
  t.assertEqual(
    map.get(h2),
    "shared",
    "clone must survive removal of original",
  );
});

test("removing clone does not invalidate the original", (t) => {
  const map = new UniffiHandleMap<string>();
  const h1 = map.insert("shared");
  const h2 = map.clone(h1);
  map.remove(h2);
  t.assertEqual(
    map.get(h1),
    "shared",
    "original must survive removal of clone",
  );
});

test("clone of unknown handle throws stale handle error", (t) => {
  const map = new UniffiHandleMap<string>();
  t.assertThrows(isStaleHandleError, () => map.clone(BigInt(99)));
});

test("clone increments size by one", (t) => {
  const map = new UniffiHandleMap<string>();
  map.insert("a");
  t.assertEqual(map.size, 1);
  const h = map.insert("b");
  t.assertEqual(map.size, 2);
  map.clone(h);
  t.assertEqual(map.size, 3);
});

// Model the native owner boundary, while retaining the real handle-map code.
// The C++ lifetime test separately exercises these hooks in a real JSI runtime.
type AllocatorHooks = {
  allocate(kind: string): bigint;
  release(handle: bigint): void;
  owns(handle: bigint): boolean;
};
const nativeGlobal = globalThis as typeof globalThis & {
  __ubrnHandleAllocator?: AllocatorHooks;
};

function withNativeAllocators(
  body: (
    make: () => {
      hooks: AllocatorHooks;
      stop: () => void;
      kinds: string[];
      released: bigint[];
    },
  ) => void,
): void {
  const saved = nativeGlobal.__ubrnHandleAllocator;
  let next = BigInt(101);
  try {
    body(() => {
      let active = true;
      const live = new Set<bigint>();
      const kinds: string[] = [];
      const released: bigint[] = [];
      const hooks: AllocatorHooks = {
        allocate(kind) {
          if (!active) throw new Error("Runtime has stopped");
          const handle = next;
          next += BigInt(2);
          live.add(handle);
          kinds.push(kind);
          return handle;
        },
        release(handle) {
          if (live.delete(handle)) released.push(handle);
        },
        owns: (handle) => active && live.has(handle),
      };
      return {
        hooks,
        stop: () => {
          active = false;
        },
        kinds,
        released,
      };
    });
  } finally {
    if (saved === undefined) delete nativeGlobal.__ubrnHandleAllocator;
    else nativeGlobal.__ubrnHandleAllocator = saved;
  }
}

test("native handles share one allocator across all callback kinds", (t) => {
  withNativeAllocators((make) => {
    const owner = make();
    nativeGlobal.__ubrnHandleAllocator = owner.hooks;
    const callbacks = new UniffiHandleMap<string>();
    const continuations = new UniffiHandleMap<string>(
      "rust-future-continuation",
    );
    const futures = new UniffiHandleMap<string>("foreign-future-drop");
    const handles = [
      callbacks.insert("callback"),
      continuations.insert("poll"),
      futures.insert("future"),
    ];
    t.assertEqual(new Set(handles).size, 3);
    t.assertEqual(
      owner.kinds.join(","),
      "callback,rust-future-continuation,foreign-future-drop",
    );
    const clone = callbacks.clone(handles[0]);
    t.assertNotEqual(clone, handles[0]);
    callbacks.remove(handles[0]);
    callbacks.remove(handles[0]);
    t.assertEqual(owner.released.length, 1);
    t.assertEqual(callbacks.get(clone), "callback");
  });
});

test("old maps never adopt the replacement runtime allocator", (t) => {
  withNativeAllocators((make) => {
    const first = make();
    nativeGlobal.__ubrnHandleAllocator = first.hooks;
    const old = new UniffiHandleMap<string>();
    const retained = old.insert("old");
    first.stop();
    const second = make();
    nativeGlobal.__ubrnHandleAllocator = second.hooks;
    const current = new UniffiHandleMap<string>();
    const live = current.insert("new");
    t.assertNotEqual(live, retained);
    t.assertFalse(old.has(retained));
    t.assertThrows(isStaleHandleError, () => old.get(retained));
    t.assertThrows(isStaleHandleError, () => old.clone(retained));
    t.assertThrows(
      (e) => e instanceof Error && e.message.includes("stopped"),
      () => old.insert("too late"),
    );
    old.remove(retained);
    t.assertEqual(first.released.length, 1);
    t.assertEqual(second.released.length, 0);
    t.assertEqual(current.get(live), "new");
  });
});

test("a map may bind at first insert but never switches after allocating", (t) => {
  withNativeAllocators((make) => {
    delete nativeGlobal.__ubrnHandleAllocator;
    const early = new UniffiHandleMap<string>();
    const fallback = new UniffiHandleMap<string>();
    t.assertEqual(fallback.insert("node"), BigInt(1));
    const owner = make();
    nativeGlobal.__ubrnHandleAllocator = owner.hooks;
    t.assertEqual(early.insert("native"), BigInt(101));
    t.assertEqual(fallback.insert("still node"), BigInt(3));
  });
});

test("stale odd object handles never become Rust pointers", (t) => {
  let created = false;
  const factory = {
    create: (handle: bigint) => {
      created = true;
      return handle;
    },
  } as unknown as UniffiObjectFactory<bigint>;
  const converter = new FfiConverterObjectWithCallbacks(factory);
  t.assertThrows(isStaleHandleError, () => converter.lift(BigInt(99)));
  t.assertFalse(created);
  t.assertEqual(converter.lift(BigInt(100)), BigInt(100));
  t.assertTrue(created);
});

asyncTest(
  "failed poll registration releases its continuation and Rust future",
  async (t) => {
    const before = uniffiRustFutureHandleCount();
    let freed = 0;
    const error = new Error("poll registration failed");
    try {
      await uniffiRustCallAsync(
        new UniffiRustCaller(() => ({ code: 0 })),
        () => BigInt(2),
        () => {
          throw error;
        },
        () => {},
        () => 0,
        () => {
          ++freed;
        },
        (value) => value,
        () => "",
      );
      t.fail("failed poll unexpectedly completed");
    } catch (caught) {
      t.assertTrue(caught === error);
    }
    t.assertEqual(uniffiRustFutureHandleCount(), before);
    t.assertEqual(freed, 1);
    t.end();
  },
);
