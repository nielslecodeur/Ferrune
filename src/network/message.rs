use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use uuid::Uuid;

use crate::StateError;
use crate::network::{Network, NetworkError, State};
use crate::node::actions;

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    id: Uuid,
    r#type: MessageType,
}

impl Message {
    pub fn new(r#type: MessageType, id: Option<Uuid>) -> Self {
        Self {
            id: id.unwrap_or(Uuid::new_v4()),
            r#type,
        }
    }

    pub fn with_id(id: Uuid, r#type: MessageType) -> Self {
        Self { id, r#type }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn state(&self) -> State {
        match &self.r#type {
            MessageType::LetsConnect => State::StartingConnection,
            MessageType::ChainId(_) => State::WaitingForChainId,
            MessageType::Register => State::WaitingForRegistration,
        }
    }

    pub(crate) async fn action(
        &self,
        addr: SocketAddr,
        network: &Network,
    ) -> Result<(), NetworkError> {
        match &self.r#type {
            MessageType::LetsConnect => {
                actions::connection::send_chain_id(self.id, addr, network).await
            }
            MessageType::ChainId(chain_id) => {
                actions::connection::verify_chain_id(self.id, *chain_id, addr, network).await
            }
            MessageType::Register => actions::connection::register(addr, network),
        }
    }

    pub(crate) async fn handle(
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

            message.action(addr, &network).await?;

            network.states().set_next_state(message.id())?;
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    LetsConnect,
    ChainId(u8),
    Register,
}
