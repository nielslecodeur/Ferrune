use std::net::SocketAddr;

use tracing::{debug, info};
use uuid::Uuid;

use crate::network::message::{Message, MessageType};
use crate::network::{Network, NetworkError};

pub(crate) async fn send_pong(
    id: Uuid,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    let message = Message::new(MessageType::Pong, Some(id));
    network.reply(addr, &message).await
}

pub(crate) fn receive_pong(addr: SocketAddr, network: &Network) {
    debug!(peer = %network.peer_listen_addr(addr), "Pong received");
}

pub(crate) fn receive_disconnect(addr: SocketAddr, network: &Network) {
    info!(peer = %network.peer_listen_addr(addr), "Peer requested disconnection");
}
