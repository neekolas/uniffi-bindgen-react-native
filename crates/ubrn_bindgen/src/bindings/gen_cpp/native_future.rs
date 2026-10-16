/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::extensions::CppComponentInterfaceExt;
use anyhow::{anyhow, bail, Result};
use std::collections::BTreeMap;
use std::fmt::Write;
use uniffi_bindgen::{
    interface::{AsType, FfiType, Type},
    ComponentInterface,
};

fn component<'a>(ci: &'a ComponentInterface, path: &str) -> Result<&'a ComponentInterface> {
    let name = path.split("::").next().unwrap_or(path);
    if name == ci.crate_name() {
        Ok(ci)
    } else {
        ci.find_component_interface(name)
            .ok_or_else(|| anyhow!("missing native result schema {path}"))
    }
}

struct Decoder<'a> {
    ci: &'a ComponentInterface,
    types: BTreeMap<Type, usize>,
    bodies: Vec<String>,
    objects: BTreeMap<String, String>,
}
impl Decoder<'_> {
    fn reader(&mut self, ty: &Type) -> Result<usize> {
        if let Some(id) = self.types.get(ty) {
            return Ok(*id);
        }
        let id = self.types.len();
        self.types.insert(ty.clone(), id);
        self.bodies.push(String::new());
        let body = match ty {
            Type::Boolean | Type::UInt8 | Type::Int8 => "r.skip(1);".into(),
            Type::UInt16 | Type::Int16 => "r.skip(2);".into(),
            Type::UInt32 | Type::Int32 | Type::Float32 => "r.skip(4);".into(),
            Type::UInt64 | Type::Int64 | Type::Float64 => "r.skip(8);".into(),
            Type::Timestamp | Type::Duration => "r.skip(12);".into(),
            Type::String | Type::Bytes => "r.skip(r.count());".into(),
            Type::Box { inner_type }
            | Type::Custom {
                builtin: inner_type,
                ..
            } => format!("read_{}(r);", self.reader(inner_type)?),
            Type::Optional { inner_type } => format!(
                "if (r.integer(1)) read_{}(r);",
                self.reader(inner_type)?
            ),
            Type::Sequence { inner_type } | Type::Set { inner_type } => format!(
                "auto count = r.count(); for (uint32_t i = 0; i < count; ++i) read_{}(r);",
                self.reader(inner_type)?
            ),
            Type::Map {
                key_type,
                value_type,
            } => format!(
                "auto count = r.count(); for (uint32_t i = 0; i < count; ++i) {{ read_{}(r); read_{}(r); }}",
                self.reader(key_type)?,
                self.reader(value_type)?
            ),
            Type::Object {
                module_path, name, ..
            } => {
                let owner = component(self.ci, module_path)?;
                let object = owner
                    .get_object_definition(name)
                    .ok_or_else(|| anyhow!("missing native object {name}"))?;
                let helper = if owner.crate_name() == self.ci.crate_name() {
                    name.clone()
                } else {
                    format!("{}_{}", owner.crate_name(), name)
                };
                self.objects.insert(
                    helper.clone(),
                    object.ffi_object_free().name().into(),
                );
                format!("discard_object_{helper}(r.integer(8));")
            }
            Type::Record { module_path, name } => {
                let fields: Vec<_> = component(self.ci, module_path)?
                    .get_record_definition(name)
                    .ok_or_else(|| anyhow!("missing native record {name}"))?
                    .fields()
                    .iter()
                    .map(AsType::as_type)
                    .collect();
                let mut body = String::new();
                for field in fields {
                    write!(body, "read_{}(r); ", self.reader(&field)?).unwrap();
                }
                body
            }
            Type::Enum { module_path, name } => {
                let variants: Vec<Vec<_>> = component(self.ci, module_path)?
                    .get_enum_definition(name)
                    .ok_or_else(|| anyhow!("missing native enum {name}"))?
                    .variants()
                    .iter()
                    .map(|v| v.fields().iter().map(AsType::as_type).collect())
                    .collect();
                let mut body = "switch (r.integer(4)) {".to_owned();
                for (index, fields) in variants.iter().enumerate() {
                    write!(body, "case {}: {{", index + 1).unwrap();
                    for field in fields {
                        write!(body, "read_{}(r); ", self.reader(field)?).unwrap();
                    }
                    body.push_str("break; }");
                }
                body.push_str("default: throw std::runtime_error(\"invalid native result enum\"); }");
                body
            }
            Type::CallbackInterface { name, .. } => {
                bail!("cannot discard native async callback handle {name}")
            }

        };
        self.bodies[id] = format!("static void read_{id}(Reader &r) {{ {body} }}\n");
        Ok(id)
    }
    fn discard(&mut self, ty: &Type, value: &str) -> Result<String> {
        if let Type::Custom { builtin, .. }
        | Type::Box {
            inner_type: builtin,
        } = ty
        {
            return self.discard(builtin, value);
        }
        match FfiType::from(ty) {
            FfiType::RustBuffer(_) => {
                // String and byte results contain no nested native handles.
                let read = if matches!(ty, Type::String | Type::Bytes) {
                    String::new()
                } else {
                    format!("Reader r({value}); read_{}(r);", self.reader(ty)?)
                };
                Ok(format!("{{ {read} free_buffer({value}); }}"))
            }
            FfiType::Handle => {
                let id = self.reader(ty)?;
                if let Type::Object {
                    module_path, name, ..
                } = ty
                {
                    let owner = component(self.ci, module_path)?;
                    let helper = if owner.crate_name() == self.ci.crate_name() {
                        name.clone()
                    } else {
                        format!("{}_{}", owner.crate_name(), name)
                    };
                    Ok(format!("discard_object_{helper}({value});"))
                } else {
                    bail!("unsupported native handle result {ty:?} ({id})")
                }
            }
            _ => Ok(String::new()),
        }
    }
}

pub(super) fn generate(ci: &ComponentInterface) -> Result<String> {
    let mut decoder = Decoder {
        ci,
        types: BTreeMap::new(),
        bodies: Vec::new(),
        objects: BTreeMap::new(),
    };
    let mut disposers = String::new();
    for callable in ci.iter_callables().filter(|c| c.is_async()) {
        let symbol = callable.ffi_func().name();
        let complete = callable.ffi_rust_future_complete(ci);
        let free = callable.ffi_rust_future_free(ci);
        writeln!(disposers, "static void dispose_{symbol}(uint64_t handle, bool completed) noexcept {{\n if (!completed) {{\n RustCallStatus status{{}};").unwrap();
        if let Some(ty) = callable.return_type() {
            writeln!(disposers, "auto value = {complete}(handle, &status);\nif (status.code == UNIFFI_CALL_STATUS_OK) {{ {} }}", decoder.discard(ty, "value")?).unwrap();
        } else {
            writeln!(disposers, "{complete}(handle, &status);").unwrap();
        }
        if let Some(ty) = callable.throws_type() {
            let id = decoder.reader(ty)?;
            writeln!(disposers, "if (status.code == UNIFFI_CALL_STATUS_ERROR) {{ Reader r(status.error_buf); read_{id}(r); }}").unwrap();
        }
        writeln!(
            disposers,
            "free_buffer(status.error_buf);\n }}\n {free}(handle);\n}}"
        )
        .unwrap();
    }
    let mut output = format!("namespace {}::native_discard {{\n", ci.cpp_namespace());
    output.push_str(r#"
// Rust produced these buffers, so their schema and lengths are trusted. Bounds
// checks make a schema mismatch fail before dereferencing unrelated memory.
struct Reader {
 const uint8_t *data; uint64_t remaining;
 explicit Reader(RustBuffer buffer): data(buffer.data), remaining(buffer.len) {}
 void skip(uint64_t n) { if (n > remaining) throw std::runtime_error("invalid native result length"); if (n) data += n; remaining -= n; }
 uint64_t integer(size_t n) { if (n > remaining) throw std::runtime_error("truncated native result"); uint64_t value = 0; for (size_t i = 0; i < n; ++i) value = (value << 8) | data[i]; skip(n); return value; }
 uint32_t count() { auto n = integer(4); if (n > INT32_MAX) throw std::runtime_error("negative native result count"); return static_cast<uint32_t>(n); }
};
static void free_buffer(RustBuffer buffer) { if (buffer.data) Bridging<RustBuffer>::rustbuffer_free(buffer); }
"#);
    for (helper, symbol) in &decoder.objects {
        writeln!(output, "extern \"C\" void {symbol}(uint64_t, RustCallStatus *);\nstatic void discard_object_{helper}(uint64_t pointer) {{\n // UBRN_DISCARD_OBJECT {helper}\n RustCallStatus status{{}};\n {symbol}(pointer, &status);\n free_buffer(status.error_buf);\n}}").unwrap();
    }
    for id in 0..decoder.bodies.len() {
        writeln!(output, "static void read_{id}(Reader &r);").unwrap();
    }
    for body in decoder.bodies {
        output.push_str(&body);
    }
    output.push_str(&disposers);
    output.push_str("}\n");
    Ok(output)
}
