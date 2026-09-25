//! `[packages.<namespace>]` — where a Typst package namespace resolves from.
//!
//! Without this table `@rheo` resolves from its built-in releases host and every
//! other namespace goes to Typst universe. A table entry replaces that for one
//! namespace: a repository checked out at a ref, a different releases host, or
//! a directory on disk. An entry may further name the `packages` it applies to;
//! every other package in that namespace keeps resolving as if the table were
//! absent.

use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use tracing::warn;

/// The GitHub download base a bare `<owner>/<repo>` shorthand expands to.
const GITHUB_RELEASES: &str = "https://github.com";

/// Which ref a repository-backed namespace is checked out at.
///
/// Ordered by the precedence a config states them in: an explicit commit beats a
/// tag, which beats a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRef {
    /// An exact commit sha, pinning absolutely.
    Rev(String),
    /// A tag — immutable in practice, which is what a stable consumer wants.
    Tag(String),
    /// A branch head, re-resolved on every build.
    Branch(String),
}

impl Default for GitRef {
    fn default() -> Self {
        GitRef::Branch("main".to_string())
    }
}

impl GitRef {
    /// The ref itself, as git would be given it.
    pub fn as_str(&self) -> &str {
        match self {
            GitRef::Rev(s) | GitRef::Tag(s) | GitRef::Branch(s) => s,
        }
    }

    /// The config key this ref came from, for error and log messages.
    pub fn kind(&self) -> &'static str {
        match self {
            GitRef::Rev(_) => "rev",
            GitRef::Tag(_) => "tag",
            GitRef::Branch(_) => "branch",
        }
    }
}

impl Display for GitRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} {}", self.kind(), self.as_str())
    }
}

/// A namespace served from a repository checked out at a ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSource {
    /// Any URL the `git` binary accepts: https, ssh, or a local path.
    pub url: String,
    pub git_ref: GitRef,
    /// Path prefix inside the repository — a package `@<ns>/<name>:<version>`
    /// lives at `<subdir>/<name>/<version>/`. Empty when packages sit at the root.
    pub subdir: String,
}

/// A namespace served from release tarballs named `<name>-<version>.tar.gz`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleasesSource {
    /// A host root, from the `<owner>/<repo>` shorthand or a scheme-carrying URL
    /// with no placeholders. GitHub's asset path is appended to it.
    Base(String),
    /// A URL template carrying `{name}` and `{version}`, substituted verbatim —
    /// the form that keeps a non-GitHub forge usable.
    Template(String),
}

impl ReleasesSource {
    /// The tarball URL for one package.
    ///
    /// A `Base` gets GitHub's asset path appended: the tag and the asset share
    /// the `<name>-<version>` name, which is what
    /// `.github/workflows/publish-packages.yml` produces. A `Template` is
    /// substituted verbatim, since another forge shapes its URLs differently.
    pub fn url_for(&self, name: &str, version: &str) -> String {
        match self {
            ReleasesSource::Base(base) => {
                format!("{base}/{name}-{version}/{name}-{version}.tar.gz")
            }
            ReleasesSource::Template(template) => template
                .replace("{name}", name)
                .replace("{version}", version),
        }
    }

    /// The value identifying this source, for keying its cache directory.
    pub fn source_key(&self) -> &str {
        match self {
            ReleasesSource::Base(base) => base,
            ReleasesSource::Template(template) => template,
        }
    }
}

/// A namespace served from a directory on disk — a package's own working
/// tree, read in place. No ref, because there is nothing to check out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSource {
    /// The directory holding `<name>/<version>/` package trees.
    pub root: PathBuf,
    /// Path prefix inside it, the same meaning `RepoSource::subdir` has.
    pub subdir: String,
}

impl PathSource {
    /// Anchor a relative root against `base_dir` — the config file's own
    /// directory, so `path = "../pkgs"` means the same thing however rheo was
    /// invoked. An absolute root is already anchored.
    pub fn anchor_to(&mut self, base_dir: &Path) {
        if self.root.is_relative() {
            self.root = base_dir.join(&self.root);
        }
    }
}

/// Where one namespace resolves from. Exactly one variant per namespace: moving
/// a project between a release and a branch is an explicit edit, not a
/// precedence rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceSource {
    Repo(RepoSource),
    Releases(ReleasesSource),
    Path(PathSource),
}

/// A `[packages.<ns>]` table: its source, and optionally which packages in the
/// namespace it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceEntry {
    pub source: NamespaceSource,
    /// `None` applies the source to every package in the namespace — the
    /// default, and the whole behaviour before this field existed. `Some`
    /// limits it to these names; every other package in the namespace resolves
    /// as if the table were absent.
    pub(crate) only: Option<Vec<String>>,
}

impl NamespaceEntry {
    /// Build an entry directly, bypassing TOML parsing — for a test that hand
    /// constructs a `[packages.<ns>]` table's parsed shape.
    pub fn new(source: NamespaceSource, only: Option<Vec<String>>) -> Self {
        Self { source, only }
    }

    /// Whether `name` resolves through this entry's source rather than falling
    /// through to the namespace's default.
    pub fn applies_to(&self, name: &str) -> bool {
        match &self.only {
            None => true,
            Some(names) => names.iter().any(|n| n == name),
        }
    }

    /// The `packages` key this entry was given, if any — for a caller that
    /// needs to carry the list itself rather than ask `applies_to` per name.
    pub fn only(&self) -> Option<&Vec<String>> {
        self.only.as_ref()
    }

    /// Anchor whatever part of this entry's source is a relative path against
    /// the config file's own directory. Only a [`PathSource`] has one.
    pub fn anchor_to(&mut self, base_dir: &Path) {
        self.source.anchor_to(base_dir);
    }
}

/// The `[packages.<ns>]` keys as written, before the one-of and ref-precedence
/// rules are applied.
#[derive(Debug, Deserialize)]
struct NamespaceSourceRaw {
    repo: Option<String>,
    releases: Option<String>,
    path: Option<String>,
    branch: Option<String>,
    tag: Option<String>,
    rev: Option<String>,
    subdir: Option<String>,
    packages: Option<Vec<String>>,
}

fn reject<T>(namespace: &str, message: impl Display) -> Result<T, toml::de::Error> {
    Err(serde::de::Error::custom(format!(
        "[packages.{namespace}]: {message}"
    )))
}

impl NamespaceSource {
    /// Anchor whatever part of this source is a relative path against the
    /// config file's own directory. Only a [`PathSource`] has one.
    pub fn anchor_to(&mut self, base_dir: &Path) {
        if let NamespaceSource::Path(path) = self {
            path.anchor_to(base_dir);
        }
    }

    /// Parse the whole `[packages]` table, validating each namespace.
    pub(super) fn parse_table(
        value: toml::Value,
    ) -> Result<HashMap<String, NamespaceEntry>, toml::de::Error> {
        let raw: HashMap<String, NamespaceSourceRaw> = value.try_into()?;
        raw.into_iter()
            .map(|(namespace, entry)| {
                Self::from_raw(&namespace, entry).map(|source| (namespace, source))
            })
            .collect()
    }

    /// Validate the `packages` key: the list of package names this table's
    /// source is limited to, or `None` when the key is absent.
    fn only(
        namespace: &str,
        packages: Option<Vec<String>>,
    ) -> Result<Option<Vec<String>>, toml::de::Error> {
        let Some(packages) = packages else {
            return Ok(None);
        };
        if packages.is_empty() {
            return reject(
                namespace,
                "`packages` lists which packages this table applies to; an empty list applies \
                 it to none — remove the table instead",
            );
        }
        let mut seen = std::collections::HashSet::new();
        for name in &packages {
            if !typst_syntax::is_ident(name) {
                return reject(
                    namespace,
                    format!(
                        "`packages` names `{name}`, which is not a valid package name: letters, \
                         digits, `_` and `-`, not starting with a digit"
                    ),
                );
            }
            if !seen.insert(name.as_str()) {
                return reject(
                    namespace,
                    format!("`packages` names `{name}` more than once"),
                );
            }
        }
        Ok(Some(packages))
    }

    fn from_raw(
        namespace: &str,
        raw: NamespaceSourceRaw,
    ) -> Result<NamespaceEntry, toml::de::Error> {
        // The key has to survive `parse_namespace` in an import spec, so an
        // invalid one must fail here rather than much later as an unresolvable
        // import that never names the config as the cause.
        if !typst_syntax::is_ident(namespace) {
            return reject(
                namespace,
                format!(
                    "`{namespace}` is not a valid package namespace. It appears in every import \
                     spec as `@{namespace}/name:1.0.0`, so it must be a Typst identifier: \
                     letters, digits, `_` and `-`, not starting with a digit"
                ),
            );
        }

        let only = Self::only(namespace, raw.packages)?;

        let ref_keys = [
            ("branch", raw.branch.as_ref()),
            ("tag", raw.tag.as_ref()),
            ("rev", raw.rev.as_ref()),
            ("subdir", raw.subdir.as_ref()),
        ];

        let source = match (raw.repo, raw.releases, raw.path) {
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => reject(
                namespace,
                "set exactly one of `repo` (a repository at a ref), `releases` (a releases host) \
                 or `path` (a directory on disk). Switching a project between a release, a \
                 branch and a local directory is an explicit edit, not a precedence rule",
            ),
            (None, None, None) => reject(
                namespace,
                "set one of `repo` (a repository at a ref), `releases` (a releases host) or \
                 `path` (a directory on disk)",
            ),
            (None, Some(releases), None) => {
                if let Some((key, _)) = ref_keys.iter().find(|(_, value)| value.is_some()) {
                    return reject(
                        namespace,
                        format!(
                            "`{key}` selects a ref inside a repository and is meaningless \
                                 alongside `releases`; use `repo` instead"
                        ),
                    );
                }
                Ok(NamespaceSource::Releases(Self::releases(
                    namespace, releases,
                )?))
            }
            (Some(url), None, None) => Ok(NamespaceSource::Repo(RepoSource {
                url,
                git_ref: Self::git_ref(namespace, raw.branch, raw.tag, raw.rev),
                subdir: Self::subdir(namespace, raw.subdir)?,
            })),
            (None, None, Some(path)) => {
                let path_ref_keys = [
                    ("branch", raw.branch.as_ref()),
                    ("tag", raw.tag.as_ref()),
                    ("rev", raw.rev.as_ref()),
                ];
                if let Some((key, _)) = path_ref_keys.iter().find(|(_, value)| value.is_some()) {
                    return reject(
                        namespace,
                        format!(
                            "`{key}` selects a ref inside a repository, and `path` reads a \
                             directory in place, so there is no ref to select. Use \
                             `repo = \"<path>\"` with `{key}` if you want a local repository AT \
                             a ref."
                        ),
                    );
                }
                Ok(NamespaceSource::Path(PathSource {
                    root: PathBuf::from(path),
                    subdir: Self::subdir(namespace, raw.subdir)?,
                }))
            }
        }?;

        Ok(NamespaceEntry { source, only })
    }

    fn releases(namespace: &str, value: String) -> Result<ReleasesSource, toml::de::Error> {
        // No scheme means the `<owner>/<repo>` shorthand; that is the whole
        // detection rule, so a value like `foo/bar` can never be read as a URL.
        if !value.contains("://") {
            let Some((owner, repo)) = value.split_once('/') else {
                return reject(
                    namespace,
                    format!(
                        "`releases = \"{value}\"` is neither an `<owner>/<repo>` shorthand nor a \
                         URL. Write `owner/repo`, or a full URL template containing `{{name}}` \
                         and `{{version}}`"
                    ),
                );
            };
            if owner.is_empty() || repo.is_empty() || repo.contains('/') {
                return reject(
                    namespace,
                    format!("`releases = \"{value}\"` is not an `<owner>/<repo>` shorthand"),
                );
            }
            return Ok(ReleasesSource::Base(format!(
                "{GITHUB_RELEASES}/{owner}/{repo}/releases/download"
            )));
        }

        // A URL is only usable if it says where the name and version go; a bare
        // host would silently download the same asset for every package.
        let missing: Vec<&str> = ["{name}", "{version}"]
            .into_iter()
            .filter(|p| !value.contains(p))
            .collect();
        if !missing.is_empty() {
            return reject(
                namespace,
                format!(
                    "`releases = \"{value}\"` is a URL template but is missing {}. A template must \
                     carry both `{{name}}` and `{{version}}` so each package resolves to its own \
                     asset",
                    missing.join(" and "),
                ),
            );
        }
        Ok(ReleasesSource::Template(value))
    }

    /// Pick the ref by precedence, warning rather than silently discarding the
    /// keys that lost.
    fn git_ref(
        namespace: &str,
        branch: Option<String>,
        tag: Option<String>,
        rev: Option<String>,
    ) -> GitRef {
        let given: Vec<&str> = [
            rev.as_ref().map(|_| "rev"),
            tag.as_ref().map(|_| "tag"),
            branch.as_ref().map(|_| "branch"),
        ]
        .into_iter()
        .flatten()
        .collect();

        let chosen = rev
            .map(GitRef::Rev)
            .or_else(|| tag.map(GitRef::Tag))
            .or_else(|| branch.map(GitRef::Branch))
            .unwrap_or_default();

        if given.len() > 1 {
            warn!(
                "[packages.{namespace}] sets {}; only `{}` is used (rev, then tag, then branch)",
                given.join(", "),
                chosen.kind(),
            );
        }
        chosen
    }

    fn subdir(namespace: &str, subdir: Option<String>) -> Result<String, toml::de::Error> {
        let Some(subdir) = subdir else {
            return Ok(String::new());
        };
        let path = std::path::Path::new(&subdir);
        // A config-supplied prefix is joined onto the checkout root, so an
        // absolute path or a `..` component is an arbitrary-file read.
        if path.is_absolute() || subdir.starts_with('/') {
            return reject(
                namespace,
                format!("`subdir = \"{subdir}\"` must be relative to the repository root"),
            );
        }
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return reject(
                namespace,
                format!(
                    "`subdir = \"{subdir}\"` may not contain `..`: it names a path inside the \
                         repository, not one outside it"
                ),
            );
        }
        Ok(subdir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RheoConfig, RheoConfigRaw};

    fn parse(rest: &str) -> Result<RheoConfig, toml::de::Error> {
        let toml = format!("version = \"{}\"\n{rest}", env!("CARGO_PKG_VERSION"));
        let raw: RheoConfigRaw = toml::from_str(&toml).expect("raw parse failed");
        RheoConfig::try_from(raw)
    }

    fn entry(rest: &str, namespace: &str) -> NamespaceEntry {
        parse(rest)
            .expect("parse failed")
            .packages
            .get(namespace)
            .expect("namespace absent")
            .clone()
    }

    fn source(rest: &str, namespace: &str) -> NamespaceSource {
        entry(rest, namespace).source
    }

    fn error(rest: &str) -> String {
        parse(rest)
            .expect_err("expected a config error")
            .to_string()
    }

    /// The two-variant example from the docs, both namespaces at once.
    #[test]
    fn both_variants_parse() {
        let toml = r#"
        [packages.rookery]
        releases = "freecomputinglab/rookery"

        [packages.rheo]
        repo = "https://github.com/freecomputinglab/rheo-packages"
        branch = "feat-x"
        "#;
        assert_eq!(
            source(toml, "rookery"),
            NamespaceSource::Releases(ReleasesSource::Base(
                "https://github.com/freecomputinglab/rookery/releases/download".to_string()
            )),
        );
        assert_eq!(
            source(toml, "rheo"),
            NamespaceSource::Repo(RepoSource {
                url: "https://github.com/freecomputinglab/rheo-packages".to_string(),
                git_ref: GitRef::Branch("feat-x".to_string()),
                subdir: String::new(),
            }),
        );
    }

    /// No `[packages]` table is the load-bearing case: four live sites depend on
    /// it behaving exactly as before.
    #[test]
    fn absent_table_yields_no_sources() {
        assert!(parse("").expect("parse failed").packages.is_empty());
        assert!(RheoConfig::default().packages.is_empty());
    }

    /// `[packages]` must be pulled out of `extra` before the plugin-section loop,
    /// or it becomes a phantom plugin section that nothing reads.
    #[test]
    fn packages_is_not_a_plugin_section() {
        let config = parse("[packages.rheo]\nreleases = \"a/b\"").expect("parse failed");
        assert!(!config.plugin_sections.contains_key("packages"));
    }

    #[test]
    fn branch_defaults_to_main_and_subdir_to_empty() {
        let NamespaceSource::Repo(repo) =
            source("[packages.ns]\nrepo = \"git@host:o/r.git\"", "ns")
        else {
            panic!("expected a repo source");
        };
        assert_eq!(repo.git_ref, GitRef::Branch("main".to_string()));
        assert_eq!(repo.subdir, "");
    }

    #[test]
    fn ref_precedence_selects_rev() {
        let NamespaceSource::Repo(repo) = source(
            "[packages.ns]\nrepo = \"u\"\nrev = \"abc123\"\ntag = \"v1\"\nbranch = \"b\"",
            "ns",
        ) else {
            panic!("expected a repo source");
        };
        assert_eq!(repo.git_ref, GitRef::Rev("abc123".to_string()));
    }

    #[test]
    fn tag_beats_branch() {
        let NamespaceSource::Repo(repo) = source(
            "[packages.ns]\nrepo = \"u\"\ntag = \"core-0.1.0\"\nbranch = \"b\"",
            "ns",
        ) else {
            panic!("expected a repo source");
        };
        assert_eq!(repo.git_ref, GitRef::Tag("core-0.1.0".to_string()));
    }

    #[test]
    fn subdir_is_kept() {
        let NamespaceSource::Repo(repo) =
            source("[packages.ns]\nrepo = \"u\"\nsubdir = \"packages\"", "ns")
        else {
            panic!("expected a repo source");
        };
        assert_eq!(repo.subdir, "packages");
    }

    #[test]
    fn releases_template_is_kept_verbatim() {
        assert_eq!(
            source(
                "[packages.ns]\nreleases = \"https://x/{name}/{version}/pkg.tar.gz\"",
                "ns",
            ),
            NamespaceSource::Releases(ReleasesSource::Template(
                "https://x/{name}/{version}/pkg.tar.gz".to_string()
            )),
        );
    }

    #[test]
    fn invalid_namespace_key_is_rejected() {
        for key in ["bad/ns", "my pkgs", "9lives"] {
            let msg = error(&format!("[packages.\"{key}\"]\nreleases = \"a/b\""));
            assert!(
                msg.contains(key) && msg.contains("namespace"),
                "error should name the namespace, got: {msg}",
            );
        }
    }

    #[test]
    fn repo_and_releases_together_are_rejected() {
        let msg = error("[packages.ns]\nrepo = \"u\"\nreleases = \"a/b\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("exactly one"),
            "{msg}"
        );
    }

    #[test]
    fn neither_repo_nor_releases_is_rejected() {
        let msg = error("[packages.ns]\nbranch = \"b\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("set one of"),
            "{msg}"
        );
    }

    #[test]
    fn ref_keys_alongside_releases_are_rejected() {
        for key in [
            "branch = \"b\"",
            "tag = \"t\"",
            "rev = \"r\"",
            "subdir = \"s\"",
        ] {
            let msg = error(&format!("[packages.ns]\nreleases = \"a/b\"\n{key}"));
            assert!(
                msg.contains("[packages.ns]") && msg.contains("`releases`"),
                "{msg}"
            );
        }
    }

    #[test]
    fn escaping_subdir_is_rejected() {
        let msg = error("[packages.ns]\nrepo = \"u\"\nsubdir = \"../etc\"");
        assert!(msg.contains("[packages.ns]") && msg.contains(".."), "{msg}");

        let msg = error("[packages.ns]\nrepo = \"u\"\nsubdir = \"/etc\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("relative"),
            "{msg}"
        );
    }

    #[test]
    fn path_alone_parses() {
        let NamespaceSource::Path(path) = source("[packages.ns]\npath = \"../pkgs\"", "ns") else {
            panic!("expected a path source");
        };
        assert_eq!(path.root, std::path::PathBuf::from("../pkgs"));
        assert_eq!(path.subdir, "");
    }

    #[test]
    fn path_and_repo_together_are_rejected() {
        let msg = error("[packages.ns]\npath = \"../pkgs\"\nrepo = \"u\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("exactly one"),
            "{msg}"
        );
    }

    #[test]
    fn path_and_branch_together_are_rejected() {
        let msg = error("[packages.ns]\npath = \"../pkgs\"\nbranch = \"b\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("branch") && msg.contains("repo"),
            "{msg}"
        );
    }

    #[test]
    fn path_keeps_subdir() {
        let NamespaceSource::Path(path) = source(
            "[packages.ns]\npath = \"../pkgs\"\nsubdir = \"packages\"",
            "ns",
        ) else {
            panic!("expected a path source");
        };
        assert_eq!(path.subdir, "packages");
    }

    #[test]
    fn releases_template_missing_a_placeholder_is_rejected() {
        let msg = error("[packages.ns]\nreleases = \"https://x/{name}.tar.gz\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("{version}"),
            "error should name the missing placeholder, got: {msg}",
        );
    }

    /// The worked example from the docs: a `packages` key limits the table's
    /// source to the named packages, and every other package in the namespace
    /// is untouched.
    #[test]
    fn packages_key_limits_which_names_apply() {
        let e = entry(
            "[packages.rheo]\nrepo = \"u\"\npackages = [\"contents-panel\"]",
            "rheo",
        );
        assert!(e.applies_to("contents-panel"));
        assert!(!e.applies_to("justify"));
    }

    #[test]
    fn no_packages_key_applies_to_every_name() {
        let e = entry("[packages.ns]\nrepo = \"u\"", "ns");
        assert!(e.applies_to("anything"));
        assert!(e.applies_to("something-else"));
    }

    #[test]
    fn empty_packages_list_is_rejected() {
        let msg = error("[packages.ns]\nrepo = \"u\"\npackages = []");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("`packages`"),
            "{msg}"
        );
    }

    #[test]
    fn duplicate_package_name_is_rejected() {
        let msg = error("[packages.ns]\nrepo = \"u\"\npackages = [\"a\", \"a\"]");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("more than once"),
            "{msg}"
        );
    }

    #[test]
    fn invalid_package_name_is_rejected() {
        let msg = error("[packages.ns]\nrepo = \"u\"\npackages = [\"bad name\"]");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("not a valid package name"),
            "{msg}"
        );
    }
}
