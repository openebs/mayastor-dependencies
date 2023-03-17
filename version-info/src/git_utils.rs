use regex::Regex;

#[cfg(feature = "default-git-versions")]
mod default_version {
    /// Returns the git version as &'static str, in the long format:
    /// either tag, number of additional commits and the abbreviated commit name,
    /// or just commit hash in the case no tags found.
    /// See `git describe` manual for details.
    pub fn long_raw_version_str() -> &'static str {
        // to keep clippy happy
        #[allow(dead_code)]
        fn fallback() -> &'static str {
            option_env!("GIT_VERSION_LONG").expect("git version fallback")
        }

        #[cfg(not(any(feature = "git-version-stale", feature = "git-version-fallback")))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always", "--long"],
            fallback = fallback()
        );

        #[cfg(feature = "git-version-stale")]
        #[cfg(not(feature = "git-version-fallback"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always", "--long"],
            fallback = fallback(),
            git_deps = ["logs/HEAD"]
        );

        #[cfg(not(feature = "git-version-stale"))]
        #[cfg(feature = "git-version-fallback")]
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
        fn fallback() -> &'static str {
            option_env!("GIT_VERSION").expect("git version fallback")
        }

        #[cfg(not(any(feature = "git-version-stale", feature = "git-version-fallback")))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always"],
            fallback = fallback()
        );

        #[cfg(feature = "git-version-stale")]
        #[cfg(not(feature = "git-version-fallback"))]
        let version = git_version_macro::git_version!(
            args = ["--abbrev=12", "--always"],
            fallback = fallback(),
            git_deps = ["logs/HEAD"]
        );

        #[cfg(feature = "git-version-fallback")]
        #[cfg(not(feature = "git-version-stale"))]
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

#[cfg(feature = "default-git-versions")]
pub use default_version::{long_raw_version_str, raw_version_str, raw_version_string};

/// Git version data.
pub(super) struct GitVersion {
    /// Abbreviated commit hash.
    pub(super) commit_hash: String,

    /// Tag name.
    pub(super) tag: Option<String>,

    /// Number of additional commits on top of the tag.
    #[allow(dead_code)]
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
