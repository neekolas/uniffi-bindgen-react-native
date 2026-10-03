/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */

use std::{fs, process::Command};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;

fn repo_root() -> Result<Utf8PathBuf> {
    Ok(Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize_utf8()?)
}

fn fixture(test_name: &str) -> Result<(Utf8PathBuf, Utf8PathBuf, Utf8PathBuf)> {
    let root = repo_root()?;
    let build = Command::new("cargo")
        .current_dir(&root)
        .args([
            "build",
            "--locked",
            "--lib",
            "-p",
            "uniffi-fixture-ext-types",
        ])
        .output()?;
    anyhow::ensure!(
        build.status.success(),
        "fixture build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let target_dir = match std::env::var("CARGO_TARGET_DIR") {
        Ok(path) => {
            let path = Utf8PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
        Err(_) => root.join("target"),
    };
    let dylib = target_dir.join("debug").join(format!(
        "{}uniffi_ext_types_lib.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));
    anyhow::ensure!(dylib.exists(), "fixture library missing: {dylib}");

    let output = Utf8PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("config-override")
        .join(test_name);
    fs::create_dir_all(&output)?;
    Ok((root, dylib, output))
}

fn generate(
    root: &Utf8PathBuf,
    dylib: &Utf8PathBuf,
    output: &Utf8PathBuf,
    name: &str,
    config: Option<&Utf8PathBuf>,
    crate_name: Option<&str>,
) -> Result<std::process::Output> {
    let ts_dir = output.join(name);
    let _ = fs::remove_dir_all(&ts_dir);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_uniffi-bindgen-react-native"));
    cmd.current_dir(root).args([
        "generate",
        "napi",
        "bindings",
        "--lib-colocated",
        "--no-format",
        "--ts-dir",
        ts_dir.as_str(),
    ]);
    if let Some(crate_name) = crate_name {
        cmd.args(["--crate", crate_name]);
    }
    if let Some(config) = config {
        cmd.args(["--config", config.as_str()]);
    }
    cmd.arg(dylib.as_str()).output().map_err(Into::into)
}

fn assert_success(result: &std::process::Output) -> Result<()> {
    anyhow::ensure!(
        result.status.success(),
        "generation failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

fn assert_other_ts_equal(output: &Utf8PathBuf, target: &str) -> Result<()> {
    let baseline = output.join("baseline");
    let overridden = output.join("override");
    for entry in fs::read_dir(&baseline)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "ts") {
            continue;
        }
        let name = path.file_name().context("generated file has no name")?;
        let name = name.to_string_lossy();
        if name == format!("{target}.ts") || name == format!("{target}-ffi.ts") {
            continue;
        }
        assert!(
            fs::read(&path)? == fs::read(overridden.join(name.as_ref()))?,
            "{name} changed under another crate's config"
        );
    }
    Ok(())
}

#[test]
fn config_override_changes_only_the_named_crate() -> Result<()> {
    let (root, dylib, output) = fixture("explicit")?;
    let config = output.join("override.toml");
    fs::write(
        &config,
        r#"[bindings.typescript]
strictTypeChecking = true

[bindings.typescript.customTypes.Guid]
typeName = "string"
intoCustom = "intermediate + '-configured'"
fromCustom = "value"
"#,
    )?;

    assert_success(&generate(&root, &dylib, &output, "baseline", None, None)?)?;
    assert_success(&generate(
        &root,
        &dylib,
        &output,
        "override",
        Some(&config),
        Some("ext_types_custom"),
    )?)?;

    let baseline = output.join("baseline");
    let overridden = output.join("override");
    let baseline_custom = fs::read_to_string(baseline.join("ext_types_custom.ts"))?;
    let override_custom = fs::read_to_string(overridden.join("ext_types_custom.ts"))?;
    assert!(!baseline_custom.contains("'-configured'"));
    assert!(override_custom.contains("'-configured'"));

    assert_other_ts_equal(&output, "ext_types_custom")?;
    Ok(())
}

#[test]
fn config_selects_library_basename_and_distinct_namespace() -> Result<()> {
    let (root, dylib, output) = fixture("basename")?;
    let named_lib = output.join(format!(
        "{}uniffi_one.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));
    fs::copy(&dylib, &named_lib)?;
    let config = output.join("one.toml");
    fs::write(
        &config,
        "[bindings.typescript]\nstrictTypeChecking = true\nlogLevel = \"verbose\"\n",
    )?;
    assert_success(&generate(
        &root, &named_lib, &output, "baseline", None, None,
    )?)?;
    assert_success(&generate(
        &root,
        &named_lib,
        &output,
        "override",
        Some(&config),
        None,
    )?)?;
    assert!(
        fs::read(output.join("baseline/uniffi_one_ns.ts"))?
            != fs::read(output.join("override/uniffi_one_ns.ts"))?
    );
    assert_other_ts_equal(&output, "uniffi_one_ns")?;
    Ok(())
}

#[test]
fn ambiguous_library_uses_per_crate_configs() -> Result<()> {
    let (root, dylib, output) = fixture("megazord")?;
    let megazord = output.join(format!(
        "{}mymegazord.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));
    fs::copy(&dylib, &megazord)?;
    let config = output.join("unused.toml");
    fs::write(
        &config,
        "[bindings.typescript]\nstrictTypeChecking = false\n",
    )?;
    assert_success(&generate(
        &root, &megazord, &output, "baseline", None, None,
    )?)?;
    let result = generate(&root, &megazord, &output, "override", Some(&config), None)?;
    assert_success(&result)?;
    let warning = String::from_utf8_lossy(&result.stderr);
    assert_eq!(
        warning
            .matches("warning: cannot select a component for --config")
            .count(),
        1
    );
    for name in [
        "custom_types",
        "ext_types_custom",
        "uniffi_ext_types_lib",
        "uniffi_one",
        "uniffi_sublib",
    ] {
        assert!(warning.contains(name), "missing {name}: {warning}");
    }
    assert!(warning.contains("--crate <component>"));
    for entry in fs::read_dir(output.join("baseline"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "ts") {
            let name = path.file_name().context("generated file has no name")?;
            let name = name.to_str().context("generated file name is not UTF-8")?;
            assert!(
                fs::read(&path)? == fs::read(output.join("override").join(name))?,
                "{name} changed under ambiguous --config"
            );
        }
    }
    Ok(())
}

#[test]
fn explicit_unknown_crate_is_an_error() -> Result<()> {
    let (root, dylib, output) = fixture("invalid")?;
    let config = output.join("unknown.toml");
    fs::write(
        &config,
        "[bindings.typescript]\nforceAsync = [\"echoI32\"]\n",
    )?;
    let result = generate(
        &root,
        &dylib,
        &output,
        "unknown",
        Some(&config),
        Some("missing_component"),
    )?;
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("--crate missing_component is not a component"));
    Ok(())
}

#[test]
fn single_component_is_selected_without_name_match() -> Result<()> {
    let root = repo_root()?;
    let build = Command::new("cargo")
        .current_dir(&root)
        .args([
            "build",
            "--locked",
            "--lib",
            "-p",
            "uniffi-fixture-defaults",
        ])
        .output()?;
    assert_success(&build)?;
    let target_dir = match std::env::var("CARGO_TARGET_DIR") {
        Ok(path) if Utf8PathBuf::from(&path).is_absolute() => Utf8PathBuf::from(path),
        Ok(path) => root.join(path),
        Err(_) => root.join("target"),
    };
    let dylib = target_dir.join("debug").join(format!(
        "{}uniffi_defaults.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));
    let output = Utf8PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("config-override/single");
    fs::create_dir_all(&output)?;
    let renamed = output.join(format!(
        "{}my_single_library.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    ));
    fs::copy(dylib, &renamed)?;
    let config = output.join("single.toml");
    fs::write(
        &config,
        "[bindings.typescript]\nforceAsync = [\"echoI32\"]\n",
    )?;
    assert_success(&generate(&root, &renamed, &output, "baseline", None, None)?)?;
    assert_success(&generate(
        &root,
        &renamed,
        &output,
        "override",
        Some(&config),
        None,
    )?)?;
    let baseline = fs::read(output.join("baseline/uniffi_defaults.ts"))?;
    let overridden = fs::read(output.join("override/uniffi_defaults.ts"))?;
    assert_ne!(baseline, overridden);
    Ok(())
}
