use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use dashmap::{DashMap, DashSet};
use rand::RngExt;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};

use crate::blockchain::block::Block;
use crate::blockchain::core::{Blockchain, BlockchainError};
use crate::blockchain::transaction::Transaction;
use crate::consts::{
    CONNECT_TIMEOUT, DISCOVERY_INTERVAL, MAX_BLOCKS_PER_MESSAGE, MAX_PEERS, PEER_TIMEOUT,
    PING_INTERVAL,
};
use crate::network::NetworkError;
use crate::network::message::{Message, MessageType};
use crate::network::state::StateHandler;

type Writer = Arc<Mutex<OwnedWriteHalf>>;

pub(crate) struct Network {
    listen_addr: SocketAddr,
    chain_id: u8,
    blockchain: Arc<RwLock<Blockchain>>,

    states: StateHandler,

    /// Addresses we are currently dialing (before the TCP stream exists).
    dialing: DashSet<SocketAddr>,
    /// Connections whose handshake is not finished, keyed by socket address.
    ongoing_connections: DashMap<SocketAddr, Writer>,
    /// Registered peers, keyed by their listen address.
    connections: DashMap<SocketAddr, Writer>,
    peer_addrs: DashMap<SocketAddr, SocketAddr>,
    /// Last time we received a message from a peer, keyed by its listen address.
    last_seen: DashMap<SocketAddr, Instant>,
}

impl Network {
    pub(crate) fn new(
        listen_addr: SocketAddr,
        chain_id: u8,
        blockchain: Arc<RwLock<Blockchain>>,
    ) -> Self {
        Self {
            listen_addr,
            chain_id,
            blockchain,
            states: StateHandler::default(),
            dialing: DashSet::new(),
            ongoing_connections: DashMap::new(),
            connections: DashMap::new(),
            peer_addrs: DashMap::new(),
            last_seen: DashMap::new(),
        }
    }

    pub(crate) fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub(crate) fn chain_id(&self) -> u8 {
        self.chain_id
    }

    pub(crate) fn height(&self) -> Option<u64> {
        self.blockchain
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .height()
    }

    pub(crate) fn blocks_from(&self, from: u64) -> Vec<Block> {
        self.blockchain
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .blocks_from(from, MAX_BLOCKS_PER_MESSAGE)
    }

    /// Height of the next block we expect.
    pub(crate) fn next_height(&self) -> u64 {
        self.height().map_or(0, |height| height.saturating_add(1))
    }

    pub(crate) fn add_block(&self, block: Block) -> Result<(), BlockchainError> {
        self.blockchain
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .add_block(block)
    }

    /// Returns `false` if the transaction was already in the mempool.
    pub(crate) fn add_transaction(&self, transaction: Transaction) -> Result<bool, NetworkError> {
        Ok(self
            .blockchain
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .add_transaction(transaction)?)
    }

    pub(crate) fn mempool_len(&self) -> usize {
        self.blockchain
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .mempool
            .len()
    }

    /// Sets the right nonce, adds the transaction to the mempool and sends it to every peer.
    pub(crate) async fn submit_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<Transaction, NetworkError> {
        let transaction = {
            let mut blockchain = self
                .blockchain
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let nonce = blockchain.next_nonce(&transaction);
            let transaction = transaction.with_nonce(nonce);
            blockchain.add_transaction(transaction.clone())?;
            transaction
        };

        self.broadcast_transaction(&transaction, None).await;
        Ok(transaction)
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

            if self.get_connections_number() >= MAX_PEERS {
                warn!(%peer_addr, "Max peers reached, refusing incoming connection");
                continue;
            }

            self.clone().spawn_connection(stream, peer_addr);
        }
    }

    pub(crate) async fn connect_to(self: Arc<Self>, addr: SocketAddr) -> Result<(), NetworkError> {
        if addr == self.listen_addr
            || self.is_connected(&addr)
            || self.ongoing_connections.contains_key(&addr)
            || self.get_connections_number() >= MAX_PEERS
            || !self.dialing.insert(addr)
        {
            return Ok(());
        }

        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await;
        let stream = match stream {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                self.dialing.remove(&addr);
                return Err(error.into());
            }
            Err(_) => {
                self.dialing.remove(&addr);
                return Err(NetworkError::ConnectTimeout(addr));
            }
        };

        // Registered in `ongoing_connections` before leaving `dialing`, so no gap.
        self.clone().spawn_connection(stream, addr);
        self.dialing.remove(&addr);

        self.converse(addr, MessageType::LetsConnect(self.chain_id))
            .await
    }

    fn spawn_connection(self: Arc<Self>, stream: TcpStream, socket_addr: SocketAddr) {
        let (reader, writer) = stream.into_split();
        let writer = Arc::new(Mutex::new(writer));
        self.ongoing_connections.insert(socket_addr, writer.clone());

        tokio::spawn(async move {
            let result = Message::handle(reader, socket_addr, self.clone()).await;

            self.unregister_connection(socket_addr, &writer);

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

            let Some(peer) = self.get_random_connection(None) else {
                continue;
            };

            if let Err(error) = self.converse(peer, MessageType::AskPeerAddr).await {
                warn!(%peer, %error, "Failed to ask for a peer address");
            }
        }
    }

    /// Pings every peer, and drops the ones that stayed silent for too long.
    pub(crate) async fn check_peers(self: Arc<Self>) {
        let mut interval = tokio::time::interval(PING_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let peers: Vec<SocketAddr> =
                self.connections.iter().map(|entry| *entry.key()).collect();

            for peer in peers {
                let silent = self
                    .last_seen
                    .get(&peer)
                    .is_none_or(|last_seen| last_seen.elapsed() > PEER_TIMEOUT);

                if silent {
                    self.drop_peer(peer).await;
                } else if let Err(error) = self.ping(peer).await {
                    warn!(%peer, %error, "Failed to ping peer");
                }
            }
        }
    }

    async fn drop_peer(&self, peer: SocketAddr) {
        self.last_seen.remove(&peer);
        self.peer_addrs
            .retain(|_, listen_addr| *listen_addr != peer);

        let Some((_, writer)) = self.connections.remove(&peer) else {
            return;
        };

        warn!(
            %peer,
            peers = self.get_connections_number(),
            "Peer timed out"
        );

        if let Err(error) = writer.lock().await.shutdown().await {
            debug!(%peer, %error, "Failed to close connection");
        }
    }

    pub(super) fn mark_seen(&self, socket_addr: SocketAddr) {
        let peer = self.peer_listen_addr(socket_addr);

        if self.is_connected(&peer) {
            self.last_seen.insert(peer, Instant::now());
        }
    }

    pub(crate) async fn send_block(
        &self,
        addr: SocketAddr,
        block: Block,
    ) -> Result<(), NetworkError> {
        self.converse(addr, MessageType::SendBlock(block)).await
    }

    /// Sends to every peer except `exclude` (usually the peer we received it from).
    pub(crate) async fn broadcast_block(&self, block: &Block, exclude: Option<SocketAddr>) {
        let peers: Vec<SocketAddr> = self
            .connections
            .iter()
            .map(|entry| *entry.key())
            .filter(|peer| Some(*peer) != exclude)
            .collect();

        for peer in peers {
            if let Err(error) = self.send_block(peer, block.clone()).await {
                warn!(%peer, %error, "Failed to send block");
            }
        }
    }

    pub(crate) async fn send_transaction(
        &self,
        addr: SocketAddr,
        transaction: Transaction,
    ) -> Result<(), NetworkError> {
        self.converse(addr, MessageType::SendTransaction(transaction))
            .await
    }

    /// Sends to every peer except `exclude` (usually the peer we received it from).
    pub(crate) async fn broadcast_transaction(
        &self,
        transaction: &Transaction,
        exclude: Option<SocketAddr>,
    ) {
        let peers: Vec<SocketAddr> = self
            .connections
            .iter()
            .map(|entry| *entry.key())
            .filter(|peer| Some(*peer) != exclude)
            .collect();

        for peer in peers {
            if let Err(error) = self.send_transaction(peer, transaction.clone()).await {
                warn!(%peer, %error, "Failed to send transaction");
            }
        }
    }

    pub(crate) async fn ask_height(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        self.converse(addr, MessageType::AskHeight).await
    }

    pub(crate) async fn ask_blocks(&self, addr: SocketAddr, from: u64) -> Result<(), NetworkError> {
        self.converse(addr, MessageType::AskBlocks { from }).await
    }

    pub(crate) async fn ping(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        self.converse(addr, MessageType::Ping).await
    }

    pub(crate) async fn disconnect(&self, addr: SocketAddr) -> Result<(), NetworkError> {
        self.converse(addr, MessageType::Disconnect).await?;

        let writer = self.get_writer(addr)?;
        writer.lock().await.shutdown().await?;

        Ok(())
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
        if listen_addr == self.listen_addr || self.is_connected(&listen_addr) {
            return Err(NetworkError::AlreadyConnected(listen_addr));
        }

        let (_, writer) = self
            .ongoing_connections
            .remove(&socket_addr)
            .ok_or(NetworkError::TcpStreamNotConnected(socket_addr))?;

        if socket_addr != listen_addr {
            self.peer_addrs.insert(socket_addr, listen_addr);
        }
        self.connections.insert(listen_addr, writer);
        self.last_seen.insert(listen_addr, Instant::now());

        info!(
            peer = %listen_addr,
            peers = self.get_connections_number(),
            "Peer connected"
        );

        Ok(())
    }

    /// Removes this connection only: if another connection to the same peer
    /// (different writer) is registered, it is left untouched.
    fn unregister_connection(&self, socket_addr: SocketAddr, writer: &Writer) {
        self.ongoing_connections
            .remove_if(&socket_addr, |_, w| Arc::ptr_eq(w, writer));

        let listen_addr = self
            .peer_addrs
            .remove(&socket_addr)
            .map(|(_, addr)| addr)
            .unwrap_or(socket_addr);

        if self
            .connections
            .remove_if(&listen_addr, |_, w| Arc::ptr_eq(w, writer))
            .is_some()
        {
            self.last_seen.remove(&listen_addr);

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

    pub(crate) fn get_connections_number(&self) -> usize {
        self.connections.len()
    }

    /// Picks a random registered peer, optionally excluding one address.
    pub(crate) fn get_random_connection(&self, exclude: Option<SocketAddr>) -> Option<SocketAddr> {
        let candidates: Vec<SocketAddr> = self
            .connections
            .iter()
            .map(|entry| *entry.key())
            .filter(|addr| Some(*addr) != exclude)
            .collect();

        if candidates.is_empty() {
            return None;
        }

        let index = rand::rng().random_range(0..candidates.len());
        Some(candidates[index])
    }

    fn is_connected(&self, addr: &SocketAddr) -> bool {
        self.connections.contains_key(addr)
    }
}
