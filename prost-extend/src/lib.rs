mod timestamp;

mod well_known_types {
    include!(concat!(env!("OUT_DIR"), "/google.protobuf.rs"));
}

pub use well_known_types::Timestamp;
