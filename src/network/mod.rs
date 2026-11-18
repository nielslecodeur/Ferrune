mod core;
mod error;
pub(crate) mod message;
mod state;

pub use core::Network;

pub use error::NetworkError;
use state::{State, StateError};

#[cfg(test)]
mod tests;
