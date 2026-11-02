use std::{net::SocketAddr, sync::Arc};
use uuid::Uuid;

use crate::network::{
    Network, NetworkError,
    message::{Message, MessageType},
};

static CHAIN_ID: u8 = 0;

pub(crate) async fn send_chain_id(
    id: Uuid,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let message = Message::new(MessageType::ChainId(CHAIN_ID), Some(id));
    network.send(addr, &message).await
}

// Initiator side: addr is the peer's listening address (we connected to it)
pub(crate) async fn verify_chain_id(
    id: Uuid,
    chain_id: u8,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if chain_id != CHAIN_ID {
        return Err(NetworkError::DifferentChain);
    }

    let message = Message::new(MessageType::Register(network.listen_addr()), Some(id));
    network.send(addr, &message).await?;

    register(addr, addr, network)
}

// addr: socket address of the connection (ephemeral port if incoming)
// listen_addr: address the peer listens on
pub(crate) fn register(
    addr: SocketAddr,
    listen_addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    network.register_connection(addr, listen_addr)
}

pub(crate) async fn send_peer_addr(
    id: Uuid,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let message_type = match network.get_random_connection() {
        // Don't send the requester its own address
        Some(peer) if peer != addr => MessageType::PeerAddress(peer),
        _ => MessageType::NoPeerFound,
    };

    let message = Message::new(message_type, Some(id));
    network.send(addr, &message).await
}

pub(crate) async fn add_connection(
    addr: SocketAddr,
    network: Arc<Network>,
) -> Result<(), NetworkError> {
    // connect_to opens the TCP stream, skips known peers and ourselves
    network.connect_to(addr).await
}
