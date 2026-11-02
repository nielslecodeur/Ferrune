use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::blockchain::block::Block;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum MessageType {
    LetsConnect(u8),
    ChainId(u8),
    Register(SocketAddr),
    AskPeerAddr,
    PeerAddress(SocketAddr),
    NoPeerFound,
    SendBlock(Block),
    Ok,
}
