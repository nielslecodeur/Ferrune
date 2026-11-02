use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    LetsConnect,
    ChainId(u8),
    Register(SocketAddr),
    AskPerrAddr,
    PeerAddress(SocketAddr),
    NoPeerFound,
}
