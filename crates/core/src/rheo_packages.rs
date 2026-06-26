use std::io::Cursor;

use ecow::eco_format;
use flate2::read::GzDecoder;
use typst_kit::downloader::{Downloader, SystemDownloader};
use typst_kit::files::FsRoot;
use typst_kit::packages::FsPackages;
use typst_library::diag::{PackageError, PackageResult};
use typst_syntax::package::PackageSpec;

const REGISTRY_URL: &str =
    "https://github.com/freecomputinglab/rheo-packages/releases/download";

/// Downloads and caches packages from the @rheo namespace via GitHub Releases.
///
/// Packages are stored as `{name}-{version}.tar.gz` release assets under the
/// tag `{name}-{version}` in the rheo-packages repository.
pub struct RheoPackages {
    cache: Option<FsPackages>,
    downloader: SystemDownloader,
}

impl RheoPackages {
    pub fn new(downloader: SystemDownloader) -> Self {
        Self {
            cache: dirs::cache_dir()
                .map(|d| FsPackages::new(d.join("typst/packages"))),
            downloader,
        }
    }

    pub fn obtain(&self, spec: &PackageSpec) -> PackageResult<FsRoot> {
        if let Some(cache) = &self.cache
            && let Some(root) = cache.obtain(spec)
        {
            return Ok(root);
        }

        let url = format!(
            "{}/{name}-{version}/{name}-{version}.tar.gz",
            REGISTRY_URL,
            name = spec.name,
            version = spec.version,
        );

        let data = self
            .downloader
            .download(spec, &url)
            .map_err(|e: std::io::Error| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    PackageError::NotFound(spec.clone())
                } else {
                    PackageError::NetworkFailed(Some(eco_format!("{e}")))
                }
            })?;

        let Some(cache) = &self.cache else {
            return Err(PackageError::Other(Some(eco_format!(
                "no cache directory available to store @rheo/{} {}",
                spec.name,
                spec.version,
            ))));
        };

        cache.store(spec, |tempdir| {
            let decompressed = GzDecoder::new(Cursor::new(data));
            let mut archive = tar::Archive::new(decompressed);
            archive.unpack(tempdir).map_err(|e| {
                PackageError::MalformedArchive(Some(eco_format!("{e}")))
            })
        })?;

        cache
            .obtain(spec)
            .ok_or_else(|| PackageError::NotFound(spec.clone()))
    }
}
