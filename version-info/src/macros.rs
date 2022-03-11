/// Makes a version info instance.
#[macro_export]
macro_rules! version_info {
    () => {
        $crate::VersionInfo::new(
            $crate::raw_version_string(),
            String::from(env!("CARGO_PKG_NAME")),
            String::from(env!("CARGO_PKG_DESCRIPTION")),
            String::from(env!("CARGO_PKG_VERSION")),
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
/// Each call to this macro leaks a string.
#[macro_export]
macro_rules! version_info_str {
    () => {
        Box::leak(Box::new(String::from($crate::version_info!()))) as &'static str
    };
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
