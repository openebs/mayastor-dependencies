fn main() {
    tonic_build::configure()
        .compile(&["protobuf/v1/pb_time.proto"], &["protobuf/"])
        .unwrap_or_else(|e| panic!("prost-extend v1 protobuf compilation failed: {e}"));
}
