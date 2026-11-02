use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum MessageType {
    LetsConnect(u8),
    ChainId(u8),
    Register(SocketAddr),
    AskPeerAddr,
    PeerAddress(SocketAddr),
    NoPeerFound,
    Ok,
}
