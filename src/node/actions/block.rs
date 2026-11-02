use std::net::SocketAddr;

use tracing::{info, warn};
use uuid::Uuid;

use crate::blockchain::block::Block;
use crate::network::message::{Message, MessageType, RejectReason};
use crate::network::{Network, NetworkError};

pub(crate) async fn receive_block(
    id: Uuid,
    block: &Block,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if !block.has_valid_hash()? {
        let message = Message::new(
            MessageType::Reject(RejectReason::InvalidBlockHash),
            Some(id),
        );
        return network.reply(addr, &message).await;
    }

    info!(
        peer = %network.peer_listen_addr(addr),
        height = block.height,
        "Block received"
    );

    let message = Message::new(MessageType::Ok, Some(id));
    network.reply(addr, &message).await
}

pub(crate) fn confirm_block(addr: SocketAddr, network: &Network) {
    info!(peer = %network.peer_listen_addr(addr), "Block acknowledged");
}

pub(crate) fn reject_block(reason: &RejectReason, addr: SocketAddr, network: &Network) {
    warn!(peer = %network.peer_listen_addr(addr), ?reason, "Block rejected");
}
