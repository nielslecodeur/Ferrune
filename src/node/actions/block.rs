use std::net::SocketAddr;

use tracing::{info, warn};
use uuid::Uuid;

use crate::blockchain::block::Block;
use crate::blockchain::core::BlockchainError;
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

    let peer = network.peer_listen_addr(addr);

    // Already received from another peer: nothing to do.
    if block.height < network.next_height() {
        let message = Message::new(MessageType::Ok, Some(id));
        return network.reply(addr, &message).await;
    }

    match network.add_block(block.clone()) {
        Ok(()) => {
            info!(%peer, height = block.height, "Block added");

            let message = Message::new(MessageType::Ok, Some(id));
            network.reply(addr, &message).await?;

            network.broadcast_block(block, Some(peer)).await;
            Ok(())
        }
        Err(error) => {
            warn!(%peer, height = block.height, ?error, "Invalid block received");

            let message = Message::new(MessageType::Reject(RejectReason::InvalidBlock), Some(id));
            network.reply(addr, &message).await?;

            // The peer is ahead of us: get the missing blocks.
            if let BlockchainError::InvalidHeight { expected, received } = error
                && received > expected
            {
                network.ask_blocks(addr, expected).await?;
            }

            Ok(())
        }
    }
}

pub(crate) fn confirm_block(addr: SocketAddr, network: &Network) {
    info!(peer = %network.peer_listen_addr(addr), "Block acknowledged");
}

pub(crate) fn reject_block(reason: &RejectReason, addr: SocketAddr, network: &Network) {
    warn!(peer = %network.peer_listen_addr(addr), ?reason, "Block rejected");
}
