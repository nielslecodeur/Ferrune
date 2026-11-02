use std::net::SocketAddr;
use std::sync::Arc;

use dashmap::DashMap;
use rand::RngExt;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::consts::{DISCOVERY_INTERVAL, MAX_PEERS};
use crate::network::NetworkError;
use crate::network::message::{Message, MessageType};
use crate::network::state::StateHandler;

type Writer = Arc<Mutex<OwnedWriteHalf>>;

pub(crate) struct Network {
    listen_addr: SocketAddr,
    chain_id: u8,

    states: StateHandler,

    ongoing_connections: DashMap<SocketAddr, Writer>,
    connections: DashMap<SocketAddr, Writer>,
    peer_addrs: DashMap<SocketAddr, SocketAddr>,
}

impl Network {
    pub(crate) fn new(listen_addr: SocketAddr, chain_id: u8) -> Self {
        Self {
            listen_addr,
            chain_id,
            states: StateHandler::default(),
            ongoing_connections: DashMap::new(),
            connections: DashMap::new(),
            peer_addrs: DashMap::new(),
        }
    }

    pub(crate) fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub(crate) fn chain_id(&self) -> u8 {
        self.chain_id
    }

    pub(crate) fn peer_listen_addr(&self, socket_addr: SocketAddr) -> SocketAddr {
        self.peer_addrs
            .get(&socket_addr)
            .map(|entry| *entry.value())
            .unwrap_or(socket_addr)
    }

    fn get_writer(&self, addr: SocketAddr) -> Result<Writer, NetworkError> {
        self.connections
            .get(&self.peer_listen_addr(addr))
            .or_else(|| self.ongoing_connections.get(&addr))
            .map(|entry| entry.value().clone())
            .ok_or(NetworkError::TcpStreamNotConnected(addr))
    }

    pub(crate) async fn listen(self: Arc<Self>) -> Result<(), NetworkError> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        info!(addr = %self.listen_addr, chain_id = self.chain_id, "Listening");

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            self.clone().spawn_connection(stream, peer_addr);
        }
    }

    pub(crate) async fn connect_to(self: Arc<Self>, addr: SocketAddr) -> Result<(), NetworkError> {
        if addr == self.listen_addr || self.is_connected(&addr) {
            return Ok(());
        }

        let stream = TcpStream::connect(addr).await?;
        self.clone().spawn_connection(stream, addr);

        self.converse(addr, MessageType::LetsConnect).await
    }

    fn spawn_connection(self: Arc<Self>, stream: TcpStream, socket_addr: SocketAddr) {
        let (reader, writer) = stream.into_split();
        self.ongoing_connections
            .insert(socket_addr, Arc::new(Mutex::new(writer)));

        tokio::spawn(async move {
            let result = Message::handle(reader, socket_addr, self.clone()).await;

            self.unregister_connection(socket_addr);

            if let Err(error) = result {
                warn!(addr = %socket_addr, %error, "Connection closed with error");
            }
        });
    }

    pub(crate) async fn discover_peers(self: Arc<Self>) {
        let mut interval = tokio::time::interval(DISCOVERY_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            if self.get_connections_number() >= MAX_PEERS {
                continue;
            }

            let Some(peer) = self.get_random_connection() else {
                continue;
            };

            if let Err(error) = self.converse(peer, MessageType::AskPerrAddr).await {
                warn!(%peer, %error, "Failed to ask for a peer address");
            }
        }
    }

    async fn converse(
        &self,
        addr: SocketAddr,
        message_type: MessageType,
    ) -> Result<(), NetworkError> {
        let message = Message::new(message_type, None);

        self.states.register_state(message.id(), message.state())?;
        self.states.set_next_state(message.id())?;
        self.send(addr, &message).await
    }

    pub(crate) async fn reply(
        &self,
        addr: SocketAddr,
        message: &Message,
    ) -> Result<(), NetworkError> {
        self.states.set_next_state(message.id())?;
        self.send(addr, message).await
    }

    async fn send(&self, addr: SocketAddr, message: &Message) -> Result<(), NetworkError> {
        let mut json_message = serde_json::to_string(message)?;
        json_message.push('\n');

        let writer = self.get_writer(addr)?;
        writer
            .lock()
            .await
            .write_all(json_message.as_bytes())
            .await?;

        Ok(())
    }

    pub(crate) fn register_connection(
        &self,
        socket_addr: SocketAddr,
        listen_addr: SocketAddr,
    ) -> Result<(), NetworkError> {
        let (_, writer) = self
            .ongoing_connections
            .remove(&socket_addr)
            .ok_or(NetworkError::TcpStreamNotConnected(socket_addr))?;

        if socket_addr != listen_addr {
            self.peer_addrs.insert(socket_addr, listen_addr);
        }
        self.connections.insert(listen_addr, writer);

        info!(
            peer = %listen_addr,
            peers = self.get_connections_number(),
            "Peer connected"
        );

        Ok(())
    }

    fn unregister_connection(&self, socket_addr: SocketAddr) {
        self.ongoing_connections.remove(&socket_addr);

        let listen_addr = self
            .peer_addrs
            .remove(&socket_addr)
            .map(|(_, addr)| addr)
            .unwrap_or(socket_addr);

        if self.connections.remove(&listen_addr).is_some() {
            info!(
                peer = %listen_addr,
                peers = self.get_connections_number(),
                "Peer disconnected"
            );
        }
    }

    pub(super) fn states(&self) -> &StateHandler {
        &self.states
    }

    fn get_connections_number(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn get_random_connection(&self) -> Option<SocketAddr> {
        let len = self.connections.len();
        if len == 0 {
            return None;
        }

        let index = rand::rng().random_range(0..len);
        self.connections.iter().nth(index).map(|entry| *entry.key())
    }

    fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.connections.contains_key(addr)
    }
}
