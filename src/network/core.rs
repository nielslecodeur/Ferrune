use std::{net::SocketAddr, sync::Arc};

use dashmap::{DashMap, mapref::one::RefMut};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream, tcp::OwnedWriteHalf},
};

use crate::network::{
    NetworkError,
    message::{Message, MessageType},
    state::StateHandler,
};

#[derive(Clone, Default)]
pub struct Network {
    states: Arc<StateHandler>,

    ongoing_connections: Arc<DashMap<SocketAddr, OwnedWriteHalf>>,
    connections: Arc<DashMap<SocketAddr, OwnedWriteHalf>>,
}

impl Network {
    fn get_writer(
        &self,
        addr: SocketAddr,
    ) -> Result<RefMut<'_, SocketAddr, OwnedWriteHalf>, NetworkError> {
        self.connections
            .get_mut(&addr)
            .or_else(|| self.ongoing_connections.get_mut(&addr))
            .ok_or(NetworkError::TcpStreamNotConnected(addr))
    }

    pub async fn listen(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        let listener = TcpListener::bind(addr).await?;

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let (reader, writer) = stream.into_split();
            self.ongoing_connections.insert(peer_addr, writer);

            let network = self.clone();

            tokio::spawn(async move {
                let result = Message::handle(reader, peer_addr, network.clone()).await;

                network.ongoing_connections.remove(&peer_addr);
                network.connections.remove(&peer_addr);

                if let Err(error) = result {
                    todo!("Handle connection error: {error}");
                }
            });
        }
    }

    pub async fn connect_to(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = stream.into_split();

        self.ongoing_connections.insert(addr, writer);

        let network = self.clone();

        tokio::spawn(async move {
            let result = Message::handle(reader, addr, network.clone()).await;

            network.ongoing_connections.remove(&addr);
            network.connections.remove(&addr);

            if let Err(error) = result {
                todo!("Handle connection error: {error}");
            }
        });

        self.converse(addr, MessageType::LetsConnect).await
    }

    // Use this function to create a conversation with another peer
    pub async fn converse(
        &self,
        addr: SocketAddr,
        message_type: MessageType,
    ) -> Result<(), NetworkError> {
        let message = Message::new(message_type, None);

        self.states.register_state(message.id(), message.state())?;
        self.states.set_next_state(message.id())?;
        self.send(addr, &message).await
    }

    // Use this function if the conversation has already started, and you got the id
    pub async fn send(&self, addr: SocketAddr, message: &Message) -> Result<(), NetworkError> {
        let mut json_message = serde_json::to_string(message)?;
        json_message.push('\n');

        self.get_writer(addr)?
            .write_all(json_message.as_bytes())
            .await?;

        Ok(())
    }

    pub(crate) fn register_connection(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        let (_, writer) = self
            .ongoing_connections
            .remove(&addr)
            .ok_or(NetworkError::TcpStreamNotConnected(addr))?;

        self.connections.insert(addr, writer);

        Ok(())
    }

    pub(crate) fn states(&self) -> &Arc<StateHandler> {
        &self.states
    }
}
