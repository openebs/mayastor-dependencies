use regex::Regex;

#[cfg(feature = "deps")]
mod default_version {
    /// Returns the git version as &'static str, in the long format:
    /// either tag, number of additional commits and the abbreviated commit name,
    /// or just commit hash in the case no tags found.
    /// See `git describe` manual for details.
    pub fn long_raw_version_str() -> &'static str {
        // to keep clippy happy
        #[allow(dead_code)]
        #[allow(clippy::option_env_unwrap)]
        fn fallback() -> &'static str {
            option_env!("GIT_VERSION_LONG").expect("git version fallback")
        }

        #[cfg(feature = "deps-logs-head")]
        #[cfg(not(feature = "deps-index"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always", "--long"],
            fallback = fallback(),
            git_deps = ["logs/HEAD"]
        );

        #[cfg(not(feature = "deps-logs-head"))]
        #[cfg(feature = "deps-index")]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always", "--long"],
            fallback = fallback(),
            git_deps = ["index"]
        );

        #[cfg(all(feature = "deps-index", feature = "deps-logs-head"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always", "--long"],
            fallback = fallback(),
            git_deps = ["logs/HEAD", "index"]
        );

        #[cfg(not(feature = "deps-logs-head"))]
        #[cfg(not(feature = "deps-index"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always", "--long"],
            fallback = fallback(),
            git_deps = []
        );

        version
    }

    /// Returns the git version as &'static str.
    /// See `git describe` manual for details.
    pub fn raw_version_str() -> &'static str {
        // to keep clippy happy
        #[allow(dead_code)]
        #[allow(clippy::option_env_unwrap)]
        fn fallback() -> &'static str {
            option_env!("GIT_VERSION").expect("git version fallback")
        }

        #[cfg(feature = "deps-logs-head")]
        #[cfg(not(feature = "deps-index"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always"],
            fallback = fallback(),
            git_deps = ["logs/HEAD"]
        );

        #[cfg(not(feature = "deps-logs-head"))]
        #[cfg(feature = "deps-index")]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always"],
            fallback = fallback(),
            git_deps = ["index"]
        );

        #[cfg(all(feature = "deps-index", feature = "deps-logs-head"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always"],
            fallback = fallback(),
            git_deps = ["logs/HEAD", "index"]
        );

        #[cfg(not(feature = "deps-logs-head"))]
        #[cfg(not(feature = "deps-index"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always"],
            fallback = fallback(),
            git_deps = []
        );

        version
    }

    /// Returns the git version as String.
    pub fn raw_version_string() -> String {
        String::from(raw_version_str())
    }
}

#[cfg(feature = "deps")]
pub use default_version::{long_raw_version_str, raw_version_str, raw_version_string};

/// Git version data.
pub(super) struct GitVersion {
    /// Abbreviated commit hash.
    pub(super) commit_hash: String,

    /// Tag name.
    pub(super) tag: Option<String>,

    /// Number of additional commits on top of the tag.
    pub(super) num_commits: Option<String>,
}

impl GitVersion {
    /// Parses a version string returned by git_version!.
    pub(super) fn parse_git_ver(git_ver: &str) -> Self {
        let re = Regex::new(r"^(.+)-(.+)-g([0-9a-f]{12})$").unwrap();

        if let Some(cap) = re.captures(git_ver) {
            Self {
                commit_hash: cap.get(3).unwrap().as_str().to_string(),
                tag: Some(cap.get(1).unwrap().as_str().to_string()),
                num_commits: Some(cap.get(2).unwrap().as_str().to_string()),
            }
        } else {
            Self {
                commit_hash: git_ver.to_string(),
                tag: None,
                num_commits: None,
            }
        }
    }
}
