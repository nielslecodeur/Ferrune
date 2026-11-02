use std::net::SocketAddr;
use std::sync::Arc;

use tracing::warn;
use uuid::Uuid;

use crate::network::message::{Message, MessageType};
use crate::network::{Network, NetworkError};

/// Called on the accepting side: checks the initiator's chain, then answers with ours.
pub(crate) async fn send_chain_id(
    id: Uuid,
    chain_id: u8,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if chain_id != network.chain_id() {
        return Err(NetworkError::DifferentChain);
    }

    let message = Message::new(MessageType::ChainId(network.chain_id()), Some(id));
    network.reply(addr, &message).await
}

/// Called on the initiating side: checks the peer's chain, registers it, then sends our listen address.
pub(crate) async fn verify_chain_id(
    id: Uuid,
    chain_id: u8,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if chain_id != network.chain_id() {
        return Err(NetworkError::DifferentChain);
    }

    register(addr, addr, network)?;

    let message = Message::new(MessageType::Register(network.listen_addr()), Some(id));
    network.reply(addr, &message).await
}

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
    let asker = network.peer_listen_addr(addr);

    let message_type = match network.get_random_connection(Some(asker)) {
        Some(peer) => MessageType::PeerAddress(peer),
        None => MessageType::NoPeerFound,
    };

    let message = Message::new(message_type, Some(id));
    network.reply(addr, &message).await
}

/// Connects to a discovered peer in the background, so that a failure does not
/// affect the connection with the peer that advertised it.
pub(crate) fn add_connection(addr: SocketAddr, network: Arc<Network>) {
    tokio::spawn(async move {
        if let Err(error) = network.connect_to(addr).await {
            warn!(peer = %addr, %error, "Failed to connect to discovered peer");
        }
    });
}
