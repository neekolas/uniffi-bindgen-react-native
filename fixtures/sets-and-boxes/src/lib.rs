/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

//! `HashSet<T>`, `Box<T>` and recursive records, which uniffi-rs 0.32 added.

// This fixture passes a `Box<T>` across the FFI on purpose.
#![allow(
    clippy::boxed_local,
    clippy::box_collection,
    clippy::redundant_allocation
)]

#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Sets
// ---------------------------------------------------------------------------

#[uniffi::export]
fn identity_u32_set(value: HashSet<u32>) -> HashSet<u32> {
    value
}

#[uniffi::export]
fn identity_i64_set(value: HashSet<i64>) -> HashSet<i64> {
    value
}

#[uniffi::export]
fn identity_string_set(value: HashSet<String>) -> HashSet<String> {
    value
}

/// Rust removes duplicates when it makes the set.
#[uniffi::export]
fn string_set_from(values: Vec<String>) -> HashSet<String> {
    values.into_iter().collect()
}

#[uniffi::export]
fn string_set_contains(value: HashSet<String>, item: String) -> bool {
    value.contains(&item)
}

#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A JS `Set` compares records by reference, so it can hold two equal
/// records. Rust compares them by value.
#[uniffi::export]
fn point_set_len(value: HashSet<Point>) -> u32 {
    value.len() as u32
}

#[uniffi::export]
fn identity_point_set(value: HashSet<Point>) -> HashSet<Point> {
    value
}

#[uniffi::export]
fn identity_nested(value: Option<Vec<HashSet<String>>>) -> Option<Vec<HashSet<String>>> {
    value
}

#[uniffi::export]
fn identity_set_map(value: HashMap<String, HashSet<u32>>) -> HashMap<String, HashSet<u32>> {
    value
}

/// A custom type over a set.
pub struct Tags(pub HashSet<String>);
uniffi::custom_newtype!(Tags, HashSet<String>);

#[uniffi::export]
fn identity_tags(value: Tags) -> Tags {
    value
}

#[uniffi::export]
fn tags_len(value: Tags) -> u32 {
    value.0.len() as u32
}

/// A custom type over a custom type over a set.
pub struct TagList(pub Tags);
uniffi::custom_newtype!(TagList, Tags);

#[uniffi::export]
fn identity_tag_list(value: TagList) -> TagList {
    value
}

#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq)]
pub struct Labelled {
    pub name: String,
    pub labels: HashSet<String>,
    pub scores: Option<HashSet<i32>>,
}

#[uniffi::export]
fn identity_labelled(value: Labelled) -> Labelled {
    value
}

// ---------------------------------------------------------------------------
// Boxes
// ---------------------------------------------------------------------------

#[uniffi::export]
fn add_one_boxed(value: Box<i32>) -> Box<i32> {
    Box::new(*value + 1)
}

#[uniffi::export]
fn shout_boxed(value: Box<String>) -> Box<String> {
    Box::new(value.to_uppercase())
}

#[uniffi::export]
fn swap_boxed_point(value: Box<Point>) -> Box<Point> {
    Box::new(Point {
        x: value.y,
        y: value.x,
    })
}

#[uniffi::export]
fn identity_maybe_boxed_point(value: Option<Box<Point>>) -> Option<Box<Point>> {
    value
}

#[derive(uniffi::Object)]
pub struct Counter {
    value: i32,
}

#[uniffi::export]
impl Counter {
    #[uniffi::constructor]
    fn new(value: i32) -> Arc<Self> {
        Arc::new(Self { value })
    }

    fn value(&self) -> i32 {
        self.value
    }
}

/// A `Box<Arc<T>>` has the FFI type of an object handle, not a `RustBuffer`.
#[uniffi::export]
fn next_boxed_counter(value: Box<Arc<Counter>>) -> Box<Arc<Counter>> {
    Box::new(Counter::new(value.value + 1))
}

// ---------------------------------------------------------------------------
// Async
// ---------------------------------------------------------------------------

#[uniffi::export]
async fn async_identity_string_set(value: HashSet<String>) -> HashSet<String> {
    value
}

/// The async return has the FFI type of an `i32`, not a `RustBuffer`.
#[uniffi::export]
async fn async_add_one_boxed(value: Box<i32>) -> Box<i32> {
    Box::new(*value + 1)
}

// ---------------------------------------------------------------------------
// Recursive records
// ---------------------------------------------------------------------------

/// A record that has a recursive record in a `Box`.
#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq)]
pub struct AHolder {
    pub head: Box<LinkedNode>,
}

/// A record that refers to itself through `Option<Box<Self>>`.
///
/// Because of the cycle, uniffi-rs puts `Option<Box<LinkedNode>>` before
/// `Box<LinkedNode>` in its sorted type definitions. The generated converters
/// must still come in the opposite order.
#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq)]
pub struct LinkedNode {
    pub value: i32,
    pub next: Option<Box<LinkedNode>>,
}

#[uniffi::export]
fn linked_from(values: Vec<i32>) -> Option<LinkedNode> {
    let mut next: Option<Box<LinkedNode>> = None;
    for value in values.into_iter().rev() {
        next = Some(Box::new(LinkedNode { value, next }));
    }
    next.map(|node| *node)
}

#[uniffi::export]
fn linked_sum(value: LinkedNode) -> i32 {
    let mut sum = 0;
    let mut node = Some(&value);
    while let Some(n) = node {
        sum += n.value;
        node = n.next.as_deref();
    }
    sum
}

#[uniffi::export]
fn identity_linked(value: LinkedNode) -> LinkedNode {
    value
}

#[uniffi::export]
fn identity_holder(value: AHolder) -> AHolder {
    value
}

/// A record that refers to itself through `Vec<Self>`.
#[derive(uniffi::Record, Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub value: i32,
    pub children: Vec<TreeNode>,
}

#[uniffi::export]
fn tree_sum(value: TreeNode) -> i32 {
    value.value + value.children.into_iter().map(tree_sum).sum::<i32>()
}

#[uniffi::export]
fn identity_tree(value: TreeNode) -> TreeNode {
    value
}

uniffi::setup_scaffolding!();
