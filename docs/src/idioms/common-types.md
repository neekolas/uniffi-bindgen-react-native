For usage in Rust on how to use uniffi's proc-macros, see the `uniffi-rs` book for [Procedural Macros: Attributes and Derives](https://mozilla.github.io/uniffi-rs/latest/proc_macro/index.html).

This section is about how the generated Typescript maps onto the Rust idioms available.

A useful way of organizing this is via the types that can be passed across the FFI.

### Simple scalar types

|   | Rust | Typescript |   |
| - | ---- | ---------- | - |
| Unsigned integers | `u8`, `u16`, `u32` | `number` | Positive numbers only |
| Signed integers   | `i8`, `i16`, `i32` | `number` | |
| Floating point    | `f32`, `f64` | `number` | |
| 64 bit integers   | `u64`, `i64` | `bigint` | [MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt) |
| Strings           | `String` | `string` | UTF-8 encoded |

### Other simple types

|   | Rust | Typescript |   |
| - | ---- | ---------- | - |
| Byte array | `Vec<u8>` | `ArrayBuffer` | [MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/ArrayBuffer) |
| Borrowed byte array | `&[u8]`, UDL `[ByRef] bytes` | `ArrayBuffer` | Arguments only. See [below](#borrowed-byte-arrays). |
| Timestamp | `std::time::SystemTime` | `Date` | aliased to `UniffiTimestamp` |
| Duration | `std::time::Duration` | `number` ms | aliased to `UniffiDuration`


With `strictByteArrays = true` in `uniffi.toml`, both byte array types are `Uint8Array` instead.

#### Borrowed byte arrays

A `&[u8]` argument is borrowed by Rust for one call. Rust must not keep the slice after the call returns.

| | React Native (JSI) | Node (napi) | `web` (wasm-bindgen) | `wasm2` |
| - | - | - | - | - |
| How Rust gets the bytes | A pointer into the JS buffer, no copy | A pointer into the JS buffer, no copy | One copy into wasm memory | One copy into wasm memory, freed after the call |
| A view over a `SharedArrayBuffer` | Copied first (Hermes has no `SharedArrayBuffer`) | Copied first | Copied | Copied |
| A `Uint8Array` that is detached before the call | Empty slice | Empty slice | `TypeError` | Empty slice |
| The buffer is detached or shrunk by JS that runs while the arguments are converted (for example a getter) | Empty slice | Empty slice | Not possible | Not possible |
| JS changes, transfers, detaches or resizes the buffer while Rust runs (for example from a callback that Rust calls) | **Not allowed:** Rust can read freed memory | **Not allowed:** Rust can read freed memory | Rust does not see it | Rust does not see it |

With the default `ArrayBuffer` type, the bindings make a `Uint8Array` view of the argument, and a detached `ArrayBuffer` throws a `TypeError` on every flavor.

An `async fn` cannot take a `&[u8]` argument. uniffi-bindgen-react-native stops with an error if it finds one. Use an owned `Vec<u8>` argument.

### Structural types

|   | Rust | Typescript |   |
| - | ---- | ---------- | - |
| Optional | `Option<T>` | `T \| undefined` | |
| Sequences   | `Vec<T>` | `Array<T>` | Max length is 2**31 - 1|
| Maps    | `HashMap<K, V>` <br/> `BTreeMap<K, V>` | `Map<K, V>` | Max length is 2**31 - 1 |

### Enumerated types

|   | Rust | Typescript |   |
| - | ---- | ---------- | - |
| [Enums](./enums.md#enums-without-properties)    | `enum` | `enum` | [Flat enums](./enums.md#enums-without-properties)
| [Tagged Union Types](./enums.md#enums-with-properties) | `enum` | Tagged unions | [Enums with properties](./enums.md#enums-with-properties)
| [Error enums](./errors.md#enums-as-errors) | `enums` | `Error` | |

### Struct types

|   | Rust | Typescript | |
| - | ---------- | ---- | - |
| [Objects](./objects.md) | `struct Foo {}` | `class Foo` | class objects with methods
| [Records](./records.md) | `struct Bar {}` | `type Bar = {}` | objects without methods
| [Error objects](./errors.md#objects-as-errors) | `struct Baz {}` | `Error` | object is a property of the `Error` |
