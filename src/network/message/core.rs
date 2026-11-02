use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use uuid::Uuid;

use super::MessageType;
use crate::network::{Network, NetworkError, State, StateError};
use crate::node::actions;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Message {
    id: Uuid,
    r#type: MessageType,
}

impl Message {
    pub(crate) fn new(r#type: MessageType, id: Option<Uuid>) -> Self {
        Self {
            id: id.unwrap_or(Uuid::new_v4()),
            r#type,
        }
    }

    pub(in crate::network) fn id(&self) -> Uuid {
        self.id
    }

    pub(in crate::network) fn state(&self) -> State {
        match &self.r#type {
            MessageType::LetsConnect => State::StartingConnection,
            MessageType::ChainId(_) => State::WaitingForChainId,
            MessageType::Register(_) => State::WaitingForRegistration,
            MessageType::AskPerrAddr => State::AskingPeerAddress,
            MessageType::PeerAddress(_) | MessageType::NoPeerFound => State::WaitingForPeerAddress,
        }
    }

    async fn action(
        &self,
        addr: SocketAddr,
        network: &Arc<Network>,
    ) -> Result<(), NetworkError> {
        match &self.r#type {
            MessageType::LetsConnect => {
                actions::connection::send_chain_id(self.id, addr, network).await
            }
            MessageType::ChainId(chain_id) => {
                actions::connection::verify_chain_id(self.id, *chain_id, addr, network).await
            }
            MessageType::Register(listen_addr) => {
                actions::connection::register(addr, *listen_addr, network)
            }
            MessageType::AskPerrAddr => {
                actions::connection::send_peer_addr(self.id, addr, network).await
            }
            MessageType::PeerAddress(peer_addr) => {
                actions::connection::add_connection(*peer_addr, network.clone()).await
            }
            MessageType::NoPeerFound => Ok(()),
        }
    }

    pub(in crate::network) async fn handle(
        reader: OwnedReadHalf,
        addr: SocketAddr,
        network: Arc<Network>,
    ) -> Result<(), NetworkError> {
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            let message: Self = serde_json::from_str(&line)?;

            if message.state().is_conversation_start() {
                network
                    .states()
                    .register_state(message.id(), message.state())?;
            }

            let expected = network
                .states()
                .get_state(message.id())
                .ok_or(StateError::ConversationNotStarted(message.id()))?;

            if expected != message.state() {
                return Err(StateError::UnexpectedState {
                    id: message.id(),
                    expected,
                    received: message.state(),
                }
                .into());
            }

            network.states().set_next_state(message.id())?;

            message.action(addr, &network).await?;
        }

        Ok(())
    }
}
