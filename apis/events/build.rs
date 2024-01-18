fn main() {
    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .extern_path(".google.protobuf.Timestamp", "::prost_extend::Timestamp")
        .extern_path(".google.protobuf.Duration", "::prost_extend::Duration")
        .compile(&["protobuf/v1/event.proto"], &["protobuf/"])
        .unwrap_or_else(|e| panic!("event v1 protobuf compilation failed: {e}"));
}
