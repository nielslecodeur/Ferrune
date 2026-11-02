use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

pub(crate) const MAX_PEERS: usize = 8;
pub(crate) const DISCOVERY_INTERVAL: Duration = Duration::from_secs(60);
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const STATE_CLEANUP_INTERVAL: Duration = Duration::from_secs(1);
pub(crate) const CONVERSATION_TTL: Duration = Duration::from_secs(60);
pub(crate) const MAX_BLOCKS_PER_MESSAGE: usize = 100;
pub(crate) const MINT_AUTHORITY: &str = "admin";
pub(crate) const MAIN_CHAIN_ID: u8 = 0;
pub(crate) const DEFAULT_BOOTSTRAP: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8000));
