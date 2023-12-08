//! Module to access v1 version of grpc APIs
use std::str::FromStr;

// dont export the raw pb generated code
mod pb {
    // Include Rust sources generated by protobuf.
    #![allow(unknown_lints)]
    #![allow(clippy::derive_partial_eq_without_eq)]
    #[cfg(target_os = "linux")]
    include!(concat!(env!("OUT_DIR"), "/mayastor.v1.rs"));

    // In order to IDE to work properly with protobuf definitions on non-Linux platform,
    // one can copy generated bindings (mayastor.v1.rs) from a Linux target and put them
    // to .pregenerated directory.
    // This has to be done every time Mayastor API V1 is changed.
    #[cfg(not(target_os = "linux"))]
    include!("../../../.pregenerated/mayastor.v1.rs");
}

pub mod common {
    pub use super::pb::ShareProtocol;
}

/// v1 version of bdev grpc API
pub mod bdev {
    pub use super::pb::{
        bdev_rpc_client::BdevRpcClient,
        bdev_rpc_server::{BdevRpc, BdevRpcServer},
        Bdev, BdevShareRequest, BdevShareResponse, BdevUnshareRequest, CreateBdevRequest,
        CreateBdevResponse, DestroyBdevRequest, ListBdevOptions, ListBdevResponse, ShareProtocol,
    };
}

/// v1 version of json-rpc grpc API
pub mod json {
    pub use super::pb::{
        json_rpc_client::JsonRpcClient,
        json_rpc_server::{JsonRpc, JsonRpcServer},
        JsonRpcRequest, JsonRpcResponse,
    };
}

pub mod pool {
    pub use super::pb::{
        pool_rpc_client::PoolRpcClient,
        pool_rpc_server::{PoolRpc, PoolRpcServer},
        CreatePoolRequest, DestroyPoolRequest, ExportPoolRequest, ImportPoolRequest,
        ListPoolOptions, ListPoolsResponse, Pool, PoolState, PoolType,
    };
}

pub mod replica {
    pub use super::pb::{
        destroy_replica_request, list_replica_options,
        replica_rpc_client::ReplicaRpcClient,
        replica_rpc_server::{ReplicaRpc, ReplicaRpcServer},
        CreateReplicaRequest, DestroyReplicaRequest, ListReplicaOptions, ListReplicasResponse,
        Replica, ReplicaSpaceUsage, ShareReplicaRequest, SnapshotInfo, UnshareReplicaRequest,
    };
}
pub mod snapshot {
    pub use super::pb::{
        destroy_snapshot_request, list_snapshots_request,
        snapshot_rpc_client::SnapshotRpcClient,
        snapshot_rpc_server::{SnapshotRpc, SnapshotRpcServer},
        CreateReplicaSnapshotRequest, CreateReplicaSnapshotResponse, CreateSnapshotCloneRequest,
        DestroySnapshotRequest, ListSnapshotCloneRequest, ListSnapshotCloneResponse,
        ListSnapshotsRequest, ListSnapshotsResponse, Nexus, NexusCreateSnapshotReplicaDescriptor,
        NexusCreateSnapshotReplicaStatus, NexusCreateSnapshotRequest, NexusCreateSnapshotResponse,
        Replica, SnapshotInfo,
    };
}
pub mod registration {
    pub use super::pb::{registration_client, ApiVersion, DeregisterRequest, RegisterRequest};
}
pub mod host {
    pub use super::pb::{
        block_device::{Filesystem, Partition},
        host_rpc_client::HostRpcClient,
        host_rpc_server::{HostRpc, HostRpcServer},
        BlockDevice, GetMayastorResourceUsageResponse, ListBlockDevicesRequest,
        ListBlockDevicesResponse, ListNvmeControllersResponse, MayastorFeatures,
        MayastorInfoResponse, NvmeController, NvmeControllerIoStats, NvmeControllerState,
        ResourceUsage, StatNvmeControllerRequest, StatNvmeControllerResponse,
    };
}

pub mod nexus {
    pub use super::pb::{
        nexus_rpc_client::NexusRpcClient,
        nexus_rpc_server::{NexusRpc, NexusRpcServer},
        AddChildNexusRequest, AddChildNexusResponse, Child, ChildAction, ChildOperationRequest,
        ChildOperationResponse, ChildState, ChildStateReason, CreateNexusRequest,
        CreateNexusResponse, DestroyNexusRequest, FaultNexusChildRequest, FaultNexusChildResponse,
        GetNvmeAnaStateRequest, GetNvmeAnaStateResponse, ListNexusOptions, ListNexusResponse,
        ListRebuildHistoryRequest, ListRebuildHistoryResponse, Nexus, NexusNvmePreemption,
        NexusState, NvmeAnaState, NvmeReservation, PauseRebuildRequest, PauseRebuildResponse,
        PublishNexusRequest, PublishNexusResponse, RebuildHistoryRecord, RebuildHistoryRequest,
        RebuildHistoryResponse, RebuildJobState, RebuildStateRequest, RebuildStateResponse,
        RebuildStatsRequest, RebuildStatsResponse, RemoveChildNexusRequest,
        RemoveChildNexusResponse, ResumeRebuildRequest, ResumeRebuildResponse,
        SetNvmeAnaStateRequest, SetNvmeAnaStateResponse, ShareProtocol, ShutdownNexusRequest,
        ShutdownNexusResponse, StartRebuildRequest, StartRebuildResponse, StopRebuildRequest,
        StopRebuildResponse, UnpublishNexusRequest, UnpublishNexusResponse,
    };
}

pub mod test {
    pub use super::pb::{
        test_rpc_client::TestRpcClient,
        test_rpc_server::{TestRpc, TestRpcServer},
        wipe_options, wipe_replica_request, wipe_replica_response, AddFaultInjectionRequest,
        FaultInjection, ListFaultInjectionsReply, ListFaultInjectionsRequest,
        RemoveFaultInjectionRequest, StreamWipeOptions, TestFeatures, WipeOptions,
        WipeReplicaRequest, WipeReplicaResponse,
    };
}

#[derive(Debug)]
pub enum Error {
    ParseError,
}

impl FromStr for nexus::NvmeAnaState {
    type Err = Error;
    fn from_str(state: &str) -> Result<Self, Self::Err> {
        match state {
            "optimized" => Ok(Self::NvmeAnaOptimizedState),
            "non_optimized" => Ok(Self::NvmeAnaNonOptimizedState),
            "inaccessible" => Ok(Self::NvmeAnaInaccessibleState),
            _ => Err(Error::ParseError),
        }
    }
}
