use std::time::{SystemTime, UNIX_EPOCH};

use crate::network::state::State;

pub(super) struct Statettl {
    timestamp: u64,
    state: State,
}

impl Statettl {
    pub(super) fn new(state: State) -> Self {
        Self {
            timestamp: Self::now(),
            state,
        }
    }

    pub(super) fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    pub(super) fn is_still_valid(&self) -> bool {
        Self::now() > self.timestamp + 60_000
    }

    pub(super) fn update(&mut self) -> Option<State> {
        if let Some(n) = self.state.next() {
            self.timestamp = Self::now();
            self.state = n;

            return Some(n);
        }

        None
    }
}
