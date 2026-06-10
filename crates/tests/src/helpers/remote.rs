use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use super::cli::rheo_cli_command;

/// Clone a public GitHub repo using `git clone --depth 1`.
///
/// Destination: `crates/tests/store/compat/<name>/`.
/// If the destination already exists, skip cloning (fast local re-runs).
///
/// Returns the path to the cloned directory.
pub fn clone_repo(url: &str, name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dest = PathBuf::from(manifest_dir).join("store/compat").join(name);

    if dest.exists() {
        return dest;
    }

    fs::create_dir_all(&dest).unwrap_or_else(|e| {
        panic!(
            "Failed to create compat store directory {}: {}",
            dest.display(),
            e
        )
    });

    let output = Command::new("git")
        .args(["clone", "--depth", "1", url, dest.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("Failed to run git clone: {}", e));

    if !output.status.success() {
        panic!(
            "Failed to clone repo {}: {}\nstdout: {}\nstderr: {}",
            url,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    dest
}

/// Patch the `version` field in `<project>/rheo.toml` to match
/// `env!("CARGO_PKG_VERSION")`. Overrides whatever version the external
/// project declares, so version-mismatch errors don't mask real failures.
///
/// Does nothing if no rheo.toml is present.
pub fn patch_rheo_version(project_path: &Path) {
    let toml_path = project_path.join("rheo.toml");
    if !toml_path.exists() {
        return;
    }

    let content = fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", toml_path.display(), e));

    let version = env!("CARGO_PKG_VERSION");
    let had_trailing_newline = content.ends_with('\n');

    let patched: String = content
        .lines()
        .map(|line| {
            let key = line.trim_start().split('=').next().unwrap_or("").trim();
            if key == "version" {
                format!("version = \"{}\"", version)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let patched = if had_trailing_newline {
        patched + "\n"
    } else {
        patched
    };

    fs::write(&toml_path, patched)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", toml_path.display(), e));
}

/// Clone the repo, patch its version, run `rheo compile <project_path>`,
/// and panic with full stdout+stderr if exit code is non-zero or if errors
/// are present in the output.
pub fn run_compat(url: &str, name: &str) {
    let cloned_path = clone_repo(url, name);
    patch_rheo_version(&cloned_path);

    let output = rheo_cli_command()
        .args(["compile", cloned_path.to_str().unwrap()])
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
        .output()
        .unwrap_or_else(|e| panic!("Failed to run rheo compile: {}", e));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Check exit code
    if !output.status.success() {
        panic!("rheo compile failed for {} ({}):\n{}", name, url, combined);
    }

    // Check for ERROR strings in output
    if combined.contains("ERROR") || combined.contains("error:") {
        panic!(
            "rheo compile for {} ({}) produced errors despite success exit code:\n{}",
            name, url, combined
        );
    }
}
