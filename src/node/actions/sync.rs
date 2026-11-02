use std::net::SocketAddr;

use tracing::info;
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

pub(crate) fn receive_height(height: Option<u64>, addr: SocketAddr, network: &Network) {
    info!(
        peer = %network.peer_listen_addr(addr),
        ?height,
        local_height = ?network.height(),
        "Height received"
    );
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

pub(crate) fn receive_blocks(
    blocks: &[Block],
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if blocks.len() > MAX_BLOCKS_PER_MESSAGE {
        return Err(NetworkError::InvalidBlock);
    }

    for block in blocks {
        if !block.has_valid_hash()? {
            return Err(NetworkError::InvalidBlock);
        }
    }

    info!(
        peer = %network.peer_listen_addr(addr),
        count = blocks.len(),
        first = ?blocks.first().map(|block| block.height),
        last = ?blocks.last().map(|block| block.height),
        "Blocks received"
    );

    Ok(())
}
