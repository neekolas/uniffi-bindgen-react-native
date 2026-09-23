/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/
 */
mod metadata;
mod paths;
pub mod typescript;

pub mod jsi;
pub mod napi;
pub mod ts;
pub mod wasm;
pub mod wasm2;

/// Test flavor: JSI (Hermes native), WASM (Node.js), Napi (Node.js N-API), or Wasm2 (Player-based WASM).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Jsi,
    Wasm,
    Napi,
    Wasm2,
}

impl Flavor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Flavor::Jsi => "jsi",
            Flavor::Wasm => "wasm",
            Flavor::Napi => "napi",
            Flavor::Wasm2 => "wasm2",
        }
    }
}

use std::ffi::OsStr;
use std::process::Command;
use std::sync::Mutex;

use camino::{Utf8Path, Utf8PathBuf};

/// Serialize test flavors within a fixture.
///
/// Both JSI and WASM generate TypeScript bindings into the fixture's
/// `generated/` directory, so tests for different flavors of the same
/// fixture must not run concurrently. Since all tests for a fixture run
/// in the same test binary, an in-process mutex suffices.
static FIXTURE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock_fixture() -> std::sync::MutexGuard<'static, ()> {
    FIXTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// RAII guard that removes a file when dropped, ensuring cleanup even on panic.
pub(crate) struct CleanupFile(camino::Utf8PathBuf);

impl CleanupFile {
    pub(crate) fn new(path: camino::Utf8PathBuf) -> Self {
        Self(path)
    }
}

impl Drop for CleanupFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Extension trait to convert paths to forward slashes for use in file content.
///
/// On Windows, paths use backslashes which break certain tools.
pub(crate) trait ForwardSlashPath {
    fn to_forward_slash(&self) -> String;
}

impl ForwardSlashPath for Utf8Path {
    fn to_forward_slash(&self) -> String {
        self.as_str().replace('\\', "/")
    }
}

impl ForwardSlashPath for Utf8PathBuf {
    fn to_forward_slash(&self) -> String {
        self.as_path().to_forward_slash()
    }
}

/// Compute a relative path between two paths, using forward slashes.
///
/// On Windows, `diff_utf8_paths` returns backslash paths which break
/// certain tools
pub(crate) fn relative_path(path: impl AsRef<Utf8Path>, base: impl AsRef<Utf8Path>) -> Utf8PathBuf {
    let rel = pathdiff::diff_utf8_paths(path.as_ref(), base.as_ref()).unwrap_or_else(|| {
        panic!(
            "cannot compute relative path from {} to {}",
            base.as_ref(),
            path.as_ref()
        )
    });
    Utf8PathBuf::from(rel.to_forward_slash())
}

/// Create a [`Command`] that works on Windows for `.cmd`/`.bat` scripts.
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(program);
        cmd
    } else {
        Command::new(program)
    }
}

/// Run a command, inheriting stdout/stderr so output is visible.
pub(crate) fn run_cmd(cmd: &mut Command) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to launch {:?}: {e}", cmd.get_program()));

    if !status.success() {
        panic!("{:?} failed (exit status: {})", cmd.get_program(), status);
    }
}

/// Run a command, capturing stdout/stderr. Only display output on failure.
pub(crate) fn run_cmd_quietly(cmd: &mut Command) {
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to launch {:?}: {e}", cmd.get_program()));

    if !output.status.success() {
        eprintln!("Command failed: {cmd:?}");
        eprintln!("{}", String::from_utf8_lossy(&output.stdout));
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        panic!(
            "{:?} failed (exit status: {})",
            cmd.get_program(),
            output.status
        );
    }
}

/// `cargo build -p <crate_name>`
pub(crate) fn cargo_build(crate_name: &str) {
    run_cmd_quietly(
        Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg(crate_name)
            .arg("--lib"),
    );
}

/// Write a minimal tsconfig.json into the fixture directory so that tsx
/// can resolve `@generated/*` and `@/*` imports.
///
/// `@generated/*` resolves to `./generated/$flavor/ts/*`.
pub(crate) fn write_fixture_tsconfig(
    fixture_dir: &camino::Utf8Path,
    flavor: Flavor,
) -> camino::Utf8PathBuf {
    let entries = tsconfig_paths(fixture_dir, flavor, Resolver::Tsx);

    let tsconfig_path = fixture_dir.join("tsconfig.json");
    let contents = format!(
        r#"{{
  "compilerOptions": {{
    "baseUrl": ".",
    "paths": {{
      {entries}
    }}
  }}
}}
"#
    );
    std::fs::write(&tsconfig_path, contents).expect("failed to write fixture tsconfig.json");
    tsconfig_path
}

/// The tool that reads the `paths` of a tsconfig.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Resolver {
    /// tsx, which runs the JavaScript.
    Tsx,
    /// tsc, which reads the types.
    Tsc,
}

/// The `paths` entries of a fixture tsconfig, relative to `fixture_dir`.
fn tsconfig_paths(fixture_dir: &Utf8Path, flavor: Flavor, resolver: Resolver) -> String {
    let flavor_str = flavor.as_str();
    let rel_root = relative_path(paths::repo_root(), fixture_dir);
    let mut entries = vec![
        format!(r#""@/generated": ["./generated/{flavor_str}/ts"]"#),
        format!(r#""@/generated/*": ["./generated/{flavor_str}/ts/*"]"#),
        format!(r#""@/*": ["{rel_root}/typescript/testing/*"]"#),
        format!(r#""@ubjs/core": ["{rel_root}/typescript/src/index"]"#),
        // Defensive: kept so mid-rollout fixtures still resolve. Drop
        // once all generated fixtures are regenerated to import @ubjs/core.
        format!(r#""uniffi-bindgen-react-native": ["{rel_root}/typescript/src/index"]"#),
    ];
    if flavor == Flavor::Napi {
        // tsx loads `lib.js`. tsc reads the package directory, so it uses the
        // `types` file of `package.json`, the same as a project that installs
        // the package. tsx cannot load a directory.
        let node = match resolver {
            Resolver::Tsx => "runtimes/napi/lib",
            Resolver::Tsc => "runtimes/napi",
        };
        entries.push(format!(r#""@ubjs/node": ["{rel_root}/{node}"]"#));
    }
    if flavor == Flavor::Wasm2 {
        // `paths` bypasses the package `exports` map, so the bare specifier
        // the generated index imports needs pointing at the node build.
        entries.push(format!(
            r#""@ubjs/wasm": ["{rel_root}/runtimes/wasm/node/src/index"]"#
        ));
        entries.push(format!(
            r#""@ubjs/wasm/core": ["{rel_root}/runtimes/wasm/core/src/index"]"#
        ));
        entries.push(format!(
            r#""@ubjs/wasm/browser": ["{rel_root}/runtimes/wasm/browser/src/index"]"#
        ));
        entries.push(format!(
            r#""@ubjs/wasm/node": ["{rel_root}/runtimes/wasm/node/src/index"]"#
        ));
    }
    entries.join(",\n      ")
}

/// Type-check the test script and the generated bindings with strict `tsc`.
///
/// tsx removes the types and does not check them. JSI checks the types when
/// it compiles with `tsc` (see [`typescript::prepare_for_jsi`]). This function
/// does the same check for the flavors that run on Node.
///
/// The tsconfig goes in `generated/$flavor/`, next to the bindings. It has
/// the same `paths` as the fixture tsconfig, but `@ubjs/node` resolves to the
/// published types. It includes all of the generated TypeScript files, also
/// the files that the test script does not import. The `dom` lib declares
/// `WebAssembly`, which the wasm runtime uses.
pub(crate) fn run_tsc(fixture_dir: &Utf8Path, flavor: Flavor, test_script: &Utf8Path) {
    let entries = tsconfig_paths(fixture_dir, flavor, Resolver::Tsc);
    let test_script = test_script.to_forward_slash();

    let tsconfig_path = fixture_dir
        .join("generated")
        .join(flavor.as_str())
        .join("tsconfig.json");
    let contents = format!(
        r#"{{
  "compilerOptions": {{
    "baseUrl": "../..",
    "paths": {{
      {entries}
    }},
    "strict": true,
    "noEmit": true,
    "target": "es2022",
    "lib": ["es2024", "dom"],
    "module": "esnext",
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "skipLibCheck": true
  }},
  "files": ["{test_script}"],
  "include": ["ts/**/*.ts"]
}}
"#
    );
    std::fs::write(&tsconfig_path, contents).expect("failed to write tsconfig.json for tsc");

    let tsc = paths::node_modules_bin().join("tsc");
    run_cmd_quietly(command(&tsc).arg("--project").arg(tsconfig_path.as_str()));
}

/// Run a test script with tsx and experimental WASM module support.
pub(crate) fn run_tsx(test_script: &camino::Utf8Path) {
    let tsx = paths::node_modules_bin().join("tsx");
    run_cmd(
        command(&tsx)
            .arg("--experimental-wasm-modules")
            .arg(test_script.as_str()),
    );
}

/// As [`run_tsx`], but evaluates `preload` first. Node runs `--import` modules
/// to completion — including their top-level `await` — before the entry
/// module, which is what lets the preload finish opening the wasm before the
/// test script imports anything that needs it.
pub(crate) fn run_tsx_with_preload(test_script: &camino::Utf8Path, preload: &camino::Utf8Path) {
    let tsx = paths::node_modules_bin().join("tsx");
    run_cmd(
        command(&tsx)
            .arg("--experimental-wasm-modules")
            .arg("--import")
            .arg(format!("file://{preload}"))
            .arg(test_script.as_str()),
    );
}
