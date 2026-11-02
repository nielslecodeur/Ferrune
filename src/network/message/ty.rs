use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::blockchain::block::Block;
use crate::blockchain::transaction::Transaction;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum MessageType {
    LetsConnect(u8),
    ChainId(u8),
    Register(SocketAddr),
    AskPeerAddr,
    PeerAddress(SocketAddr),
    NoPeerFound,
    SendBlock(Block),
    SendTransaction(Transaction),
    AskHeight,
    Height(Option<u64>),
    AskBlocks { from: u64 },
    Blocks(Vec<Block>),
    Ping,
    Pong,
    Disconnect,
    Ok,
    Reject(RejectReason),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum RejectReason {
    InvalidBlockHash,
    InvalidTransaction,
}
