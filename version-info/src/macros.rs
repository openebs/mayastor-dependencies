/// Makes a version info instance.
#[macro_export]
macro_rules! version_info {
    () => {
        $crate::VersionInfo::new(
            String::from($crate::long_raw_version_str()),
            String::from(env!("CARGO_PKG_NAME")),
            String::from(env!("CARGO_PKG_DESCRIPTION")),
            Some(String::from(env!("CARGO_PKG_VERSION"))),
            option_env!("CARGO_BIN_NAME").map(|s| s.to_string()),
            if cfg!(debug_assertions) {
                String::from("debug")
            } else {
                String::from("")
            },
        )
    };
    ($pkg_version:expr, $version:expr) => {
        $crate::VersionInfo::new(
            String::from($version),
            String::from(env!("CARGO_PKG_NAME")),
            String::from(env!("CARGO_PKG_DESCRIPTION")),
            $pkg_version,
            option_env!("CARGO_BIN_NAME").map(|s| s.to_string()),
            if cfg!(debug_assertions) {
                String::from("debug")
            } else {
                String::from("")
            },
        )
    };
}

/// Returns a version info instance.
#[macro_export]
macro_rules! package_description {
    () => {
        $crate::version_info!().fmt_description()
    };
}

/// Gets package's version info as a String.
#[macro_export]
macro_rules! version_info_string {
    () => {
        String::from($crate::version_info!())
    };
}

/// Gets package's version info as a static str.
#[macro_export]
macro_rules! version_info_str {
    () => {{
        static mut BUF: Vec<u8> = Vec::new();
        let s = String::from($crate::version_info!());
        unsafe {
            BUF.resize(s.len(), 0);
            BUF.clone_from_slice(s.as_bytes());
        }
        unsafe { std::str::from_utf8_unchecked(BUF.as_slice()) }
    }};
}

/// Formats package related information.
/// This includes the package name and version, and commit info.
#[macro_export]
macro_rules! fmt_package_info {
    () => {{
        let vi = $crate::version_info!();
        format!("{} {}", vi.fmt_description(), vi)
    }};
}

/// Prints package related information.
/// This includes the package name and version, and commit info.
#[macro_export]
macro_rules! print_package_info {
    () => {
        println!("{}", $crate::fmt_package_info!());
    };
}
