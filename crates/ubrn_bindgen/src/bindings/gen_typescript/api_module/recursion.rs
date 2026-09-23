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
//! `U -> T`, `V` is sometimes not marked. It also does not follow a custom
//! type to its builtin type, so it misses a cycle through a custom type.

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

    fn named(namespace: &str, name: &str) -> (String, String, String) {
        (namespace.into(), name.into(), name.into())
    }

    fn record(namespace: &str, name: &str) -> general::Type {
        let (namespace, name, orig_name) = named(namespace, name);
        general::Type::Record {
            namespace,
            name,
            orig_name,
        }
    }

    fn names_in(ty: &general::Type) -> Vec<String> {
        let mut names = BTreeSet::new();
        add_type_names("here", ty, &mut names);
        names.into_iter().collect()
    }

    #[test]
    fn edges_go_through_containers_and_custom_types() {
        let map = general::Type::Map {
            key_type: Box::new(record("here", "Key")),
            value_type: Box::new(general::Type::Box {
                inner_type: Box::new(record("here", "Value")),
            }),
        };
        assert_eq!(names_in(&map), ["Key", "Value"]);

        let (namespace, name, orig_name) = named("here", "Wrapped");
        let custom = general::Type::Custom {
            namespace,
            name,
            orig_name,
            builtin: Box::new(general::Type::Set {
                inner_type: Box::new(general::Type::Optional {
                    inner_type: Box::new(general::Type::Sequence {
                        inner_type: Box::new(record("here", "Inner")),
                    }),
                }),
            }),
        };
        assert_eq!(names_in(&custom), ["Inner"]);
    }

    #[test]
    fn objects_and_other_namespaces_are_not_edges() {
        assert!(names_in(&record("elsewhere", "Remote")).is_empty());
        let (namespace, name, orig_name) = named("here", "Object");
        let object = general::Type::Interface {
            namespace,
            name,
            orig_name,
            imp: general::ObjectImpl::Struct,
        };
        assert!(names_in(&object).is_empty());
        assert!(names_in(&general::Type::String).is_empty());
    }

    #[test]
    fn self_cycle_and_no_cycle() {
        assert_eq!(
            reaching(&[("List", &["List"]), ("Leaf", &[]), ("Uses", &["List"])]),
            ["List"]
        );
    }

    /// Runs metadata for one crate through the general pipeline, as `cli.rs`
    /// does, and returns the recursive enums that ubrn finds.
    mod pipeline {
        use super::super::super::test_metadata::*;
        use super::super::*;
        use uniffi_meta::{Metadata, ObjectImpl, ObjectMetadata, Type};

        fn boxed(inner: Type) -> Type {
            Type::Box {
                inner_type: Box::new(inner),
            }
        }

        fn recursive_enums() -> anyhow::Result<Vec<String>> {
            let object = Type::Object {
                module_path: CRATE.into(),
                name: "Obj".into(),
                imp: ObjectImpl::Struct,
            };
            let (wrapped, wrapped_ty) = custom(
                "Wrapped",
                Type::Sequence {
                    inner_type: Box::new(enum_ty("ViaCustom")),
                },
            );
            let items = vec![
                // Recursive through a `HashMap`.
                enum_(
                    "ViaMap",
                    vec![
                        variant(
                            "N",
                            vec![Type::Map {
                                key_type: Box::new(Type::String),
                                value_type: Box::new(enum_ty("ViaMap")),
                            }],
                        ),
                        variant("L", vec![]),
                    ],
                ),
                // Recursive through a custom type over `Vec<ViaCustom>`.
                wrapped,
                enum_(
                    "ViaCustom",
                    vec![variant("N", vec![wrapped_ty]), variant("L", vec![])],
                ),
                // An object is not an edge, even when its methods could
                // return the enum.
                Metadata::Object(ObjectMetadata {
                    module_path: CRATE.into(),
                    name: "Obj".into(),
                    orig_name: None,
                    remote: false,
                    imp: ObjectImpl::Struct,
                    docstring: None,
                }),
                enum_("HoldsObject", vec![variant("N", vec![object])]),
                // `T -> U -> T` and `T -> V -> U`. The uniffi-rs flag can
                // miss `V`.
                enum_(
                    "T",
                    vec![
                        variant("A", vec![record_ty("U")]),
                        variant("B", vec![enum_ty("V")]),
                        variant("Z", vec![]),
                    ],
                ),
                record(
                    "U",
                    vec![field(
                        "t",
                        Type::Optional {
                            inner_type: Box::new(boxed(enum_ty("T"))),
                        },
                    )],
                ),
                enum_("V", vec![variant("X", vec![record_ty("U")])]),
                // Uses a recursive enum, but is not in a cycle.
                enum_("Outside", vec![variant("A", vec![enum_ty("T")])]),
            ];
            let mut names: Vec<String> = recursive_enum_names(&namespace(items)?)
                .into_iter()
                .collect();
            names.sort();
            Ok(names)
        }

        #[test]
        fn finds_the_recursive_enums_of_a_namespace() -> anyhow::Result<()> {
            // uniffi-rs starts its search in `HashMap` order, so run the
            // pipeline many times: the result must not change.
            for _ in 0..20 {
                assert_eq!(recursive_enums()?, ["T", "V", "ViaCustom", "ViaMap"]);
            }
            Ok(())
        }
    }
}
