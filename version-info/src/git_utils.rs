use regex::Regex;

/// Returns the git version as &'static str.
pub fn raw_version_str() -> &'static str {
    git_version::git_version!(args = ["--abbrev=12", "--always"])
}

/// Returns the git version as String.
pub fn raw_version_string() -> String {
    String::from(raw_version_str())
}

/// Parses version string returned by git_version and returns a tuple of
/// (tag, revision, commit hash).
pub(crate) fn parse_git_version(git_ver: &str) -> (String, String, String) {
    let re = Regex::new(r"^(.+)-(.+)-g([0-9a-f]{12})$").unwrap();

    let cap = re.captures(git_ver).unwrap();
    assert_eq!(cap.len(), 4);

    (
        cap.get(1).unwrap().as_str().to_string(),
        cap.get(2).unwrap().as_str().to_string(),
        cap.get(3).unwrap().as_str().to_string(),
    )
}
