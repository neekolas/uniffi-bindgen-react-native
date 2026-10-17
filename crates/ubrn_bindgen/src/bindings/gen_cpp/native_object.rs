/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use uniffi_bindgen::{interface::AsType, ComponentInterface};

use super::Config;

#[derive(Default, Debug)]
pub(super) struct NativeObjectHooks {
    pub started: BTreeMap<String, String>,
    pub completed: BTreeMap<String, String>,
    pub freed: BTreeMap<String, String>,
    pub discard: BTreeMap<String, String>,
}

impl NativeObjectHooks {
    pub fn resolve(ci: &ComponentInterface, config: &Config) -> Result<Self> {
        let mut hooks = Self::default();
        let Some(policy) = &config.native_object else {
            return Ok(hooks);
        };
        for name in [
            &policy.future_started,
            &policy.future_completed,
            &policy.future_freed,
            &policy.discard,
        ] {
            if !name.split("::").all(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }) {
                bail!("native object policy requires a qualified C++ function name: {name}");
            }
        }
        let object = ci.get_object_definition(&policy.object).with_context(|| {
            format!(
                "native object policy names unknown object {}",
                policy.object
            )
        })?;
        for callable in ci
            .iter_callables()
            .filter(|c| c.return_type() == Some(&object.as_type()))
        {
            if !callable.is_async() {
                bail!(
                    "native object policy requires an async return: {}",
                    callable.ffi_func().name()
                );
            }
            hooks.started.insert(
                callable.ffi_func().name().into(),
                policy.future_started.clone(),
            );
            hooks.completed.insert(
                callable.ffi_rust_future_complete(ci),
                policy.future_completed.clone(),
            );
            hooks.freed.insert(
                callable.ffi_rust_future_free(ci),
                policy.future_freed.clone(),
            );
        }
        if hooks.started.is_empty() {
            bail!(
                "native object policy has no async {} producers",
                policy.object
            );
        }
        hooks.discard.insert(
            object.ffi_object_free().name().into(),
            policy.discard.clone(),
        );
        Ok(hooks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(object: &str) -> Config {
        toml::from_str(&format!(
            r#"
            [nativeObject]
            object = "{object}"
            futureStarted = "owners::started"
            futureCompleted = "owners::completed"
            futureFreed = "owners::freed"
            discard = "owners::discard"
        "#
        ))
        .unwrap()
    }

    fn component(udl: &str) -> ComponentInterface {
        let mut ci = ComponentInterface::from_webidl(udl, "policy_test").unwrap();
        ci.derive_ffi_funcs().unwrap();
        ci
    }

    #[test]
    fn policy_resolves_constructor_function_and_method_without_other_abi_peers() {
        let ci = component(
            r#"
            namespace policy_test {
                [Async] Client from_function();
                [Async] Other other();
                [Async] u64 same_abi();
            };
            interface Client {
                [Async] constructor();
                [Async] Client from_method();
            };
            interface Other {};
        "#,
        );
        let hooks = NativeObjectHooks::resolve(&ci, &config("Client")).unwrap();
        assert_eq!(
            hooks.started.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "uniffi_policy_test_fn_constructor_client_new",
                "uniffi_policy_test_fn_func_from_function",
                "uniffi_policy_test_fn_method_client_from_method",
            ]
        );
        assert_eq!(
            hooks
                .completed
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["ffi_policy_test_rust_future_complete_u64",]
        );
        assert_eq!(
            hooks.freed.keys().map(String::as_str).collect::<Vec<_>>(),
            ["ffi_policy_test_rust_future_free_u64",]
        );
        assert_eq!(
            hooks.discard.keys().map(String::as_str).collect::<Vec<_>>(),
            ["uniffi_policy_test_fn_free_client",]
        );
    }

    #[test]
    fn policy_rejects_unknown_object_sync_results_and_invalid_hook_names() {
        let ci = component("namespace policy_test { Client sync_client(); }; interface Client {};");
        assert!(NativeObjectHooks::resolve(&ci, &config("Missing"))
            .unwrap_err()
            .to_string()
            .contains("unknown object Missing"));
        assert!(NativeObjectHooks::resolve(&ci, &config("Client"))
            .unwrap_err()
            .to_string()
            .contains("requires an async return"));
        let mut config = config("Client");
        config.native_object.as_mut().unwrap().discard = "owners::discard(pointer)".into();
        assert!(NativeObjectHooks::resolve(&ci, &config)
            .unwrap_err()
            .to_string()
            .contains("qualified C++ function name"));
    }
}
