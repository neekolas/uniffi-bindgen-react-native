/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

//! Find the enums that refer to themselves.
//!
//! uniffi-rs 0.32 sets `recursive` on each enum and record in a cycle, but we
//! do not use that flag. Its depth-first search skips a node that it has
//! visited before, and it starts from the nodes in `HashMap` order. So an
//! enum that is in a cycle can be missed, and the result can change from one
//! run to the next. For example, with `T -> U`, `T -> V`, `V -> U` and
//! `U -> T`, `V` is sometimes not marked.

use std::collections::{BTreeSet, HashMap, HashSet};

use uniffi_bindgen::pipeline::general;

/// The names of the enums in `namespace` that can reach themselves through
/// the fields of enums and records. The path can go through `Box`, `Option`,
/// `Vec`, `HashSet`, `HashMap` and custom types.
pub(super) fn recursive_enum_names(namespace: &general::Namespace) -> HashSet<String> {
    let mut enums = Vec::new();
    let mut edges: HashMap<String, BTreeSet<String>> = HashMap::new();
    for td in &namespace.type_definitions {
        let (name, fields): (_, Vec<&general::Field>) = match td {
            general::TypeDefinition::Enum(e) => {
                enums.push(e.name.clone());
                (&e.name, e.variants.iter().flat_map(|v| &v.fields).collect())
            }
            general::TypeDefinition::Record(r) => (&r.name, r.fields.iter().collect()),
            _ => continue,
        };
        let deps = edges.entry(name.clone()).or_default();
        for field in fields {
            add_type_names(&namespace.name, &field.ty.ty, deps);
        }
    }
    enums
        .into_iter()
        .filter(|name| can_reach_itself(name, &edges))
        .collect()
}

/// Adds the names of the enums and records in `namespace` that `ty` refers to.
fn add_type_names(namespace: &str, ty: &general::Type, names: &mut BTreeSet<String>) {
    match ty {
        general::Type::Enum {
            namespace: ns,
            name,
            ..
        }
        | general::Type::Record {
            namespace: ns,
            name,
            ..
        } => {
            // A type from another crate cannot be in a cycle with this crate.
            if ns == namespace {
                names.insert(name.clone());
            }
        }
        general::Type::Box { inner_type }
        | general::Type::Optional { inner_type }
        | general::Type::Sequence { inner_type }
        | general::Type::Set { inner_type }
        | general::Type::Custom {
            builtin: inner_type,
            ..
        } => add_type_names(namespace, inner_type, names),
        general::Type::Map {
            key_type,
            value_type,
        } => {
            add_type_names(namespace, key_type, names);
            add_type_names(namespace, value_type, names);
        }
        _ => {}
    }
}

/// A search from `start` that marks each node once. The result does not
/// depend on the order of the nodes.
fn can_reach_itself(start: &str, edges: &HashMap<String, BTreeSet<String>>) -> bool {
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = vec![start];
    while let Some(node) = stack.pop() {
        for next in edges.get(node).into_iter().flatten() {
            if next == start {
                return true;
            }
            if seen.insert(next.as_str()) {
                stack.push(next);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(pairs: &[(&str, &[&str])]) -> HashMap<String, BTreeSet<String>> {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|v| v.to_string()).collect()))
            .collect()
    }

    fn reaching(pairs: &[(&str, &[&str])]) -> Vec<String> {
        let edges = graph(pairs);
        let mut names: Vec<String> = pairs
            .iter()
            .map(|(k, _)| k.to_string())
            .filter(|k| can_reach_itself(k, &edges))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_member_of_a_cycle_is_found_in_every_order() {
        // `V` is in the cycle `T -> V -> U -> T`. The uniffi-rs search can
        // miss it when it visits `T -> U -> T` first.
        let pairs: [(&str, &[&str]); 4] = [
            ("T", &["U", "V"]),
            ("U", &["T"]),
            ("V", &["U"]),
            ("Outside", &["T"]),
        ];
        let mut order = pairs.to_vec();
        for _ in 0..order.len() {
            order.rotate_left(1);
            for reversed in [false, true] {
                let mut o = order.clone();
                if reversed {
                    o.reverse();
                }
                assert_eq!(reaching(&o), ["T", "U", "V"]);
            }
        }
    }

    #[test]
    fn self_cycle_and_no_cycle() {
        assert_eq!(
            reaching(&[("List", &["List"]), ("Leaf", &[]), ("Uses", &["List"])]),
            ["List"]
        );
    }
}
