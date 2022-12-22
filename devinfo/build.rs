#[cfg(target_os = "linux")]
use std::{env, path::PathBuf};

#[cfg(target_os = "linux")]
fn main() {
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .allowlist_function("^blkid.*")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("libblkid.rs"))
        .expect("failed to generate bindings");

    println!("cargo:rustc-link-lib=blkid");
}

#[cfg(not(target_os = "linux"))]
fn main() {}
