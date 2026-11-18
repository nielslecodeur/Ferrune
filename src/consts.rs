use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

pub const MAX_PEERS: usize = 8;
pub const DISCOVERY_INTERVAL: Duration = Duration::from_secs(60);
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
pub const PEER_TIMEOUT: Duration = Duration::from_secs(30);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const STATE_CLEANUP_INTERVAL: Duration = Duration::from_secs(1);
pub const CONVERSATION_TTL: Duration = Duration::from_secs(60);
pub const MAX_BLOCKS_PER_MESSAGE: usize = 100;
pub const MAX_TRANSACTIONS_PER_BLOCK: usize = 100;
pub const BLOCK_INTERVAL: Duration = Duration::from_secs(5);
pub const MAIN_CHAIN_ID: u8 = 0;
pub const DEFAULT_BOOTSTRAP: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8000));
