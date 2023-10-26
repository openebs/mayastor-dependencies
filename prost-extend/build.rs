fn main() {
    tonic_build::configure()
        .compile_well_known_types(true)
        .type_attribute(
            "google.protobuf.Duration",
            "#[derive(serde::Serialize, serde::Deserialize)] #[serde(default)]",
        )
        .compile(&["protobuf/v1/pb_time.proto"], &["protobuf/"])
        .unwrap_or_else(|e| panic!("prost-extend v1 protobuf compilation failed: {e}"));
}
