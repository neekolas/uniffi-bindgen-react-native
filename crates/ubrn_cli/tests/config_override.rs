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

#[test]
fn config_override_changes_only_the_named_crate() -> Result<()> {
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

    let output = Utf8PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("config-override");
    let _ = fs::remove_dir_all(&output);
    fs::create_dir_all(&output)?;
    let config = output.join("override.toml");
    fs::write(
        &config,
        r#"[bindings.typescript.customTypes.Guid]
typeName = "string"
intoCustom = "intermediate + '-configured'"
fromCustom = "value"
"#,
    )?;

    for (name, override_config) in [("baseline", false), ("override", true)] {
        let ts_dir = output.join(name);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_uniffi-bindgen-react-native"));
        cmd.current_dir(&root).args([
            "generate",
            "napi",
            "bindings",
            "--lib-colocated",
            "--no-format",
            "--ts-dir",
            ts_dir.as_str(),
        ]);
        if override_config {
            cmd.args(["--crate", "ext_types_custom", "--config", config.as_str()]);
        }
        let result = cmd.arg(dylib.as_str()).output()?;
        anyhow::ensure!(
            result.status.success(),
            "{name} generation failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let baseline = output.join("baseline");
    let overridden = output.join("override");
    let baseline_custom = fs::read_to_string(baseline.join("ext_types_custom.ts"))?;
    let override_custom = fs::read_to_string(overridden.join("ext_types_custom.ts"))?;
    assert!(!baseline_custom.contains("'-configured'"));
    assert!(override_custom.contains("'-configured'"));

    for module in ["uniffi_one_ns.ts", "imported_types_lib.ts"] {
        let before = fs::read_to_string(baseline.join(module))
            .with_context(|| format!("read baseline {module}"))?;
        let after = fs::read_to_string(overridden.join(module))
            .with_context(|| format!("read overridden {module}"))?;
        assert_eq!(
            before, after,
            "{module} changed under another crate's config"
        );
    }
    Ok(())
}
