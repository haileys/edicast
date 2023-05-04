use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpListener;

#[derive(Error, Debug)]
#[error("could not bind {address}")]
pub struct BindError {
    pub address: SocketAddr,
    #[source]
    pub error: std::io::Error,
}

pub async fn bind(address: SocketAddr) -> Result<TcpListener, BindError> {
    TcpListener::bind(address).await
        .map_err(|error| BindError { address, error })
}

#[derive(Debug)]
pub struct SocketPeer(pub SocketAddr);
