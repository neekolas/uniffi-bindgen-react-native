/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

pub enum Animal {
    Dog,
    Cat,
}

// Though it has the proc-macro, we drop the variant
// literals if there is not a repr type defined
#[derive(uniffi::Enum)]
pub enum AnimalNoReprInt {
    Dog = 3,
    Cat = 4,
}

#[repr(u8)]
#[derive(uniffi::Enum)]
pub enum AnimalUInt {
    Dog = 3,
    Cat = 4,
}

#[repr(u64)]
#[derive(uniffi::Enum)]
pub enum AnimalLargeUInt {
    Dog = 4294967298, // u32::MAX as u64 + 3
    Cat = 4294967299, // u32::MAX as u64 + 4
}

#[repr(i8)]
#[derive(Debug, uniffi::Enum)]
pub enum AnimalSignedInt {
    Dog = -3,
    Cat = -2,
    Koala,   // -1
    Wallaby, // 0
    Wombat,  // 1
}

// Two enums with the same Rust name in different modules. `#[uniffi(name)]`
// gives them different names. Only the first has a repr type, so only the
// first keeps its literals.
pub mod repr_color {
    #[repr(u16)]
    #[derive(uniffi::Enum)]
    #[uniffi(name = "ReprColor")]
    pub enum Color {
        Red = 1,
        Green = 2,
    }
}

pub mod no_repr_color {
    #[derive(uniffi::Enum)]
    #[uniffi(name = "NoReprColor")]
    pub enum Color {
        Red = 1,
        Green = 2,
    }
}

// uniffi.toml renames this enum. It keeps its literals.
#[repr(u32)]
#[derive(uniffi::Enum)]
pub enum AnimalTomlRenamed {
    Dog = 7,
    Cat, // 8
}

#[derive(uniffi::Record, Clone)]
pub struct AnimalRecord {
    value: u8,
}

#[derive(uniffi::Object)]
pub struct AnimalObject {
    pub value: AnimalRecord,
}

#[uniffi::export]
impl AnimalObject {
    #[uniffi::constructor]
    fn new(value: u8) -> Arc<Self> {
        Arc::new(Self {
            value: AnimalRecord { value },
        })
    }

    pub fn record(&self) -> AnimalRecord {
        self.value.clone()
    }
}

use std::sync::Arc;
// Adding an enum with a Associated Type that is a exported Arc<Object> with a exported Record field.
// This is done to check for compilation errors.
#[derive(uniffi::Enum)]
pub(crate) enum AnimalAssociatedType {
    Dog(Arc<AnimalObject>),
    Cat,
}

#[uniffi::export]
fn identity_enum_with_associated_type(value: AnimalAssociatedType) -> AnimalAssociatedType {
    value
}

#[uniffi::export]
fn identity_enum_with_named_associated_type(
    value: AnimalNamedAssociatedType,
) -> AnimalNamedAssociatedType {
    value
}

#[derive(uniffi::Enum)]
pub(crate) enum AnimalNamedAssociatedType {
    Dog { value: Arc<AnimalObject> },
    Cat,
}

#[uniffi::export]
fn get_animal(a: Option<Animal>) -> Animal {
    a.unwrap_or(Animal::Dog)
}

#[derive(uniffi::Enum)]
pub(crate) enum CollidingVariants {
    AnimalRecord(AnimalRecord),
    AnimalObjectInterface(Arc<AnimalObject>),
    AnimalObject(Arc<AnimalObject>),
    Animal(Animal),
    #[allow(clippy::enum_variant_names)]
    CollidingVariants,
}

#[uniffi::export]
fn identity_colliding_variants(value: CollidingVariants) -> CollidingVariants {
    value
}

#[derive(uniffi::Enum)]
pub(crate) enum OptionalFields {
    Named {
        required: String,
        maybe_string: Option<String>,
        maybe_record: Option<AnimalRecord>,
    },
    Empty,
}

#[uniffi::export]
fn identity_optional_fields(value: OptionalFields) -> OptionalFields {
    value
}

// A recursive enum: `Cons` holds a `Box<IntList>`. uniffi-rs 0.32 lifts and
// lowers `Box<T>`, so a recursive enum can cross the FFI.
#[derive(uniffi::Enum, Debug, Clone, PartialEq, Eq)]
pub enum IntList {
    Cons(i32, Box<IntList>),
    Nil,
}

#[uniffi::export]
fn identity_int_list(value: IntList) -> IntList {
    value
}

#[uniffi::export]
fn make_int_list(values: Vec<i32>) -> IntList {
    let mut list = IntList::Nil;
    for v in values.into_iter().rev() {
        list = IntList::Cons(v, Box::new(list));
    }
    list
}

#[uniffi::export]
fn int_list_sum(value: IntList) -> i32 {
    let mut sum = 0;
    let mut list = &value;
    while let IntList::Cons(v, rest) = list {
        sum += v;
        list = rest;
    }
    sum
}

// An enum and a record that refer to each other. `Expr` also refers to itself
// through a `Box`, and it has trait methods and a method.
#[derive(uniffi::Enum, Debug, Clone, PartialEq, Eq, Hash)]
#[uniffi::export(Debug, Eq, Hash)]
pub enum Expr {
    Num(i32),
    Negate { expr: Box<Expr> },
    Group(ExprGroup),
}

#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprGroup {
    pub label: String,
    pub items: Vec<Expr>,
}

#[uniffi::export]
impl Expr {
    pub fn eval(&self) -> i32 {
        match self {
            Expr::Num(n) => *n,
            Expr::Negate { expr } => -expr.eval(),
            Expr::Group(group) => group.items.iter().map(Expr::eval).sum(),
        }
    }
}

#[uniffi::export]
fn identity_expr(value: Expr) -> Expr {
    value
}

#[uniffi::export]
fn identity_expr_group(value: ExprGroup) -> ExprGroup {
    value
}

// Every enum in a cycle is recursive: `Route -> RouteLink -> Route`, and also
// `Route -> Detour -> RouteLink -> Route`. The uniffi-rs `recursive` flag can
// miss `Detour`, so ubrn finds the cycles itself.
#[derive(uniffi::Enum, Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Link(RouteLink),
    Detour(Detour),
    End,
}

#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq)]
pub struct RouteLink {
    pub next: Option<Box<Route>>,
}

#[derive(uniffi::Enum, Debug, Clone, PartialEq, Eq)]
pub enum Detour {
    Via(RouteLink),
}

#[uniffi::export]
fn identity_route(value: Route) -> Route {
    value
}

#[uniffi::export]
fn route_length(value: Route) -> u32 {
    let next = match value {
        Route::Link(link) | Route::Detour(Detour::Via(link)) => link.next,
        Route::End => return 0,
    };
    1 + next.map_or(0, |route| route_length(*route))
}

// A recursive error enum.
#[derive(uniffi::Error, thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ChainedError {
    #[error("root: {message}")]
    Root { message: String },
    #[error("wrapped at depth {depth}")]
    Wrapped {
        depth: u32,
        cause: Box<ChainedError>,
    },
}

#[uniffi::export]
fn fail_with_chain(depth: u32) -> Result<(), ChainedError> {
    let mut error = ChainedError::Root {
        message: "the cause".into(),
    };
    for d in 1..=depth {
        error = ChainedError::Wrapped {
            depth: d,
            cause: Box::new(error),
        };
    }
    Err(error)
}

uniffi::include_scaffolding!("enum_types");

#[cfg(test)]
mod test {
    use crate::AnimalSignedInt;

    #[test]
    fn check_signed() {
        assert_eq!(AnimalSignedInt::Koala as i8, -1);
        assert_eq!(AnimalSignedInt::Wallaby as i8, 0);
        assert_eq!(AnimalSignedInt::Wombat as i8, 1);
    }
}
