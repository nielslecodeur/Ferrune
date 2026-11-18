//! Ferrune: a small proof-of-stake blockchain with its own peer-to-peer network.
//!
//! The binary (`src/main.rs`) starts a node. The library is also used by the
//! benchmarks in `benches/`.

pub mod blockchain;
pub mod consts;
pub mod network;
pub mod node;
