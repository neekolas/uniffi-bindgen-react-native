/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
mod callback_validation;
mod config;
mod extensions;
mod filters;
mod native_future;
mod util;

use std::borrow::Borrow;

use anyhow::Result;
use askama::Template;
use ubrn_common::CrateMetadata;
use uniffi_bindgen::interface::{FfiDefinition, FfiType};
use uniffi_bindgen::ComponentInterface;

use self::extensions::{
    CppComponentInterfaceExt as _, CppFfiCallbackFunctionExt as _, CppFfiFieldExt as _,
    CppFfiFunctionExt as _, CppFfiStructExt as _, CppFfiTypeExt as _,
};
pub(crate) use self::{config::CppConfig as Config, util::format_directory};
use crate::bindings::{
    extensions::{
        ComponentInterfaceExt, FfiCallbackFunctionExt, FfiStructExt, FfiTypeExt, ObjectExt,
    },
    metadata::ModuleMetadata,
};

pub(crate) fn generate_hpp(
    ci: &ComponentInterface,
    config: &Config,
    module: &ModuleMetadata,
) -> Result<String> {
    callback_validation::validate_callback_discard(ci)?;
    Ok(HppWrapper::new(ci, config, module).render()?)
}

pub(crate) fn generate_cpp(
    ci: &ComponentInterface,
    config: &Config,
    module: &ModuleMetadata,
) -> Result<String> {
    callback_validation::validate_callback_discard(ci)?;
    Ok(CppWrapper::new(ci, config, module, native_future::generate(ci)?).render()?)
}

#[derive(Template)]
#[template(syntax = "hpp", escape = "none", path = "wrapper.hpp")]
struct HppWrapper<'a> {
    ci: &'a ComponentInterface,
    #[allow(unused)]
    config: &'a Config,
    module: &'a ModuleMetadata,
}

impl<'a> HppWrapper<'a> {
    fn new(ci: &'a ComponentInterface, config: &'a Config, module: &'a ModuleMetadata) -> Self {
        Self { ci, config, module }
    }
}

#[derive(Template)]
#[template(syntax = "cpp", escape = "none", path = "wrapper.cpp")]
struct CppWrapper<'a> {
    native_future_disposal: String,
    ci: &'a ComponentInterface,
    #[allow(unused)]
    config: &'a Config,
    module: &'a ModuleMetadata,
}

impl<'a> CppWrapper<'a> {
    fn new(
        ci: &'a ComponentInterface,
        config: &'a Config,
        module: &'a ModuleMetadata,
        native_future_disposal: String,
    ) -> Self {
        Self {
            ci,
            config,
            module,
            native_future_disposal,
        }
    }
}

pub fn generate_entrypoint(
    _crate: &CrateMetadata,
    modules: &Vec<ModuleMetadata>,
) -> Result<String> {
    let index = EntrypointCpp { modules };
    Ok(index.render()?)
}

#[derive(Template)]
#[template(path = "entrypoint.cpp", escape = "none")]
struct EntrypointCpp<'a> {
    modules: &'a Vec<ModuleMetadata>,
}

#[cfg(test)]
mod tests;
