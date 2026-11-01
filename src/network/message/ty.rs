use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    LetsConnect,
    ChainId(u8),
    Register,
}
