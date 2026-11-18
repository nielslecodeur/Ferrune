use std::net::SocketAddr;

use tracing::{debug, warn};
use uuid::Uuid;

use crate::blockchain::transaction::SignedTransaction;
use crate::network::message::{Message, MessageType, RejectReason};
use crate::network::{Network, NetworkError};

pub(crate) async fn receive_transaction(
    id: Uuid,
    transaction: &SignedTransaction,
    addr: SocketAddr,
    network: &Network,
) -> Result<(), NetworkError> {
    if !network.is_valid_transaction(transaction) {
        let message = Message::new(
            MessageType::Reject(RejectReason::InvalidTransaction),
            Some(id),
        );
        return network.reply(addr, &message).await;
    }

    let peer = network.peer_listen_addr(addr);

    if network.add_transaction(transaction.clone())? {
        // One line per transaction: kept at debug, it was most of the CPU time.
        debug!(%peer, ?transaction, "Transaction added to mempool");

        let message = Message::new(MessageType::Ok, Some(id));
        network.reply(addr, &message).await?;

        network.broadcast_transaction(transaction, Some(peer)).await;
        return Ok(());
    }

    debug!(%peer, ?transaction, "Transaction already known");

    let message = Message::new(MessageType::Ok, Some(id));
    network.reply(addr, &message).await
}

pub(crate) fn confirm_transaction(addr: SocketAddr, network: &Network) {
    debug!(peer = %network.peer_listen_addr(addr), "Transaction acknowledged");
}

pub(crate) fn reject_transaction(reason: &RejectReason, addr: SocketAddr, network: &Network) {
    warn!(peer = %network.peer_listen_addr(addr), ?reason, "Transaction rejected");
}
