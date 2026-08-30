use std::io::Cursor;

use ecow::eco_format;
use flate2::read::GzDecoder;
use typst_kit::downloader::{Downloader, SystemDownloader};
use typst_kit::files::FsRoot;
use typst_kit::packages::FsPackages;
use typst_library::diag::{PackageError, PackageResult};
use typst_syntax::package::PackageSpec;

use std::collections::HashMap;

use typst_kit::packages::SystemPackages;

use crate::config::{NamespaceSource, ReleasesSource};

mod git;

pub use git::GitPackages;

/// The download base `@rheo` uses when no `[packages.rheo]` table overrides it.
const REGISTRY_URL: &str = "https://github.com/freecomputinglab/rheo-packages/releases/download";

/// The user agent every package download identifies itself with.
const USER_AGENT: &str = concat!("rheo/", env!("CARGO_PKG_VERSION"));

/// Where one configured namespace is served from.
enum Backend {
    Repo(GitPackages),
    Releases(RheoPackages),
}

/// Resolves a package spec to a directory on disk, routing by namespace.
///
/// Built once per build and shared, because the repository backends memoise
/// their resolved sha — a resolver per file read would run `ls-remote` per file
/// read.
pub struct PackageResolver {
    /// Namespaces with a `[packages.<ns>]` table. Consulted FIRST, so
    /// `[packages.rheo]` overrides the built-in `@rheo` rather than losing to
    /// it — that override is how a project tests a branch of rheo-packages.
    configured: HashMap<String, Backend>,
    rheo: RheoPackages,
    universe: SystemPackages,
}

impl PackageResolver {
    pub fn new(sources: &HashMap<String, NamespaceSource>) -> Self {
        let configured = sources
            .iter()
            .map(|(namespace, source)| {
                let backend = match source {
                    NamespaceSource::Repo(repo) => {
                        Backend::Repo(GitPackages::new(namespace, repo.clone()))
                    }
                    NamespaceSource::Releases(releases) => {
                        Backend::Releases(RheoPackages::with_source(downloader(), releases.clone()))
                    }
                };
                (namespace.clone(), backend)
            })
            .collect();

        Self {
            configured,
            rheo: RheoPackages::new(downloader()),
            universe: SystemPackages::new(downloader()),
        }
    }

    pub fn obtain(&self, spec: &PackageSpec) -> PackageResult<FsRoot> {
        match self.configured.get(spec.namespace.as_str()) {
            Some(Backend::Repo(repo)) => repo.obtain(spec),
            Some(Backend::Releases(releases)) => releases.obtain(spec),
            None if spec.namespace == "rheo" => self.rheo.obtain(spec),
            None => self.universe.obtain(spec),
        }
    }

    /// Delete every cached repository checkout backing this project's
    /// namespaces, returning `(namespace, checkouts removed)` per namespace.
    ///
    /// Only the namespaces this project declares — another project's cache is
    /// none of its business.
    pub fn prune_checkouts(&self) -> Vec<(String, std::io::Result<usize>)> {
        let mut pruned: Vec<(String, std::io::Result<usize>)> = self
            .configured
            .iter()
            .filter_map(|(namespace, backend)| match backend {
                Backend::Repo(repo) => Some((namespace.clone(), repo.prune())),
                Backend::Releases(_) => None,
            })
            .collect();
        pruned.sort_by(|a, b| a.0.cmp(&b.0));
        pruned
    }

    /// Whether this namespace has a `[packages.<ns>]` table.
    pub fn is_configured(&self, namespace: &str) -> bool {
        self.configured.contains_key(namespace)
    }

    /// Whether this namespace is served from a repository ref rather than a
    /// releases host. A ref carries no build output, so its packages use their
    /// source-mode asset block.
    pub fn is_repo_backed(&self, namespace: &str) -> bool {
        matches!(self.configured.get(namespace), Some(Backend::Repo(_)))
    }

    /// Whether rheo knows how to fetch this namespace ahead of the asset scan.
    ///
    /// A namespace rheo cannot fetch must stay skipped rather than attempting a
    /// download, but a CONFIGURED one must not be skipped: pre-warming runs
    /// before asset detection, so a package missing from disk at that moment
    /// contributes no stylesheet and the build still succeeds.
    pub fn is_prewarmable(&self, namespace: &str) -> bool {
        self.is_configured(namespace) || matches!(namespace, "preview" | "rheo")
    }
}

fn downloader() -> SystemDownloader {
    SystemDownloader::new(USER_AGENT)
}

/// Downloads and caches packages served as release tarballs.
///
/// Packages are stored as `{name}-{version}.tar.gz` release assets under the tag
/// `{name}-{version}`. The host comes from the namespace's `releases` key, or
/// from the built-in rheo-packages base when a project configures nothing.
pub struct RheoPackages {
    source: ReleasesSource,
    cache: Option<FsPackages>,
    downloader: SystemDownloader,
}

impl RheoPackages {
    /// The built-in `@rheo` backend, serving from the rheo-packages releases.
    pub fn new(downloader: SystemDownloader) -> Self {
        Self::with_source(downloader, ReleasesSource::Base(REGISTRY_URL.to_string()))
    }

    /// A backend for a namespace configured with `releases = ...`.
    pub fn with_source(downloader: SystemDownloader, source: ReleasesSource) -> Self {
        Self {
            source,
            cache: dirs::cache_dir().map(|d| FsPackages::new(d.join("typst/packages"))),
            downloader,
        }
    }

    pub fn obtain(&self, spec: &PackageSpec) -> PackageResult<FsRoot> {
        // No invalidation, deliberately: a release at `<name>-<version>` is
        // immutable, so a cache hit can never be stale. The repository-ref path
        // is keyed by commit sha for the opposite reason — a branch moves — so
        // do not "fix" one of these to match the other.
        //
        // `FsPackages` keys its layout `{namespace}/{name}/{version}`, so two
        // namespaces backed by different hosts cannot collide here.
        if let Some(cache) = &self.cache
            && let Some(root) = cache.obtain(spec)
        {
            return Ok(root);
        }

        let url = self.source.url_for(&spec.name, &spec.version.to_string());

        let data = self
            .downloader
            .download(spec, &url)
            .map_err(|e: std::io::Error| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    // A 404 is nearly always an unreleased package or a typo in
                    // `releases`, and neither is guessable from the spec alone.
                    PackageError::Other(Some(eco_format!(
                        "@{namespace}/{name}:{version} was not found at {url} — the package may \
                         not be released yet, or [packages.{namespace}] `releases` may be wrong",
                        namespace = spec.namespace,
                        name = spec.name,
                        version = spec.version,
                    )))
                } else {
                    PackageError::NetworkFailed(Some(eco_format!(
                        "downloading @{namespace}/{name}:{version} from {url}: {e}",
                        namespace = spec.namespace,
                        name = spec.name,
                        version = spec.version,
                    )))
                }
            })?;

        let Some(cache) = &self.cache else {
            return Err(PackageError::Other(Some(eco_format!(
                "no cache directory available to store @{}/{} {}",
                spec.namespace,
                spec.name,
                spec.version,
            ))));
        };

        cache.store(spec, |tempdir| {
            let decompressed = GzDecoder::new(Cursor::new(data));
            let mut archive = tar::Archive::new(decompressed);
            archive
                .unpack(tempdir)
                .map_err(|e| PackageError::MalformedArchive(Some(eco_format!("{e}"))))
        })?;

        cache
            .obtain(spec)
            .ok_or_else(|| PackageError::NotFound(spec.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NamespaceSource, RheoConfig, RheoConfigRaw};

    fn parse_source(releases: &str) -> NamespaceSource {
        let toml = format!(
            "version = \"{}\"\n[packages.ns]\nreleases = \"{releases}\"",
            env!("CARGO_PKG_VERSION"),
        );
        let raw: RheoConfigRaw = toml::from_str(&toml).expect("raw parse failed");
        RheoConfig::try_from(raw)
            .expect("parse failed")
            .packages
            .remove("ns")
            .expect("namespace absent")
    }

    /// The built-in base is unchanged, so a project configuring nothing requests
    /// exactly the URL it always did.
    #[test]
    fn default_base_builds_the_rheo_packages_url() {
        let source = ReleasesSource::Base(REGISTRY_URL.to_string());
        assert_eq!(
            source.url_for("feeds", "0.1.1"),
            "https://github.com/freecomputinglab/rheo-packages/releases/download/feeds-0.1.1/feeds-0.1.1.tar.gz",
        );
    }

    /// `releases = "freecomputinglab/rheo-packages"` must be indistinguishable
    /// from configuring nothing at all.
    #[test]
    fn shorthand_matches_the_built_in_base() {
        let NamespaceSource::Releases(shorthand) = parse_source("freecomputinglab/rheo-packages")
        else {
            panic!("expected a releases source");
        };
        assert_eq!(
            shorthand.url_for("feeds", "0.1.1"),
            ReleasesSource::Base(REGISTRY_URL.to_string()).url_for("feeds", "0.1.1"),
        );
    }

    #[test]
    fn template_substitutes_every_placeholder() {
        let source = ReleasesSource::Template(
            "https://git.example.org/pkgs/{name}/-/releases/{version}/{name}.tar.gz".to_string(),
        );
        assert_eq!(
            source.url_for("core", "0.1.0"),
            "https://git.example.org/pkgs/core/-/releases/0.1.0/core.tar.gz",
        );
    }
}
