use std::sync::atomic::{AtomicBool, Ordering};

use ecow::eco_format;
use tracing::info;
use typst_kit::files::FsRoot;
use typst_library::diag::{PackageError, PackageResult};
use typst_syntax::package::PackageSpec;

use crate::config::PathSource;

/// Serves a namespace from a directory on disk — a package's own working tree,
/// read in place. No caching and no sha: the tree IS the source, so an edit
/// shows up on the next build.
pub struct PathPackages {
    namespace: String,
    source: PathSource,
    announced: AtomicBool,
}

impl PathPackages {
    pub fn new(namespace: &str, source: PathSource) -> Self {
        Self {
            namespace: namespace.to_string(),
            source,
            announced: AtomicBool::new(false),
        }
    }

    /// The directory for `@<ns>/<name>:<version>` inside the configured root.
    pub fn obtain(&self, spec: &PackageSpec) -> PackageResult<FsRoot> {
        if !self.announced.swap(true, Ordering::Relaxed) {
            info!(
                "@{} resolves from {} (a directory on disk)",
                self.namespace,
                self.source.root.display(),
            );
        }

        let mut dir = self.source.root.clone();
        if !self.source.subdir.is_empty() {
            dir.push(&self.source.subdir);
        }
        dir.push(spec.name.as_str());
        dir.push(spec.version.to_string());

        if !dir.exists() {
            return Err(PackageError::Other(Some(eco_format!(
                "@{namespace}/{name}:{version} not found at {dir}",
                namespace = self.namespace,
                name = spec.name,
                version = spec.version,
                dir = dir.display(),
            ))));
        }
        Ok(FsRoot::new(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, version: &str) -> PackageSpec {
        format!("@demo/{name}:{version}").parse().unwrap()
    }

    #[test]
    fn obtain_returns_the_directory_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("demo/0.1.0");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("typst.toml"), "[package]\n").unwrap();

        let packages = PathPackages::new(
            "demo",
            PathSource {
                root: tmp.path().to_path_buf(),
                subdir: String::new(),
            },
        );
        let root = packages.obtain(&spec("demo", "0.1.0")).expect("obtain");
        assert!(root.path().join("typst.toml").exists());
    }

    #[test]
    fn obtain_errors_naming_the_expected_path_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let packages = PathPackages::new(
            "demo",
            PathSource {
                root: tmp.path().to_path_buf(),
                subdir: String::new(),
            },
        );
        let err = format!("{:?}", packages.obtain(&spec("demo", "0.2.0")).unwrap_err());
        assert!(err.contains(&tmp.path().join("demo/0.2.0").to_string_lossy().to_string()));
    }

    #[test]
    fn subdir_is_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("packages/demo/0.1.0");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("typst.toml"), "[package]\n").unwrap();

        let packages = PathPackages::new(
            "demo",
            PathSource {
                root: tmp.path().to_path_buf(),
                subdir: "packages".to_string(),
            },
        );
        assert!(packages.obtain(&spec("demo", "0.1.0")).is_ok());
    }
}
