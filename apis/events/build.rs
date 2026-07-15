fn main() {
    let descriptor_path =
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("event_descriptor.bin");

    tonic_build::configure()
        .file_descriptor_set_path(&descriptor_path)
        .extern_path(".google.protobuf.Timestamp", "::prost_extend::Timestamp")
        .extern_path(".google.protobuf.Duration", "::prost_extend::Duration")
        .enum_attribute(
            "v1.event.EventCategory",
            "#[derive(strum_macros::EnumIter, strum_macros::Display, strum_macros::EnumString)]\
             \n#[strum(serialize_all = \"kebab-case\")]",
        )
        .enum_attribute(
            "v1.event.EventAction",
            "#[derive(strum_macros::EnumIter, strum_macros::Display, strum_macros::EnumString)]\
             \n#[strum(serialize_all = \"kebab-case\")]",
        )
        .compile_protos(&["protobuf/v1/event.proto"], &["protobuf/"])
        .unwrap_or_else(|e| panic!("event v1 protobuf compilation failed: {e}"));

    let descriptor_bytes = std::fs::read(&descriptor_path)
        .unwrap_or_else(|e| panic!("failed to read event descriptor: {e}"));
    pbjson_build::Builder::new()
        .register_descriptors(&descriptor_bytes)
        .unwrap_or_else(|e| panic!("failed to register event descriptors: {e}"))
        .extern_path(".google.protobuf.Timestamp", "::prost_extend::Timestamp")
        .extern_path(".google.protobuf.Duration", "::prost_extend::Duration")
        .build(&[".v1.event"])
        .unwrap_or_else(|e| panic!("pbjson serde generation failed: {e}"));
}
