/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

//! Helpers for tests that run metadata for one crate through the general
//! pipeline, as `cli.rs` does.

use uniffi_bindgen::pipeline::{general, initial::UniffiMetaConverter};
use uniffi_meta::{
    CustomTypeMetadata, EnumMetadata, EnumShape, FieldMetadata, Metadata, NamespaceMetadata,
    RecordMetadata, Type, VariantMetadata,
};

/// The crate and the namespace of the metadata.
pub(super) const CRATE: &str = "test_crate";

pub(super) fn field(name: &str, ty: Type) -> FieldMetadata {
    FieldMetadata {
        name: name.into(),
        orig_name: None,
        ty,
        default: None,
        docstring: None,
    }
}

/// A variant with fields named `v0`, `v1` and so on.
pub(super) fn variant(name: &str, fields: Vec<Type>) -> VariantMetadata {
    VariantMetadata {
        name: name.into(),
        orig_name: None,
        discr: None,
        fields: fields
            .into_iter()
            .enumerate()
            .map(|(i, ty)| field(&format!("v{i}"), ty))
            .collect(),
        docstring: None,
    }
}

pub(super) fn enum_(name: &str, variants: Vec<VariantMetadata>) -> Metadata {
    Metadata::Enum(EnumMetadata {
        module_path: CRATE.into(),
        name: name.into(),
        orig_name: None,
        shape: EnumShape::Enum,
        remote: false,
        variants,
        discr_type: None,
        non_exhaustive: false,
        docstring: None,
    })
}

pub(super) fn record(name: &str, fields: Vec<FieldMetadata>) -> Metadata {
    Metadata::Record(RecordMetadata {
        module_path: CRATE.into(),
        name: name.into(),
        orig_name: None,
        remote: false,
        fields,
        docstring: None,
    })
}

/// The metadata of a custom type, and the type that refers to it.
pub(super) fn custom(name: &str, builtin: Type) -> (Metadata, Type) {
    let metadata = Metadata::CustomType(CustomTypeMetadata {
        module_path: CRATE.into(),
        name: name.into(),
        orig_name: None,
        builtin: builtin.clone(),
        docstring: None,
    });
    let ty = Type::Custom {
        module_path: CRATE.into(),
        name: name.into(),
        builtin: Box::new(builtin),
    };
    (metadata, ty)
}

pub(super) fn enum_ty(name: &str) -> Type {
    Type::Enum {
        module_path: CRATE.into(),
        name: name.into(),
    }
}

pub(super) fn record_ty(name: &str) -> Type {
    Type::Record {
        module_path: CRATE.into(),
        name: name.into(),
    }
}

/// Runs the namespace metadata and `items` through the general pipeline, and
/// returns the namespace.
pub(super) fn namespace(items: Vec<Metadata>) -> anyhow::Result<general::Namespace> {
    let mut converter = UniffiMetaConverter::default();
    converter.add_metadata_item(Metadata::Namespace(NamespaceMetadata {
        crate_name: CRATE.into(),
        name: CRATE.into(),
    }))?;
    for item in items {
        converter.add_metadata_item(item)?;
    }
    let root = general::pipeline("react-native").execute(converter.try_into_initial_ir()?)?;
    Ok(root.namespaces[CRATE].clone())
}
