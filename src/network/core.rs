use std::{net::SocketAddr, sync::Arc};

use dashmap::DashMap;
use rand::RngExt;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream, tcp::OwnedWriteHalf},
    sync::Mutex,
};

use crate::network::{
    NetworkError,
    message::{Message, MessageType},
    state::StateHandler,
};

type Writer = Arc<Mutex<OwnedWriteHalf>>;

const MAX_PEERS: usize = 8;
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(60);

pub struct Network {
    // Address this node listens on, announced to peers in LetsConnect
    listen_addr: SocketAddr,

    states: StateHandler,

    // Keyed by socket address (ephemeral port for incoming connections)
    ongoing_connections: DashMap<SocketAddr, Writer>,
    // Keyed by the peer's listening address
    connections: DashMap<SocketAddr, Writer>,
    // Socket address -> listening address, for incoming connections
    peer_addrs: DashMap<SocketAddr, SocketAddr>,
}

impl Network {
    pub fn new(listen_addr: SocketAddr) -> Self {
        Self {
            listen_addr,
            states: StateHandler::default(),
            ongoing_connections: DashMap::new(),
            connections: DashMap::new(),
            peer_addrs: DashMap::new(),
        }
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    // Clones the writer so no DashMap lock is held across an await
    fn get_writer(&self, addr: SocketAddr) -> Result<Writer, NetworkError> {
        self.connections
            .get(&addr)
            .or_else(|| self.ongoing_connections.get(&addr))
            .map(|entry| entry.value().clone())
            .ok_or(NetworkError::TcpStreamNotConnected(addr))
    }

    pub async fn listen(self: Arc<Self>) -> Result<(), NetworkError> {
        let listener = TcpListener::bind(self.listen_addr).await?;

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            self.clone().spawn_connection(stream, peer_addr);
        }
    }

    pub async fn connect_to(self: Arc<Self>, addr: SocketAddr) -> Result<(), NetworkError> {
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
                todo!("Handle connection error: {error}");
            }
        });
    }

    pub async fn discover_peers(self: Arc<Self>) {
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
                eprintln!("Failed to ask {peer} for a peer address: {error}");
            }
        }
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

        let writer = self.get_writer(addr)?;
        writer
            .lock()
            .await
            .write_all(json_message.as_bytes())
            .await?;

        Ok(())
    }

    // socket_addr: address the connection was opened with (ephemeral port if incoming)
    // listen_addr: address the peer listens on (announced in LetsConnect)
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

        Ok(())
    }

    fn unregister_connection(&self, socket_addr: SocketAddr) {
        self.ongoing_connections.remove(&socket_addr);

        let listen_addr = self
            .peer_addrs
            .remove(&socket_addr)
            .map(|(_, addr)| addr)
            .unwrap_or(socket_addr);

        self.connections.remove(&listen_addr);
    }

    pub(crate) fn states(&self) -> &StateHandler {
        &self.states
    }

    pub(crate) fn get_connections_number(&self) -> usize {
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

    pub(crate) fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.connections.contains_key(addr)
    }
}
