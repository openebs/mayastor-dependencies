#![cfg(feature = "rpc")]

use mayastor::{bdev_rpc_client::BdevRpcClient, mayastor_client::MayastorClient};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::Duration;
use tonic::transport::Channel;

pub mod mayastor {
    include!(concat!(env!("OUT_DIR"), "/mayastor.rs"));
}

#[derive(Clone)]
pub struct RpcHandle {
    pub name: String,
    pub endpoint: SocketAddr,
    pub mayastor: MayastorClient<Channel>,
    pub bdev: BdevRpcClient<Channel>,
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

        Ok(Self {
            name,
            mayastor,
            bdev,
            endpoint,
        })
    }
}
