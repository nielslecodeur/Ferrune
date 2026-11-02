use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    LetsConnect,
    ChainId(u8),
    // Listening address of the peer initiating the connection
    Register(SocketAddr),
    AskPerrAddr,
    PeerAddress(SocketAddr),
    NoPeerFound,
}
