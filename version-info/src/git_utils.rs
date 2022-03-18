use regex::Regex;

/// Returns the git version as &'static str, in the long format:
/// either tag, number of additional commits and the abbreviated commit name,
/// or just commit hash in the case no tags found.
/// See `git describe` manual for details.
pub fn long_raw_version_str() -> &'static str {
    git_version::git_version!(args = ["--abbrev=12", "--always", "--long"])
}

/// Returns the git version as &'static str.
/// See `git describe` manual for details.
pub fn raw_version_str() -> &'static str {
    git_version::git_version!(args = ["--abbrev=12", "--always"])
}

/// Returns the git version as String.
pub fn raw_version_string() -> String {
    String::from(raw_version_str())
}

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
