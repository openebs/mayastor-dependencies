#![cfg(feature = "rpc")]

use mayastor::{
    bdev_rpc_client::BdevRpcClient, json_rpc_client::JsonRpcClient, mayastor_client::MayastorClient,
};

use std::{
    net::{SocketAddr, TcpStream},
    thread,
    time::Duration,
};
use tonic::transport::Channel;

pub mod mayastor {
    #![allow(unknown_lints)]
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/mayastor.rs"));
}

#[derive(Clone)]
pub struct RpcHandle {
    pub name: String,
    pub endpoint: SocketAddr,
    pub mayastor: MayastorClient<Channel>,
    pub bdev: BdevRpcClient<Channel>,
    pub jsonrpc: JsonRpcClient<Channel>,
}

impl RpcHandle {
    /// connect to the containers and construct a handle
    pub(crate) async fn connect(name: String, endpoint: SocketAddr) -> Result<Self, String> {
        let mut attempts = 40;
        loop {
            if TcpStream::connect_timeout(&endpoint, Duration::from_millis(100)).is_ok() {
                break;
            } else {
                thread::sleep(Duration::from_millis(101));
            }
            attempts -= 1;
            if attempts == 0 {
                return Err(format!("Failed to connect to {}/{}", name, endpoint));
            }
        }

        let mayastor = MayastorClient::connect(format!("http://{}", endpoint))
            .await
            .unwrap();
        let bdev = BdevRpcClient::connect(format!("http://{}", endpoint))
            .await
            .unwrap();
        let jsonrpc = JsonRpcClient::connect(format!("http://{}", endpoint))
            .await
            .unwrap();

        Ok(Self {
            name,
            endpoint,
            mayastor,
            bdev,
            jsonrpc,
        })
    }
}
