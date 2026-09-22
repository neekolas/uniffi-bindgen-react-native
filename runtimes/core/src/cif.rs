/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
//! Mapping from FfiTypeDesc to libffi::middle::Type.

use libffi::middle::Type;
use std::collections::HashMap;

use crate::spec::StructDef;
use crate::{Error, FfiTypeDesc, Result};

pub fn ffi_type_for(desc: &FfiTypeDesc, struct_defs: &HashMap<String, StructDef>) -> Result<Type> {
    match desc {
        FfiTypeDesc::UInt8 => Ok(Type::u8()),
        FfiTypeDesc::Int8 => Ok(Type::i8()),
        FfiTypeDesc::UInt16 => Ok(Type::u16()),
        FfiTypeDesc::Int16 => Ok(Type::i16()),
        FfiTypeDesc::UInt32 => Ok(Type::u32()),
        FfiTypeDesc::Int32 => Ok(Type::i32()),
        FfiTypeDesc::UInt64 | FfiTypeDesc::Handle => Ok(Type::u64()),
        FfiTypeDesc::Int64 => Ok(Type::i64()),
        FfiTypeDesc::Float32 => Ok(Type::f32()),
        FfiTypeDesc::Float64 => Ok(Type::f64()),
        FfiTypeDesc::VoidPointer
        | FfiTypeDesc::Reference(_)
        | FfiTypeDesc::MutReference(_)
        | FfiTypeDesc::Callback(_) => Ok(Type::pointer()),
        FfiTypeDesc::Void => Ok(Type::void()),
        // When RustCallStatus appears as a struct field, it is an inline value
        // with layout {i8, u64, u64, pointer} matching RustCallStatusC.
        // Function-level CIF builders push Type::pointer() directly for &mut args,
        // so this only affects struct field layout computation.
        FfiTypeDesc::RustCallStatus => Ok(Type::structure(vec![
            Type::i8(),
            Type::u64(),
            Type::u64(),
            Type::pointer(),
        ])),
        FfiTypeDesc::RustBuffer => Ok(Type::structure(vec![
            Type::u64(),
            Type::u64(),
            Type::pointer(),
        ])),
        // A `&[u8]` argument, passed by value with layout {i32, pointer},
        // matching ForeignBytesC.
        FfiTypeDesc::ForeignBytes => Ok(Type::structure(vec![Type::i32(), Type::pointer()])),
        FfiTypeDesc::Struct(name) => {
            let def = struct_defs
                .get(name)
                .ok_or_else(|| Error::UnknownStruct(name.clone()))?;
            let field_types = def
                .fields
                .iter()
                .map(|f| ffi_type_for(&f.field_type, struct_defs))
                .collect::<Result<Vec<_>>>()?;
            Ok(Type::structure(field_types))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ffi_c_types::ForeignBytesC, slot};
    use libffi::middle::{arg, Cif, CodePtr};

    extern "C" fn sum_bytes(bytes: ForeignBytesC, tag: i32) -> i32 {
        // SAFETY: the test passes a pointer and length into a live array.
        let slice = unsafe { std::slice::from_raw_parts(bytes.data, bytes.len as usize) };
        slice.iter().map(|b| i32::from(*b)).sum::<i32>() + tag
    }

    #[test]
    fn foreign_bytes_is_passed_by_value() {
        let data = [1u8, 2, 3, 4];
        let view = &data[1..3];
        let mut slot = [0u8; std::mem::size_of::<ForeignBytesC>()];
        slot::write_foreign_bytes(
            &mut slot,
            ForeignBytesC {
                len: view.len() as i32,
                data: view.as_ptr(),
            },
        );
        let tag = 100i32;
        let cif = Cif::new(
            [
                ffi_type_for(&FfiTypeDesc::ForeignBytes, &HashMap::new()).unwrap(),
                Type::i32(),
            ],
            Type::i32(),
        );
        // SAFETY: the CIF matches `sum_bytes`, and both arguments live for the call.
        let result: i32 = unsafe {
            cif.call(
                CodePtr::from_ptr(sum_bytes as *const _),
                &[arg(&slot), arg(&tag)],
            )
        };
        assert_eq!(result, 2 + 3 + 100);
    }
}
