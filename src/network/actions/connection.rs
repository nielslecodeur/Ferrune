use std::net::SocketAddr;

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
    let message = Message::with_id(id, MessageType::ChainId(CHAIN_ID));

    network.send(addr, &message).await
}

pub(crate) async fn verify_chain_id(
    id: Uuid,
    chain_id: u8,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if chain_id != CHAIN_ID {
        return Err(NetworkError::DifferentChain);
    }

    let message = Message::with_id(id, MessageType::Register);
    network.send(addr, &message).await?;

    register(addr, network)
}

pub(crate) fn register(addr: SocketAddr, network: &Network) -> Result<(), NetworkError> {
    network.register_connection(addr)?;

    Ok(())
}
