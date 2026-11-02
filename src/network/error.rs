use std::io;
use std::net::SocketAddr;

use thiserror::Error;

use crate::network::state::StateError;

#[derive(Error, Debug)]
pub(crate) enum NetworkError {
    #[error("No active TCP stream for {0}")]
    TcpStreamNotConnected(SocketAddr),

    #[error("JSON serialization error")]
    JsonSerialization(#[from] serde_json::Error),

    #[error("TCP I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("State error: {0}")]
    State(#[from] StateError),

    #[error("Failed to receive answer: {0}")]
    ReceiveError(#[from] tokio::sync::oneshot::error::RecvError),

    #[error("Already connected to {0}")]
    AlreadyConnected(SocketAddr),

    #[error("Timed out while connecting to {0}")]
    ConnectTimeout(SocketAddr),

    #[error("Connection rejected by peer: {0}")]
    ConnectionRejected(String),

    #[error("The chain is different")]
    DifferentChain,
}
