//! `[packages.<namespace>]` — where a Typst package namespace resolves from.
//!
//! Without this table `@rheo` resolves from its built-in releases host and every
//! other namespace goes to Typst universe. A table entry replaces that for one
//! namespace, either with a repository checked out at a ref or with a different
//! releases host.

use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Display;
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

/// Where one namespace resolves from. Exactly one variant per namespace: moving
/// a project between a release and a branch is an explicit edit, not a
/// precedence rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceSource {
    Repo(RepoSource),
    Releases(ReleasesSource),
}

/// The `[packages.<ns>]` keys as written, before the one-of and ref-precedence
/// rules are applied.
#[derive(Debug, Deserialize)]
struct NamespaceSourceRaw {
    repo: Option<String>,
    releases: Option<String>,
    branch: Option<String>,
    tag: Option<String>,
    rev: Option<String>,
    subdir: Option<String>,
}

fn reject<T>(namespace: &str, message: impl Display) -> Result<T, toml::de::Error> {
    Err(serde::de::Error::custom(format!(
        "[packages.{namespace}]: {message}"
    )))
}

impl NamespaceSource {
    /// Parse the whole `[packages]` table, validating each namespace.
    pub(super) fn parse_table(
        value: toml::Value,
    ) -> Result<HashMap<String, NamespaceSource>, toml::de::Error> {
        let raw: HashMap<String, NamespaceSourceRaw> = value.try_into()?;
        raw.into_iter()
            .map(|(namespace, entry)| {
                Self::from_raw(&namespace, entry).map(|source| (namespace, source))
            })
            .collect()
    }

    fn from_raw(
        namespace: &str,
        raw: NamespaceSourceRaw,
    ) -> Result<NamespaceSource, toml::de::Error> {
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

        let ref_keys = [
            ("branch", raw.branch.as_ref()),
            ("tag", raw.tag.as_ref()),
            ("rev", raw.rev.as_ref()),
            ("subdir", raw.subdir.as_ref()),
        ];

        match (raw.repo, raw.releases) {
            (Some(_), Some(_)) => reject(
                namespace,
                "set `repo` or `releases`, not both. Switching a project between a release and a \
                 branch is an explicit edit, not a precedence rule",
            ),
            (None, None) => reject(
                namespace,
                "set one of `repo` (a repository at a ref) or `releases` (a releases host)",
            ),
            (None, Some(releases)) => {
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
            (Some(url), None) => Ok(NamespaceSource::Repo(RepoSource {
                url,
                git_ref: Self::git_ref(namespace, raw.branch, raw.tag, raw.rev),
                subdir: Self::subdir(namespace, raw.subdir)?,
            })),
        }
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

    fn source(rest: &str, namespace: &str) -> NamespaceSource {
        parse(rest)
            .expect("parse failed")
            .packages
            .get(namespace)
            .expect("namespace absent")
            .clone()
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
            msg.contains("[packages.ns]") && msg.contains("not both"),
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
    fn releases_template_missing_a_placeholder_is_rejected() {
        let msg = error("[packages.ns]\nreleases = \"https://x/{name}.tar.gz\"");
        assert!(
            msg.contains("[packages.ns]") && msg.contains("{version}"),
            "error should name the missing placeholder, got: {msg}",
        );
    }
}
