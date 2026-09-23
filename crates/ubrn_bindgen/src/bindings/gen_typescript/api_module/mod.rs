/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

//! Pre-computed IR for the `wrapper.ts` template.
//!
//! Built from `uniffi_bindgen::pipeline::general::Namespace`.
//! Templates branch on pre-computed booleans and strings but still
//! assemble output structure (method bodies, class shapes).

mod builders;
mod docstring;
mod nodes;
mod type_helpers;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use heck::ToUpperCamelCase;
use uniffi_bindgen::pipeline::general;

use crate::{
    bindings::gen_typescript::{ffi_module, Config},
    switches::AbiFlavor,
};

use self::builders::*;
use self::docstring::format_docstring;
use self::nodes::*;

pub(crate) use self::nodes::{
    InitializationIR, TsCallable, TsCallbackInterface, TsCustomType, TsEnum, TsExternalType,
    TsFunction, TsObject, TsRecord, TsSimpleWrapper, TsTypeDefinition, TsUniffiTrait,
};

pub(crate) struct TsApiModule {
    pub module_name: String,
    pub namespace_docstring: Option<String>,
    pub strict_type_checking: bool,
    pub flavor: AbiFlavor,
    pub is_debug: bool,
    pub is_verbose: bool,
    pub supports_rust_backtrace: bool,
    pub console_import: Option<String>,
    pub file_imports: Vec<TsFileImport>,
    pub converter_imports: Vec<TsConverterImport>,
    pub exported_converters: BTreeSet<String>,
    pub type_definitions: Vec<TsTypeDefinition>,
    pub functions: Vec<TsFunction>,
    pub initialization: InitializationIR,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ImportedItem {
    Type(String),
    Value(String),
}

pub(crate) struct ImportAccumulator {
    imports: BTreeMap<String, BTreeSet<ImportedItem>>,
    exported_converters: BTreeSet<String>,
    imported_converters: BTreeMap<(String, String), BTreeSet<String>>,
}

impl ImportAccumulator {
    pub fn new() -> Self {
        Self {
            imports: BTreeMap::new(),
            exported_converters: BTreeSet::new(),
            imported_converters: BTreeMap::new(),
        }
    }

    pub fn add_infra_value(&mut self, name: &str) {
        self.imports
            .entry("@ubjs/core".into())
            .or_default()
            .insert(ImportedItem::Value(name.into()));
    }

    pub fn add_infra_type(&mut self, name: &str) {
        self.imports
            .entry("@ubjs/core".into())
            .or_default()
            .insert(ImportedItem::Type(name.into()));
    }

    pub fn add_ext_value(&mut self, name: &str, namespace: &str) {
        self.imports
            .entry(format!("./{namespace}"))
            .or_default()
            .insert(ImportedItem::Value(name.into()));
    }

    pub fn add_ext_type(&mut self, name: &str, namespace: &str) {
        self.imports
            .entry(format!("./{namespace}"))
            .or_default()
            .insert(ImportedItem::Type(name.into()));
    }

    pub fn add_custom_value(&mut self, name: &str, from: &str) {
        self.imports
            .entry(from.into())
            .or_default()
            .insert(ImportedItem::Value(name.into()));
    }

    pub fn add_exported_converter(&mut self, name: &str) {
        self.exported_converters.insert(name.into());
    }

    pub fn add_imported_converter(&mut self, converter: &str, namespace: &str) {
        let src = format!("./{namespace}");
        let converters = format!("uniffi{}Module", namespace.to_upper_camel_case());
        self.imported_converters
            .entry((src, converters))
            .or_default()
            .insert(converter.into());
    }

    pub fn merge(&mut self, other: Self) {
        for (k, v) in other.imports {
            self.imports.entry(k).or_default().extend(v);
        }
        self.exported_converters.extend(other.exported_converters);
        self.imported_converters.extend(other.imported_converters);
    }

    pub fn collect_base_imports(&mut self) {
        self.add_infra_value("RustBuffer");
        self.add_infra_value("UniffiInternalError");
        self.add_infra_value("UniffiRustCaller");
    }

    pub fn collect_primitive(&mut self, config: &Config, ty: &general::Type) {
        if matches!(ty, general::Type::String) {
            return;
        }
        if let Some(name) = type_helpers::ffi_converter_name_for_type(config, ty) {
            self.add_infra_value(&name);
        }
        match ty {
            general::Type::Timestamp => self.add_infra_type("UniffiTimestamp"),
            general::Type::Duration => self.add_infra_type("UniffiDuration"),
            _ => {}
        }
    }

    pub fn collect_type_definition(&mut self, td: &TsTypeDefinition) {
        match td {
            TsTypeDefinition::SimpleWrapper(w) => self.collect_simple_wrapper(w),
            TsTypeDefinition::StringHelper(_) => self.collect_string_helper(),
            TsTypeDefinition::Custom(c) => self.collect_custom(c),
            TsTypeDefinition::External(e) => self.collect_external(e),
            TsTypeDefinition::FlatEnum(e)
            | TsTypeDefinition::FlatError(e)
            | TsTypeDefinition::TaggedEnum(e) => self.collect_enum(e),
            TsTypeDefinition::Record(r) => self.collect_record(r),
            TsTypeDefinition::Object(o) => self.collect_object(o),
            TsTypeDefinition::CallbackInterface(cbi) => self.collect_callback_interface(cbi),
        }
    }

    fn collect_simple_wrapper(&mut self, w: &TsSimpleWrapper) {
        self.add_infra_value(&w.infra_class);
    }

    fn collect_string_helper(&mut self) {
        self.add_infra_type("UniffiByteArray");
        self.add_infra_value("uniffiCreateFfiConverterString");
    }

    fn collect_custom(&mut self, c: &TsCustomType) {
        self.add_infra_type("FfiConverter");
        self.add_infra_type("RustBufferAllocator");
        self.add_infra_value("Cursor");
        self.add_infra_value("uniffiTypeNameSymbol");
        if let Some(cfg) = &c.custom_config {
            for (name, from) in &cfg.imports {
                self.add_custom_value(name, from);
            }
        }
        self.add_exported_converter(&c.ffi_converter_name);
    }

    fn collect_external(&mut self, e: &TsExternalType) {
        if e.is_enum_type {
            self.add_ext_value(&e.type_name, &e.module_path);
        } else {
            self.add_ext_type(&e.type_name, &e.module_path);
        }
        self.add_imported_converter(&e.converter_name, &e.module_path);
    }

    fn collect_enum(&mut self, e: &TsEnum) {
        self.add_infra_value("AbstractFfiConverterByteArray");
        self.add_infra_value("Cursor");
        self.add_infra_value("UniffiInternalError");
        if !e.is_flat {
            self.add_infra_value("uniffiTypeNameSymbol");
        }
        if e.is_error {
            self.add_infra_value("UniffiError");
            self.add_infra_value("uniffiTypeNameSymbol");
            self.add_infra_value("variantOrdinalSymbol");
        } else {
            self.add_infra_value("UniffiEnum");
        }

        self.add_exported_converter(&e.ffi_converter_name);

        if e.has_callables() {
            self.add_infra_value("uniffiTypeNameSymbol");
        }
        if e.variants
            .iter()
            .any(|v| v.has_field_defaults && !v.has_nameless_fields)
        {
            self.add_infra_value("uniffiCreateRecord");
        }
        self.collect_uniffi_traits(&e.uniffi_traits);
        self.collect_callables(&e.constructors);
        self.collect_callables(&e.methods);
    }

    fn collect_record(&mut self, r: &TsRecord) {
        self.add_infra_value("uniffiCreateRecord");
        self.add_infra_value("AbstractFfiConverterByteArray");
        self.add_infra_value("Cursor");
        self.add_exported_converter(&r.ffi_converter_name);

        if r.has_callables() {
            self.add_infra_value("uniffiTypeNameSymbol");
        }
        self.collect_uniffi_traits(&r.uniffi_traits);
        self.collect_callables(&r.constructors);
        self.collect_callables(&r.methods);
    }

    fn collect_uniffi_traits(&mut self, traits: &Vec<TsUniffiTrait>) {
        for ut in traits {
            match ut {
                TsUniffiTrait::Display { method }
                | TsUniffiTrait::Debug { method }
                | TsUniffiTrait::Hash { method }
                | TsUniffiTrait::Ord { cmp: method } => self.collect_callable(method),
                TsUniffiTrait::Eq { eq, ne } => {
                    self.collect_callable(eq);
                    self.collect_callable(ne);
                }
            }
        }
    }

    fn collect_callables(&mut self, callables: &Vec<TsCallable>) {
        for callable in callables {
            self.collect_callable(callable);
        }
    }

    fn collect_object(&mut self, o: &TsObject) {
        self.add_infra_value("UniffiAbstractObject");
        self.add_infra_type("UniffiHandle");
        self.add_infra_value("FfiConverterObject");
        self.add_infra_type("UniffiObjectFactory");
        self.add_infra_type("FfiConverter");
        self.add_infra_type("UniffiGcObject");
        self.add_infra_value("destructorGuardSymbol");
        self.add_infra_value("pointerLiteralSymbol");
        self.add_infra_value("uniffiTypeNameSymbol");

        if o.is_error {
            self.add_infra_value("UniffiThrownObject");
        }

        if o.has_callback_interface {
            self.add_infra_value("FfiConverterObjectWithCallbacks");
        }

        if o.is_error {
            self.add_infra_value("FfiConverterObjectAsError");
            self.add_exported_converter(&o.ffi_error_converter_name);
        }

        self.add_exported_converter(&o.ffi_converter_name);

        if let Some(ref ctor) = o.primary_constructor {
            self.collect_callable(ctor);
        }
        for ctor in &o.alternate_constructors {
            self.collect_callable(ctor);
        }
        for method in &o.methods {
            self.collect_callable(method);
        }
        self.collect_uniffi_traits(&o.uniffi_traits);

        if let Some(ref vtable) = o.vtable {
            self.collect_vtable_imports(vtable);
        }
    }

    fn collect_callback_interface(&mut self, cbi: &TsCallbackInterface) {
        self.add_infra_value("FfiConverterCallback");
        self.collect_vtable_imports(&cbi.vtable);
        for method in &cbi.methods {
            self.collect_callable(method);
        }
    }

    fn collect_vtable_imports(&mut self, vtable: &TsVtable) {
        self.add_infra_type("UniffiHandle");
        self.add_infra_type("UniffiReferenceHolder");
        self.add_infra_type("UniffiByteArray");
        self.add_infra_value("UniffiResult");
        self.add_infra_type("UniffiRustCallStatus");

        for field in &vtable.fields {
            if let Some(ref method) = field.method {
                if method.is_ffi_async() {
                    if method.is_throwing() {
                        self.add_infra_value("uniffiTraitInterfaceCallAsyncWithError");
                    } else {
                        self.add_infra_value("uniffiTraitInterfaceCallAsync");
                    }
                } else if method.is_throwing() {
                    self.add_infra_value("uniffiTraitInterfaceCallWithError");
                } else {
                    self.add_infra_value("uniffiTraitInterfaceCall");
                }
            }
        }
    }

    fn collect_callable(&mut self, callable: &TsCallable) {
        if callable.is_ffi_async() {
            self.add_infra_value("uniffiRustCallAsync");
        }
    }

    pub fn collect_verbose_imports(&mut self, has_async: bool) {
        self.add_infra_type("UniffiHandle");
        self.add_infra_type("UniffiRustCallStatus");
        if has_async {
            self.add_infra_type("UniffiRustFutureContinuationCallback");
        }
    }
}

/// A converter that `build_type_definitions` writes after the base types.
struct DeferredWrapper {
    name: String,
    inner_names: Vec<String>,
    def: TsTypeDefinition,
}

impl DeferredWrapper {
    fn new<'a>(
        self_type: &general::TypeNode,
        inner: impl IntoIterator<Item = &'a general::TypeNode>,
        def: TsTypeDefinition,
    ) -> Self {
        Self {
            name: self_type.canonical_name.clone(),
            inner_names: inner
                .into_iter()
                .map(|t| t.canonical_name.clone())
                .collect(),
            def,
        }
    }
}

/// Puts each deferred converter after the deferred converters that it uses.
///
/// Each deferred converter is a `const` that reads its inner converter when
/// the module loads. If the inner converter comes later, this is a
/// temporal-dead-zone error.
///
/// uniffi-rs sorts the type definitions so that dependencies come first. But
/// with a recursive type, the cycle can put a converter before its inner
/// converter: for example, `Optional<Box<Node>>` before `Box<Node>`. This sort
/// does not change an order that is already correct.
fn sort_deferred_wrappers(wrappers: Vec<DeferredWrapper>) -> Vec<TsTypeDefinition> {
    fn visit(
        i: usize,
        wrappers: &[DeferredWrapper],
        index: &HashMap<&str, usize>,
        visited: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if visited[i] {
            return;
        }
        visited[i] = true;
        for inner in &wrappers[i].inner_names {
            if let Some(&j) = index.get(inner.as_str()) {
                visit(j, wrappers, index, visited, order);
            }
        }
        order.push(i);
    }

    let index: HashMap<&str, usize> = wrappers
        .iter()
        .enumerate()
        .map(|(i, w)| (w.name.as_str(), i))
        .collect();
    let mut visited = vec![false; wrappers.len()];
    let mut order = Vec::with_capacity(wrappers.len());
    for i in 0..wrappers.len() {
        visit(i, &wrappers, &index, &mut visited, &mut order);
    }

    let mut defs: Vec<Option<TsTypeDefinition>> =
        wrappers.into_iter().map(|w| Some(w.def)).collect();
    order
        .into_iter()
        .map(|i| defs[i].take().expect("each index is in the order once"))
        .collect()
}

impl TsApiModule {
    fn build_type_definitions(
        config: &Config,
        namespace: &general::Namespace,
        explicit_discr_enums: &HashSet<String>,
        flavor: &AbiFlavor,
    ) -> Vec<TsTypeDefinition> {
        let mut defs = Vec::new();

        let ffi_fn_types: HashMap<String, &general::FfiFunctionType> = namespace
            .ffi_definitions
            .iter()
            .filter_map(|def| match def {
                general::FfiDefinition::FunctionType(ft) => Some((ft.name.0.clone(), ft)),
                _ => None,
            })
            .collect();

        let mut string_helper_emitted = false;

        // Defer wrapper FfiConverters (Optional/Sequence/Set/Box/Map) until after base types
        // to avoid temporal-dead-zone errors where a wrapper references a converter
        // that hasn't been initialised yet.
        let mut deferred_wrappers: Vec<DeferredWrapper> = Vec::new();

        for td in &namespace.type_definitions {
            match td {
                general::TypeDefinition::Simple(node) => {
                    if matches!(node.ty, general::Type::String) && !string_helper_emitted {
                        string_helper_emitted = true;
                        defs.push(TsTypeDefinition::StringHelper(build_string_helper(flavor)));
                    }
                }
                general::TypeDefinition::Optional(opt) => {
                    deferred_wrappers.push(DeferredWrapper::new(
                        &opt.self_type,
                        [&opt.inner],
                        TsTypeDefinition::SimpleWrapper(build_optional(config, opt)),
                    ));
                }
                general::TypeDefinition::Sequence(seq) => {
                    deferred_wrappers.push(DeferredWrapper::new(
                        &seq.self_type,
                        [&seq.inner],
                        TsTypeDefinition::SimpleWrapper(build_sequence(config, seq)),
                    ));
                }
                general::TypeDefinition::Set(set) => {
                    deferred_wrappers.push(DeferredWrapper::new(
                        &set.self_type,
                        [&set.inner],
                        TsTypeDefinition::SimpleWrapper(build_set(config, set)),
                    ));
                }
                general::TypeDefinition::Box(boxed) => {
                    deferred_wrappers.push(DeferredWrapper::new(
                        &boxed.self_type,
                        [&boxed.inner],
                        TsTypeDefinition::SimpleWrapper(build_box(config, boxed)),
                    ));
                }
                general::TypeDefinition::Map(map) => {
                    deferred_wrappers.push(DeferredWrapper::new(
                        &map.self_type,
                        [&map.key, &map.value],
                        TsTypeDefinition::SimpleWrapper(build_map(config, map)),
                    ));
                }
                general::TypeDefinition::Custom(custom) => {
                    let td = TsTypeDefinition::Custom(build_custom_type(config, custom));
                    if matches!(
                        custom.builtin.ty,
                        general::Type::Map { .. }
                            | general::Type::Sequence { .. }
                            | general::Type::Set { .. }
                            | general::Type::Box { .. }
                            | general::Type::Optional { .. }
                    ) {
                        deferred_wrappers.push(DeferredWrapper::new(
                            &custom.self_type,
                            [&custom.builtin],
                            td,
                        ));
                    } else {
                        defs.push(td);
                    }
                }
                general::TypeDefinition::External(ext) => {
                    defs.push(TsTypeDefinition::External(build_external_type(config, ext)));
                }
                general::TypeDefinition::Enum(e) => {
                    let has_explicit_discr =
                        explicit_discr_enums.contains(&e.self_type.canonical_name);
                    let ts_enum = build_enum(config, e, has_explicit_discr, flavor);
                    if ts_enum.is_flat && ts_enum.is_error {
                        defs.push(TsTypeDefinition::FlatError(ts_enum));
                    } else if ts_enum.is_flat {
                        defs.push(TsTypeDefinition::FlatEnum(ts_enum));
                    } else {
                        defs.push(TsTypeDefinition::TaggedEnum(ts_enum));
                    }
                }
                general::TypeDefinition::Record(r) => {
                    defs.push(TsTypeDefinition::Record(build_record(config, r, flavor)));
                }
                general::TypeDefinition::Interface(i) => {
                    defs.push(TsTypeDefinition::Object(Box::new(build_object(
                        config,
                        i,
                        flavor,
                        &ffi_fn_types,
                        config.strict_object_types,
                    ))));
                }
                general::TypeDefinition::CallbackInterface(cbi) => {
                    defs.push(TsTypeDefinition::CallbackInterface(
                        build_callback_interface(config, cbi, &ffi_fn_types, flavor),
                    ));
                }
            }
        }

        defs.extend(sort_deferred_wrappers(deferred_wrappers));

        defs
    }

    fn collect_all_imports(&self) -> ImportAccumulator {
        let mut acc = ImportAccumulator::new();
        acc.collect_base_imports();

        for td in &self.type_definitions {
            acc.collect_type_definition(td);
        }

        for func in &self.functions {
            acc.collect_callable(func);
        }

        if self.is_verbose {
            let has_async = self.functions.iter().any(|f| f.is_ffi_async())
                || self.type_definitions.iter().any(|td| match td {
                    TsTypeDefinition::Object(o) => {
                        o.methods.iter().any(|m| m.is_ffi_async())
                            || o.primary_constructor
                                .as_ref()
                                .is_some_and(|c| c.is_ffi_async())
                            || o.alternate_constructors.iter().any(|c| c.is_ffi_async())
                    }
                    TsTypeDefinition::CallbackInterface(cbi) => cbi.has_async_methods,
                    _ => false,
                });
            if has_async {
                acc.collect_verbose_imports(true);
            } else {
                let has_any_callables = !self.functions.is_empty()
                    || self.type_definitions.iter().any(|td| {
                        matches!(
                            td,
                            TsTypeDefinition::Object(_) | TsTypeDefinition::CallbackInterface(_)
                        )
                    });
                if has_any_callables {
                    acc.collect_verbose_imports(false);
                }
            }
        }

        acc
    }

    pub(crate) fn from_general(
        config: &Config,
        namespace: &general::Namespace,
        flavor: AbiFlavor,
        ffi_exported_definitions: Vec<ffi_module::FfiExportedName>,
        explicit_discr_enums: &HashSet<String>,
    ) -> anyhow::Result<Self> {
        let module_name = namespace.name.clone();
        let namespace_docstring = namespace.docstring.as_deref().map(format_docstring);
        let supports_rust_backtrace = flavor.supports_rust_backtrace();
        let type_definitions =
            Self::build_type_definitions(config, namespace, explicit_discr_enums, &flavor);
        let functions = build_functions(config, namespace, &flavor);
        let initialization = build_initialization(namespace, &flavor);

        let mut primitive_imports = ImportAccumulator::new();
        for td in &namespace.type_definitions {
            if let general::TypeDefinition::Simple(node) = td {
                primitive_imports.collect_primitive(config, &node.ty);
            }
        }

        let mut module = Self {
            module_name,
            namespace_docstring,
            strict_type_checking: config.strict_type_checking,
            flavor,
            is_debug: config.is_debug(),
            is_verbose: config.is_verbose(),
            supports_rust_backtrace,
            console_import: config.console_import.clone(),
            file_imports: Vec::new(),
            converter_imports: Vec::new(),
            exported_converters: BTreeSet::new(),
            type_definitions,
            functions,
            initialization,
        };

        let mut acc = module.collect_all_imports();
        acc.merge(primitive_imports);

        // Build file imports: FFI types first, then cross-module imports
        let mut file_imports = Vec::new();

        if !ffi_exported_definitions.is_empty() {
            file_imports.push(TsFileImport {
                path: format!("./{}-ffi", module.module_name),
                types: ffi_exported_definitions
                    .iter()
                    .map(|def| def.name().to_string())
                    .collect(),
                values: Vec::new(),
            });
        }

        for (file, things) in acc.imports {
            let mut types = Vec::new();
            let mut values = Vec::new();
            for thing in things {
                match thing {
                    ImportedItem::Type(t) => types.push(t),
                    ImportedItem::Value(v) => values.push(v),
                }
            }
            file_imports.push(TsFileImport {
                path: file,
                types,
                values,
            });
        }

        module.file_imports = file_imports;

        // Build converter imports
        module.converter_imports = acc
            .imported_converters
            .into_iter()
            .map(|((path, default_name), converters)| TsConverterImport {
                path,
                default_name,
                converters: converters.into_iter().collect(),
            })
            .collect();

        module.exported_converters = acc.exported_converters;

        validate_force_async(&module.type_definitions)?;

        Ok(module)
    }
}

/// Reject `forceAsync` on a callback interface or `WithForeign` trait interface
/// that has any synchronous method.
///
/// Rust calls into these types through a vtable, and each slot's sync/async ABI
/// is fixed by the Rust method. On an ordinary outbound call, `forceAsync` only
/// has to widen the return value into an already-resolved promise. Here it would
/// instead hand a promise to a synchronous vtable slot, which has no way to
/// await it. So every method must be async in Rust already.
pub(super) fn validate_force_async(type_definitions: &[TsTypeDefinition]) -> anyhow::Result<()> {
    let mut blocks = Vec::new();
    for td in type_definitions {
        match td {
            TsTypeDefinition::CallbackInterface(cbi) if cbi.force_async => {
                if let Some(block) =
                    force_async_error_block("callback interface", &cbi.ts_name, &cbi.methods)
                {
                    blocks.push(block);
                }
            }
            TsTypeDefinition::Object(o) if o.force_async && o.has_callback_interface => {
                if let Some(block) =
                    force_async_error_block("trait interface", &o.ts_name, &o.methods)
                {
                    blocks.push(block);
                }
            }
            _ => {}
        }
    }
    if blocks.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(blocks.join("\n\n"))
    }
}

/// `Some(error)` when `methods` has any non-async member; `None` when all async.
fn force_async_error_block(kind: &str, name: &str, methods: &[TsCallable]) -> Option<String> {
    let sync: Vec<&str> = methods
        .iter()
        .filter(|m| !m.is_ffi_async())
        .map(|m| m.name.as_str())
        .collect();
    if sync.is_empty() {
        return None;
    }
    let list = sync
        .iter()
        .map(|m| format!("  - {m}"))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "forceAsync targets {kind} `{name}`, but these methods are synchronous:\n\
         {list}\n\
         A {kind} can only be forced async if every method is already async in Rust.\n\
         Mark them `async fn` in the Rust trait, or remove `{name}` from forceAsync."
    ))
}

/// Returns an error if the namespace uses something that this generator cannot
/// write.
///
/// `cli.rs` calls this for every flavour before it writes any file, so an
/// error leaves no partial output.
pub(crate) fn reject_unsupported(namespace: &general::Namespace) -> anyhow::Result<()> {
    reject_async_borrowed_bytes(namespace)
}

/// Rust borrows a `&[u8]` or UDL `[ByRef] bytes` argument as `ForeignBytes`
/// (pointer and length). The pointer is valid only until the call returns.
///
/// uniffi-rs 0.32 does not compile an `async fn` with such an argument on
/// native targets. On wasm32 with `wasm-unstable-single-threaded` it does
/// compile, but the Rust future reads the bytes after the call that started it
/// has returned. By then the bindings have freed their copy of the bytes, so
/// the future would read freed memory. This check stops that.
fn reject_async_borrowed_bytes(namespace: &general::Namespace) -> anyhow::Result<()> {
    let mut callables: Vec<(String, &general::Callable)> = namespace
        .functions
        .iter()
        .map(|f| (format!("function `{}`", f.callable.name), &f.callable))
        .collect();
    for td in &namespace.type_definitions {
        let (type_name, constructors, methods): (_, &[_], &[_]) = match td {
            general::TypeDefinition::Interface(i) => (&i.name, &i.constructors, &i.methods),
            general::TypeDefinition::Record(r) => (&r.name, &r.constructors, &r.methods),
            general::TypeDefinition::Enum(e) => (&e.name, &e.constructors, &e.methods),
            _ => continue,
        };
        callables.extend(constructors.iter().map(|c| {
            let name = format!("constructor `{type_name}.{}`", c.callable.name);
            (name, &c.callable)
        }));
        callables.extend(methods.iter().map(|m| {
            let name = format!("method `{type_name}.{}`", m.callable.name);
            (name, &m.callable)
        }));
    }
    for (what, callable) in callables {
        if callable.async_data.is_some() && callable.arguments.iter().any(|a| a.is_borrowed_bytes())
        {
            anyhow::bail!(
                "an async {what} in namespace `{}` takes a `&[u8]` / `[ByRef] bytes` \
                argument. uniffi-bindgen-react-native does not support this: on wasm32 \
                the Rust future reads the bytes after the call that passed them has \
                returned, when the memory is already freed. Use an owned `Vec<u8>` \
                argument instead.",
                namespace.name,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod borrowed_bytes_tests {
    use super::*;
    use uniffi_bindgen::pipeline::initial::UniffiMetaConverter;
    use uniffi_meta::{FnMetadata, FnParamMetadata, Metadata, NamespaceMetadata, Type};

    /// Runs one exported function through the same pipeline that `cli.rs`
    /// uses, then through `reject_unsupported`.
    fn check_function(is_async: bool, ty: Type, by_ref: bool) -> anyhow::Result<()> {
        let mut converter = UniffiMetaConverter::default();
        converter.add_metadata_item(Metadata::Namespace(NamespaceMetadata {
            crate_name: "bytes_crate".into(),
            name: "bytes_crate".into(),
        }))?;
        converter.add_metadata_item(Metadata::Func(FnMetadata {
            module_path: "bytes_crate".into(),
            name: "take_bytes".into(),
            orig_name: None,
            is_async,
            inputs: vec![FnParamMetadata {
                name: "bytes".into(),
                ty,
                by_ref,
                optional: false,
                default: None,
            }],
            return_type: None,
            throws: None,
            checksum: Some(0),
            docstring: None,
        }))?;
        let root = general::pipeline("react-native").execute(converter.try_into_initial_ir()?)?;
        reject_unsupported(&root.namespaces["bytes_crate"])
    }

    #[test]
    fn async_borrowed_bytes_is_an_error() {
        let err = check_function(true, Type::Bytes, true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("function `take_bytes`"), "message: {err}");
        assert!(err.contains("on wasm32"), "message: {err}");
    }

    #[test]
    fn sync_borrowed_bytes_is_ok() {
        check_function(false, Type::Bytes, true).unwrap();
    }

    #[test]
    fn async_owned_bytes_is_ok() {
        check_function(true, Type::Bytes, false).unwrap();
    }
}

#[cfg(test)]
mod deferred_wrapper_tests {
    use super::*;

    fn wrapper(name: &str, inner_names: &[&str]) -> DeferredWrapper {
        DeferredWrapper {
            name: name.into(),
            inner_names: inner_names.iter().map(|n| n.to_string()).collect(),
            def: TsTypeDefinition::SimpleWrapper(TsSimpleWrapper {
                infra_class: "FfiConverterOptional".into(),
                ffi_converter_name: format!("FfiConverter{name}"),
                type_label: name.into(),
                inner_converters: vec![],
            }),
        }
    }

    fn names(defs: Vec<TsTypeDefinition>) -> Vec<String> {
        defs.into_iter()
            .map(|def| match def {
                TsTypeDefinition::SimpleWrapper(w) => w.type_label,
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn inner_converter_moves_first() {
        // uniffi-rs gives this order for a record `Node` with an
        // `Option<Box<Node>>` field.
        let sorted = sort_deferred_wrappers(vec![
            wrapper("OptionalBoxTypeNode", &["BoxTypeNode"]),
            wrapper("SequenceString", &["String"]),
            wrapper("BoxTypeNode", &["TypeNode"]),
        ]);
        assert_eq!(
            names(sorted),
            ["BoxTypeNode", "OptionalBoxTypeNode", "SequenceString"]
        );
    }

    #[test]
    fn correct_order_does_not_change() {
        let order = [
            ("SetString", vec!["String"]),
            ("OptionalInt32", vec!["Int32"]),
            ("SequenceSetString", vec!["SetString"]),
            ("MapStringSetString", vec!["String", "SetString"]),
            ("OptionalSequenceSetString", vec!["SequenceSetString"]),
        ];
        let sorted = sort_deferred_wrappers(
            order
                .iter()
                .map(|(name, inner)| wrapper(name, inner))
                .collect(),
        );
        assert_eq!(
            names(sorted),
            order.iter().map(|(name, _)| *name).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod force_async_validation_tests {
    use super::*;

    fn callable(name: &str, ffi_async: bool) -> TsCallable {
        TsCallable {
            name: name.into(),
            docstring: None,
            arguments: vec![],
            return_type: None,
            throws: None,
            ffi_name: format!("ffi_{name}"),
            ffi_async: ffi_async.then(|| TsAsyncFfi {
                poll: "poll".into(),
                complete: "complete".into(),
                free: "free".into(),
                cancel: "cancel".into(),
            }),
            receiver: None,
            force_async: false,
        }
    }

    fn callback_interface(
        name: &str,
        force_async: bool,
        methods: Vec<TsCallable>,
    ) -> TsTypeDefinition {
        TsTypeDefinition::CallbackInterface(TsCallbackInterface {
            protocol_name: name.into(),
            ts_name: name.into(),
            ffi_converter_name: format!("FfiConverterType{name}"),
            trait_impl: format!("uniffiCallbackInterface{name}"),
            docstring: None,
            has_async_methods: methods.iter().any(|m| m.is_ffi_async()),
            methods,
            vtable: TsVtable {
                ffi_init_fn: String::new(),
                fields: vec![],
            },
            force_async,
        })
    }

    #[test]
    fn targeted_sync_callback_interface_is_an_error() {
        let defs = vec![callback_interface(
            "Logger",
            true,
            vec![callable("log", false), callable("flush", false)],
        )];
        let err = validate_force_async(&defs).unwrap_err().to_string();
        assert!(
            err.contains("callback interface `Logger`"),
            "message: {err}"
        );
        assert!(err.contains("- log"), "message: {err}");
        assert!(err.contains("- flush"), "message: {err}");
    }

    #[test]
    fn untargeted_sync_callback_interface_is_ok() {
        let defs = vec![callback_interface(
            "Logger",
            false,
            vec![callable("log", false)],
        )];
        assert!(validate_force_async(&defs).is_ok());
    }

    #[test]
    fn targeted_all_async_callback_interface_is_ok() {
        let defs = vec![callback_interface(
            "Logger",
            true,
            vec![callable("log", true), callable("flush", true)],
        )];
        assert!(validate_force_async(&defs).is_ok());
    }
}
