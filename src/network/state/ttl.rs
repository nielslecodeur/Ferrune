use std::time::Instant;

use crate::consts::CONVERSATION_TTL;
use crate::network::state::State;

pub(super) struct StateTtl {
    last_update: Instant,
    state: State,
}

impl StateTtl {
    pub(super) fn new(state: State) -> Self {
        Self {
            last_update: Instant::now(),
            state,
        }
    }

    pub(super) fn is_still_valid(&self) -> bool {
        self.last_update.elapsed() <= CONVERSATION_TTL
    }

    pub(super) fn update(&mut self) -> Option<State> {
        let next = self.state.next()?;

        self.last_update = Instant::now();
        self.state = next;

        Some(next)
    }

    pub(super) fn state(&self) -> State {
        self.state
    }
}
