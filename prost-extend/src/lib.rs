mod timestamp;

mod common_types {
    #![allow(clippy::derive_partial_eq_without_eq)]
    #![allow(clippy::large_enum_variant)]
    tonic::include_proto!("v1.pb_time");
}

pub use common_types::Timestamp;
