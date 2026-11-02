use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

pub(crate) const MAX_PEERS: usize = 8;
pub(crate) const DISCOVERY_INTERVAL: Duration = Duration::from_secs(60);
pub(crate) const CONVERSATION_TTL: Duration = Duration::from_secs(60);
pub(crate) const MAIN_CHAIN_ID: u8 = 0;
pub(crate) const DEFAULT_BOOTSTRAP: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8000));
