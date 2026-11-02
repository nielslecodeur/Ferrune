use std::net::SocketAddr;

use tracing::{info, warn};
use uuid::Uuid;

use crate::blockchain::transaction::Transaction;
use crate::network::message::{Message, MessageType, RejectReason};
use crate::network::{Network, NetworkError};

pub(crate) async fn receive_transaction(
    id: Uuid,
    transaction: &Transaction,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if transaction.validate_basic().is_err() {
        let message = Message::new(
            MessageType::Reject(RejectReason::InvalidTransaction),
            Some(id),
        );
        return network.reply(addr, &message).await;
    }

    info!(
        peer = %network.peer_listen_addr(addr),
        ?transaction,
        "Transaction received"
    );

    let message = Message::new(MessageType::Ok, Some(id));
    network.reply(addr, &message).await
}

pub(crate) fn confirm_transaction(addr: SocketAddr, network: &Network) {
    info!(peer = %network.peer_listen_addr(addr), "Transaction acknowledged");
}

pub(crate) fn reject_transaction(reason: &RejectReason, addr: SocketAddr, network: &Network) {
    warn!(peer = %network.peer_listen_addr(addr), ?reason, "Transaction rejected");
}
