use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use uuid::Uuid;

use crate::network::{Network, actions};
use crate::network::{NetworkError, State};

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
            MessageType::LetsConnect => State::WaitingForChainId,
            MessageType::ChainId(_) => State::WaitingForRegistration,
            MessageType::Register => State::None,
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
        network: Network,
    ) -> Result<(), NetworkError> {
        let mut lines = BufReader::new(reader).lines();

        async {
            while let Some(line) = lines.next_line().await? {
                let message: Self = serde_json::from_str(&line)?;

                if message.r#type.is_conversation_start() {
                    network
                        .states()
                        .register_state(message.id(), message.state())?;
                }

                message.action(addr, &network).await?;

                network.states().set_next_state(message.id())?;
            }

            Ok::<(), NetworkError>(())
        }
        .await
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    LetsConnect,
    ChainId(u8),
    Register,
}

impl MessageType {
    pub fn is_conversation_start(&self) -> bool {
        matches!(self, Self::LetsConnect)
    }
}
