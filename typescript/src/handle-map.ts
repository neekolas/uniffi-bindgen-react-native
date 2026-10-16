/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
import { UniffiInternalError } from "./errors.ts";

export type UniffiHandle = bigint;
export const defaultUniffiHandle = BigInt("0");

type HandleKind =
  "callback" | "rust-future-continuation" | "foreign-future-drop";

// Installed only by the React Native JSI runtime. The hooks retain their original
// runtime owner; an existing map must never adopt a replacement runtime's hooks.
interface NativeHandleAllocator {
  allocate(kind: HandleKind): UniffiHandle;
  release(handle: UniffiHandle): void;
  owns(handle: UniffiHandle): boolean;
}

function nativeHandleAllocator(): NativeHandleAllocator | undefined {
  return (
    globalThis as typeof globalThis & {
      __ubrnHandleAllocator?: NativeHandleAllocator;
    }
  ).__ubrnHandleAllocator;
}

export class UniffiHandleMap<T> {
  private map = new Map<UniffiHandle, T>();
  // Foreign handles must be odd values per uniffi 0.30.x:
  // "Foreign handles are generated with a handle map that only generates odd values."
  // "Foreign handles must always have the lowest bit set"
  // "0 is an invalid value."
  private currentHandle: UniffiHandle = BigInt(1);
  private allocator: NativeHandleAllocator | null | undefined =
    nativeHandleAllocator();

  constructor(private readonly kind: HandleKind = "callback") {}

  insert(value: T): UniffiHandle {
    // Some generated modules create their maps before native registration. Bind
    // lazily once in that case. null permanently selects the Node/Web allocator.
    if (this.allocator === undefined) {
      this.allocator = nativeHandleAllocator() ?? null;
    }
    const handle = this.allocator
      ? this.allocator.allocate(this.kind)
      : this.currentHandle;
    if (
      handle <= BigInt(0) ||
      handle > BigInt("18446744073709551615") ||
      handle % BigInt(2) !== BigInt(1)
    ) {
      throw new Error("Foreign handle must be an odd unsigned 64-bit value");
    }
    if (this.map.has(handle)) {
      throw new Error("Native allocator returned a live foreign handle");
    }
    this.map.set(handle, value);
    if (!this.allocator) this.currentHandle += BigInt(2); // stay odd
    return handle;
  }

  get(handle: UniffiHandle): T {
    const obj = this.map.get(handle);
    if (
      !this.map.has(handle) ||
      (this.allocator && !this.allocator.owns(handle))
    ) {
      // A Rust-held handle may outlive its original JS runtime. Its replacement
      // must reject the handle instead of resolving it in another runtime's map.
      throw new UniffiInternalError.UnexpectedStaleHandle();
    }
    return obj as T;
  }

  /**
   * Creates a second handle pointing to the same object. Rust calls this (via
   * `CallbackInterfaceClone`) when it clones an Arc holding a foreign-implemented
   * trait object, so two Arc references can exist with independent lifetimes.
   *
   * This clones the *handle*, not the object itself. Both handles resolve to the
   * same JS object reference, and removing either handle does not affect the other.
   */
  clone(handle: UniffiHandle): UniffiHandle {
    return this.insert(this.get(handle));
  }

  remove(handle: UniffiHandle): T | undefined {
    const obj = this.map.get(handle);
    if (this.map.delete(handle)) {
      this.allocator?.release(handle);
    }
    return obj;
  }

  has(handle: UniffiHandle): boolean {
    return (
      this.map.has(handle) && (!this.allocator || this.allocator.owns(handle))
    );
  }

  get size(): number {
    return this.map.size;
  }
}
