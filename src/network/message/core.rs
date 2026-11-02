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
            id: id.unwrap_or_else(Uuid::new_v4),
            r#type,
        }
    }

    pub(in crate::network) fn id(&self) -> Uuid {
        self.id
    }

    pub(in crate::network) fn state(&self) -> State {
        match &self.r#type {
            MessageType::LetsConnect(_) => State::StartingConnection,
            MessageType::ChainId(_) => State::WaitingForChainId,
            MessageType::Register(_) => State::WaitingForRegistration,
            MessageType::AskPeerAddr => State::AskingPeerAddress,
            MessageType::PeerAddress(_) | MessageType::NoPeerFound => State::WaitingForPeerAddress,
            MessageType::Ok => State::WaitingForConfirmation,
            MessageType::SendBlock(_) => State::SendingBlock,
        }
    }

    fn is_expected(&self, expected: State) -> bool {
        match &self.r#type {
            MessageType::Ok => matches!(
                expected,
                State::WaitingForConfirmation | State::WaitingForBlockConfirmation
            ),
            _ => self.state() == expected,
        }
    }

    async fn action(
        &self,
        addr: SocketAddr,
        network: &Arc<Network>,
        expected: State,
    ) -> Result<(), NetworkError> {
        match &self.r#type {
            MessageType::LetsConnect(chain_id) => {
                actions::connection::send_chain_id(self.id, *chain_id, addr, network).await
            }
            MessageType::ChainId(chain_id) => {
                actions::connection::verify_chain_id(self.id, *chain_id, addr, network).await
            }
            MessageType::Register(listen_addr) => {
                actions::connection::register(self.id, addr, *listen_addr, network).await
            }
            MessageType::AskPeerAddr => {
                actions::connection::send_peer_addr(self.id, addr, network).await
            }
            MessageType::PeerAddress(peer_addr) => {
                actions::connection::add_connection(*peer_addr, network.clone());
                Ok(())
            }
            MessageType::NoPeerFound => Ok(()),
            MessageType::Ok => match expected {
                State::WaitingForBlockConfirmation => {
                    actions::block::confirm_block(addr, network);
                    Ok(())
                }
                _ => actions::connection::confirm_registration(addr, network),
            },
            MessageType::SendBlock(block) => {
                actions::block::receive_block(self.id, block, addr, network).await
            }
        }
    }

    /// Reads and processes messages from a peer until the stream closes.
    ///
    /// Any error (invalid JSON, protocol violation, different chain, ...) is
    /// considered fatal for this peer: it is returned and the connection is closed.
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

            if !message.is_expected(expected) {
                return Err(StateError::UnexpectedState {
                    id: message.id(),
                    expected,
                    received: message.state(),
                }
                .into());
            }

            network.states().set_next_state(message.id())?;

            message.action(addr, &network, expected).await?;
        }

        Ok(())
    }
}
