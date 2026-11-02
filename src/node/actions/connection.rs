use std::net::SocketAddr;
use std::sync::Arc;

use uuid::Uuid;

use crate::network::message::{Message, MessageType};
use crate::network::{Network, NetworkError};

pub(crate) async fn send_chain_id(
    id: Uuid,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let message = Message::new(MessageType::ChainId(network.chain_id()), Some(id));
    network.reply(addr, &message).await
}

pub(crate) async fn verify_chain_id(
    id: Uuid,
    chain_id: u8,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if chain_id != network.chain_id() {
        return Err(NetworkError::DifferentChain);
    }

    let message = Message::new(MessageType::Register(network.listen_addr()), Some(id));
    network.reply(addr, &message).await?;

    register(addr, addr, network)
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
    let message_type = match network.get_random_connection() {
        Some(peer) if peer != network.peer_listen_addr(addr) => MessageType::PeerAddress(peer),
        _ => MessageType::NoPeerFound,
    };

    let message = Message::new(message_type, Some(id));
    network.reply(addr, &message).await
}

pub(crate) async fn add_connection(
    addr: SocketAddr,
    network: Arc<Network>,
) -> Result<(), NetworkError> {
    network.connect_to(addr).await
}
