use std::{env, path::PathBuf};

extern crate tonic_build;

fn main() {
    let reflection_descriptor =
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("io_engine_reflection.bin");
    tonic_build::configure()
        .file_descriptor_set_path(&reflection_descriptor)
        .build_server(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .extern_path(".google.protobuf.Timestamp", "::prost_extend::Timestamp")
        .compile(&["protobuf/mayastor.proto"], &["protobuf"])
        .unwrap_or_else(|e| panic!("io-engine protobuf compilation failed: {}", e));

    tonic_build::configure()
        .file_descriptor_set_path(&reflection_descriptor)
        .build_server(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .extern_path(".google.protobuf.Timestamp", "::prost_extend::Timestamp")
        .extern_path(".google.protobuf.Duration", "::prost_extend::Duration")
        .compile(
            &[
                "protobuf/v1/bdev.proto",
                "protobuf/v1/json.proto",
                "protobuf/v1/pool.proto",
                "protobuf/v1/replica.proto",
                "protobuf/v1/host.proto",
                "protobuf/v1/nexus.proto",
                "protobuf/v1/registration.proto",
                "protobuf/v1/snapshot.proto",
                "protobuf/v1/snapshot-rebuild.proto",
                "protobuf/v1/stats.proto",
                "protobuf/v1/test.proto",
            ],
            &["protobuf/v1"],
        )
        .unwrap_or_else(|e| panic!("io-engine v1 protobuf compilation failed: {}", e));
}
