use std::net::SocketAddr;

use tracing::{info, warn};
use uuid::Uuid;

use crate::blockchain::block::Block;
use crate::consts::MAX_BLOCKS_PER_MESSAGE;
use crate::network::message::{Message, MessageType};
use crate::network::{Network, NetworkError};

pub(crate) async fn send_height(
    id: Uuid,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let message = Message::new(MessageType::Height(network.height()), Some(id));
    network.reply(addr, &message).await
}

pub(crate) async fn receive_height(
    height: Option<u64>,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    info!(
        peer = %network.peer_listen_addr(addr),
        ?height,
        local_height = ?network.height(),
        "Height received"
    );

    let from = network.next_height();

    match height {
        Some(height) if height >= from => network.ask_blocks(addr, from).await,
        _ => Ok(()),
    }
}

pub(crate) async fn send_blocks(
    id: Uuid,
    from: u64,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let message = Message::new(MessageType::Blocks(network.blocks_from(from)), Some(id));
    network.reply(addr, &message).await
}

pub(crate) async fn receive_blocks(
    blocks: &[Block],
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let peer = network.peer_listen_addr(addr);

    if blocks.len() > MAX_BLOCKS_PER_MESSAGE {
        warn!(%peer, count = blocks.len(), "Too many blocks received, ignoring them");
        return Ok(());
    }

    for block in blocks {
        if !block.has_valid_hash()? {
            warn!(%peer, height = block.height, "Invalid block received, ignoring the batch");
            return Ok(());
        }
    }

    for block in blocks {
        // Already received from another peer in the meantime.
        if block.height < network.next_height() {
            continue;
        }

        if let Err(error) = network.add_block(block.clone()) {
            warn!(%peer, height = block.height, %error, "Invalid block received, stopping sync");
            return Ok(());
        }
    }

    info!(
        %peer,
        count = blocks.len(),
        local_height = ?network.height(),
        "Blocks received"
    );

    // A full batch means the peer may have more blocks.
    if blocks.len() == MAX_BLOCKS_PER_MESSAGE {
        network.ask_blocks(addr, network.next_height()).await?;
    }

    Ok(())
}
