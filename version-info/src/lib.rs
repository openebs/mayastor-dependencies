mod git_utils;
mod version_info;
pub use git_utils::{long_raw_version_str, raw_version_str, raw_version_string};
pub use version_info::VersionInfo;
pub mod macros;
