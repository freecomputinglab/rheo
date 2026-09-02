use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use ecow::eco_format;
use parking_lot::Mutex;
use tracing::info;
use typst_kit::files::FsRoot;
use typst_library::diag::{PackageError, PackageResult};
use typst_syntax::package::PackageSpec;

use super::{Announce, PackageSource, SourceKind, cache_root, package_dir};
use crate::config::{GitRef, RepoSource};

/// Serves a namespace from a repository, checked out at a resolved commit sha.
///
/// The cache is keyed by that sha rather than by the ref name, which is the
/// whole design: a new commit is a different key and so a natural miss, an
/// unchanged branch is a hit, and no ref's bytes can ever be served for
/// another's. Freshness is exact, so there is nothing to invalidate and no
/// staleness window to manage. The per-build cost is one `git ls-remote`, which
/// transfers no objects.
pub struct GitPackages {
    namespace: String,
    source: RepoSource,
    cache_root: Option<PathBuf>,
    /// The git executable to invoke. Injectable only so a test can exercise the
    /// binary-not-found path without mutating `PATH` for every other test in the
    /// process.
    binary: String,
    /// The resolved sha, memoised so `ls-remote` runs once per build rather than
    /// once per file read.
    sha: Mutex<Option<String>>,
    announced: Announce,
}

impl GitPackages {
    pub fn new(namespace: &str, source: RepoSource) -> Self {
        Self::with_cache_root(namespace, source, cache_root())
    }

    fn with_cache_root(namespace: &str, source: RepoSource, cache_root: Option<PathBuf>) -> Self {
        Self {
            namespace: namespace.to_string(),
            source,
            cache_root,
            binary: "git".to_string(),
            sha: Mutex::new(None),
            announced: Announce::new(),
        }
    }

    /// The directory for `@<ns>/<name>:<version>` inside the checkout.
    pub fn obtain(&self, spec: &PackageSpec) -> PackageResult<FsRoot> {
        let sha = self.resolve_sha()?;
        let checkout = self.checkout(&sha)?;

        package_dir(&checkout, &self.source.subdir, spec)
            .map(FsRoot::new)
            .map_err(|_| {
                PackageError::Other(Some(eco_format!(
                    "@{namespace}/{name}:{version} not found in {url} at {git_ref} ({sha})",
                    namespace = self.namespace,
                    name = spec.name,
                    version = spec.version,
                    url = self.source.url,
                    git_ref = self.source.git_ref,
                )))
            })
    }

    fn resolve_sha(&self) -> PackageResult<String> {
        let mut cached = self.sha.lock();
        if let Some(sha) = cached.as_ref() {
            return Ok(sha.clone());
        }

        let sha = match &self.source.git_ref {
            // A pinned commit is already the cache key; asking the server for it
            // would tell us nothing.
            GitRef::Rev(rev) => rev.clone(),
            git_ref => {
                let output = self.git(&["ls-remote", &self.source.url, git_ref.as_str()])?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let first = stdout.split_whitespace().next().unwrap_or_default();
                if first.is_empty() {
                    return Err(PackageError::Other(Some(eco_format!(
                        "[packages.{namespace}]: {kind} `{name}` does not exist in {url}",
                        namespace = self.namespace,
                        kind = git_ref.kind(),
                        name = git_ref.as_str(),
                        url = self.source.url,
                    ))));
                }
                first.to_string()
            }
        };

        self.announced.once(|| {
            info!(
                "@{} resolves from {} at {} ({})",
                self.namespace, self.source.url, self.source.git_ref, sha,
            );
        });
        *cached = Some(sha.clone());
        Ok(sha)
    }

    /// Every cached checkout of this namespace's repository, keyed by sha.
    fn slug_dir(&self) -> Option<PathBuf> {
        self.cache_root
            .as_ref()
            .map(|root| root.join("rheo/git").join(slug(&self.source.url)))
    }

    /// Delete every cached checkout of this namespace's repository, returning
    /// how many were removed.
    ///
    /// EXPLICIT ONLY — never called from the build path. A checkout is
    /// content-addressed and re-creatable, so losing one costs a re-clone, but a
    /// build reading a checkout while it is deleted fails. Pruning on the build
    /// path would have to guess which shas another build is holding open, and
    /// mtime cannot answer that: a directory's mtime is set when it is cloned
    /// and never updated by the reads that follow.
    pub fn prune(&self) -> std::io::Result<usize> {
        let Some(dir) = self.slug_dir() else {
            return Ok(0);
        };
        if !dir.exists() {
            return Ok(0);
        }
        let removed = std::fs::read_dir(&dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .count();
        std::fs::remove_dir_all(&dir)?;
        Ok(removed)
    }

    /// The checkout for `sha`, cloning it if this is the first build to want it.
    fn checkout(&self, sha: &str) -> PackageResult<PathBuf> {
        let Some(root) = &self.cache_root else {
            return Err(PackageError::Other(Some(eco_format!(
                "no cache directory available to check out @{} from {}",
                self.namespace,
                self.source.url,
            ))));
        };
        // The repo identity as well as the sha: two namespaces may point at one
        // repo, and two repos could in principle share a sha prefix.
        let dir = root.join("rheo/git").join(slug(&self.source.url)).join(sha);
        if dir.exists() {
            return Ok(dir);
        }

        let parent = dir.parent().expect("checkout dir always has a parent");
        std::fs::create_dir_all(parent).map_err(|e| {
            PackageError::Other(Some(eco_format!("creating {}: {e}", parent.display())))
        })?;

        // Clone into a temporary sibling and rename it into place, so an
        // interrupted clone cannot leave a half-tree that later reads as a hit.
        let tempdir = tempfile::tempdir_in(parent).map_err(|e| {
            PackageError::Other(Some(eco_format!(
                "creating a temporary directory in {}: {e}",
                parent.display(),
            )))
        })?;
        let staging = tempdir.path().join("checkout");
        self.clone_into(&staging, sha)?;

        match std::fs::rename(&staging, &dir) {
            Ok(()) => Ok(dir),
            // Another process finished the same clone first; its tree is the
            // same sha, so theirs is as good as ours.
            Err(_) if dir.exists() => Ok(dir),
            Err(e) => Err(PackageError::Other(Some(eco_format!(
                "moving the checkout into {}: {e}",
                dir.display(),
            )))),
        }
    }

    fn clone_into(&self, dest: &Path, sha: &str) -> PackageResult<()> {
        let dest = dest.to_string_lossy().to_string();
        match &self.source.git_ref {
            GitRef::Branch(name) | GitRef::Tag(name) => {
                self.git(&[
                    "clone",
                    "--depth",
                    "1",
                    "--single-branch",
                    "--branch",
                    name,
                    &self.source.url,
                    &dest,
                ])?;
            }
            // `clone --branch` does not accept a sha, so a pinned rev is fetched
            // explicitly. This requires the server to allow fetching an
            // arbitrary object by sha — GitHub does, some servers do not, which
            // is what the error below has to say.
            GitRef::Rev(rev) => {
                self.git(&["init", "--quiet", &dest])?;
                self.git(&["-C", &dest, "remote", "add", "origin", &self.source.url])?;
                self.git(&["-C", &dest, "fetch", "--depth", "1", "origin", rev])
                    .map_err(|e| {
                        PackageError::Other(Some(eco_format!(
                            "{e:?} — fetching the exact commit {sha} from {url} failed; not every \
                             server allows fetching a commit by sha",
                            url = self.source.url,
                        )))
                    })?;
                self.git(&["-C", &dest, "checkout", "--quiet", "FETCH_HEAD"])?;
            }
        }
        Ok(())
    }

    fn git(&self, args: &[&str]) -> PackageResult<std::process::Output> {
        let output = Command::new(&self.binary).args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PackageError::Other(Some(eco_format!(
                    "[packages.{}] is backed by a repository, which needs the `git` binary, and \
                     git was not found on PATH",
                    self.namespace,
                )))
            } else {
                PackageError::Other(Some(eco_format!("running git: {e}")))
            }
        })?;

        if !output.status.success() {
            return Err(PackageError::Other(Some(eco_format!(
                "[packages.{namespace}]: `git {args}` failed for {url}: {stderr}",
                namespace = self.namespace,
                args = args.join(" "),
                url = self.source.url,
                stderr = String::from_utf8_lossy(&output.stderr).trim(),
            ))));
        }
        Ok(output)
    }
}

impl PackageSource for GitPackages {
    fn obtain(&self, spec: &PackageSpec) -> PackageResult<FsRoot> {
        self.obtain(spec)
    }

    fn kind(&self) -> SourceKind {
        SourceKind::Repo
    }

    fn prune(&self) -> std::io::Result<usize> {
        self.prune()
    }
}

/// A filesystem-safe stand-in for a git URL. Hashed rather than sanitised: a URL
/// carries `:`, `/` and `@`, and any escaping scheme that stayed readable would
/// also have to stay injective.
///
/// `DefaultHasher` is deterministic across runs but not promised to be stable
/// across Rust versions; the cost of it changing is one extra clone, not a wrong
/// answer, since the sha below it still names the content.
fn slug(url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs git in `dir`, failing the test loudly — these set up the fixture
    /// repository the tests then fetch from.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=rheo", "-c", "user.email=rheo@example.org"])
            .args(args)
            .output()
            .expect("git should be on PATH");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// A repository holding `core/0.1.0/typst.toml` on `main`.
    fn fixture_repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        git_in(dir, &["init", "--quiet", "--initial-branch=main", "."]);
        let pkg = dir.join("core/0.1.0");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("typst.toml"), "[package]\nname = \"core\"\n").unwrap();
        git_in(dir, &["add", "-A"]);
        git_in(dir, &["commit", "--quiet", "-m", "first"]);
    }

    fn source(url: &Path, git_ref: GitRef) -> RepoSource {
        RepoSource {
            url: url.to_string_lossy().to_string(),
            git_ref,
            subdir: String::new(),
        }
    }

    fn spec(name: &str, version: &str) -> PackageSpec {
        format!("@rookery/{name}:{version}").parse().unwrap()
    }

    #[test]
    fn resolves_a_branch_and_caches_by_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let cache = tmp.path().join("cache");
        fixture_repo(&repo);

        let packages = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Branch("main".into())),
            Some(cache.clone()),
        );
        let root = packages.obtain(&spec("core", "0.1.0")).expect("obtain");
        assert!(root.path().join("typst.toml").exists());

        let sha = packages.sha.lock().clone().expect("sha memoised");
        let checkout = cache
            .join("rheo/git")
            .join(slug(&repo.to_string_lossy()))
            .join(&sha);
        assert!(checkout.is_dir(), "checkout should live under its sha");

        // A sentinel proves the second obtain is a cache hit: a re-clone would
        // replace the tree and take the file with it.
        let sentinel = checkout.join("SENTINEL");
        std::fs::write(&sentinel, "x").unwrap();
        packages
            .obtain(&spec("core", "0.1.0"))
            .expect("second obtain");
        assert!(
            sentinel.exists(),
            "second obtain re-cloned instead of hitting the cache"
        );
    }

    #[test]
    fn a_new_commit_is_a_new_cache_key() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let cache = tmp.path().join("cache");
        fixture_repo(&repo);

        let first = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Branch("main".into())),
            Some(cache.clone()),
        );
        first.obtain(&spec("core", "0.1.0")).expect("obtain");
        let first_sha = first.sha.lock().clone().unwrap();

        std::fs::write(repo.join("core/0.1.0/extra.typ"), "// added").unwrap();
        git_in(&repo, &["add", "-A"]);
        git_in(&repo, &["commit", "--quiet", "-m", "second"]);

        // A fresh instance stands in for the next build; the same one would
        // (correctly) still be holding its memoised sha.
        let second = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Branch("main".into())),
            Some(cache.clone()),
        );
        let root = second.obtain(&spec("core", "0.1.0")).expect("obtain");
        let second_sha = second.sha.lock().clone().unwrap();

        assert_ne!(
            first_sha, second_sha,
            "the branch moved, so the key must change"
        );
        assert!(
            root.path().join("extra.typ").exists(),
            "the new commit's content should be served without any cache clearing",
        );
    }

    /// Several commits of one branch leave one checkout each; a prune removes
    /// them all and leaves the slug directory gone rather than half-populated.
    #[test]
    fn prune_removes_every_cached_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let cache = tmp.path().join("cache");
        fixture_repo(&repo);

        // Two builds against two commits of the same branch.
        let mut shas = Vec::new();
        for i in 0..2 {
            if i > 0 {
                std::fs::write(repo.join("core/0.1.0/extra.typ"), "// added").unwrap();
                git_in(&repo, &["add", "-A"]);
                git_in(&repo, &["commit", "--quiet", "-m", "next"]);
            }
            let packages = GitPackages::with_cache_root(
                "rookery",
                source(&repo, GitRef::Branch("main".into())),
                Some(cache.clone()),
            );
            packages.obtain(&spec("core", "0.1.0")).expect("obtain");
            shas.push(packages.sha.lock().clone().unwrap());
        }
        assert_ne!(shas[0], shas[1], "the branch advanced, so the keys differ");

        let slug_dir = cache.join("rheo/git").join(slug(&repo.to_string_lossy()));
        assert_eq!(
            std::fs::read_dir(&slug_dir).unwrap().count(),
            2,
            "one checkout per commit built against",
        );

        let packages = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Branch("main".into())),
            Some(cache.clone()),
        );
        assert_eq!(packages.prune().expect("prune"), 2);
        assert!(!slug_dir.exists());

        // Pruning again is a no-op rather than an error, so a repeated `clean`
        // does not fail.
        assert_eq!(packages.prune().expect("prune"), 0);
    }

    /// Pruning one namespace's repository must not touch another's.
    #[test]
    fn prune_leaves_another_repository_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        let mine = tmp.path().join("mine");
        let theirs = tmp.path().join("theirs");
        fixture_repo(&mine);
        fixture_repo(&theirs);

        for repo in [&mine, &theirs] {
            let packages = GitPackages::with_cache_root(
                "ns",
                source(repo, GitRef::Branch("main".into())),
                Some(cache.clone()),
            );
            packages.obtain(&spec("core", "0.1.0")).expect("obtain");
        }

        let packages = GitPackages::with_cache_root(
            "ns",
            source(&mine, GitRef::Branch("main".into())),
            Some(cache.clone()),
        );
        packages.prune().expect("prune");

        let theirs_dir = cache.join("rheo/git").join(slug(&theirs.to_string_lossy()));
        assert!(
            theirs_dir.exists(),
            "another repository's cache was removed"
        );
    }

    #[test]
    fn a_tag_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fixture_repo(&repo);
        git_in(&repo, &["tag", "core-0.1.0"]);

        let packages = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Tag("core-0.1.0".into())),
            Some(tmp.path().join("cache")),
        );
        assert!(packages.obtain(&spec("core", "0.1.0")).is_ok());
    }

    #[test]
    fn subdir_is_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_in(&repo, &["init", "--quiet", "--initial-branch=main", "."]);
        let pkg = repo.join("packages/core/0.1.0");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("typst.toml"), "[package]\n").unwrap();
        git_in(&repo, &["add", "-A"]);
        git_in(&repo, &["commit", "--quiet", "-m", "first"]);

        let mut src = source(&repo, GitRef::Branch("main".into()));
        src.subdir = "packages".to_string();
        let packages = GitPackages::with_cache_root("rookery", src, Some(tmp.path().join("cache")));
        assert!(packages.obtain(&spec("core", "0.1.0")).is_ok());
    }

    #[test]
    fn a_missing_branch_names_the_branch_and_the_url() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fixture_repo(&repo);

        let packages = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Branch("no-such-branch".into())),
            Some(tmp.path().join("cache")),
        );
        let err = format!("{:?}", packages.obtain(&spec("core", "0.1.0")).unwrap_err());
        assert!(err.contains("no-such-branch"), "{err}");
        assert!(err.contains(&repo.to_string_lossy().to_string()), "{err}");
    }

    #[test]
    fn a_missing_package_names_the_ref_and_the_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fixture_repo(&repo);

        let packages = GitPackages::with_cache_root(
            "rookery",
            source(&repo, GitRef::Branch("main".into())),
            Some(tmp.path().join("cache")),
        );
        let err = format!(
            "{:?}",
            packages.obtain(&spec("absent", "9.9.9")).unwrap_err()
        );
        assert!(err.contains("@rookery/absent:9.9.9"), "{err}");
        assert!(err.contains("branch main"), "{err}");
    }

    /// With no git binary the message has to name git and the namespace, rather
    /// than surfacing a bare `NotFound`.
    #[test]
    fn a_missing_git_binary_names_git_and_the_namespace() {
        let tmp = tempfile::tempdir().unwrap();
        let mut packages = GitPackages::with_cache_root(
            "rookery",
            source(Path::new("/nonexistent"), GitRef::Branch("main".into())),
            Some(tmp.path().to_path_buf()),
        );
        packages.binary = "rheo-no-such-git-binary".to_string();

        let err = format!("{:?}", packages.obtain(&spec("core", "0.1.0")).unwrap_err());
        assert!(err.contains("git"), "{err}");
        assert!(err.contains("rookery"), "{err}");
    }
}
