use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use dashmap::{DashMap, Entry};
use uuid::Uuid;

use super::{State, StateError, Statettl};

pub(crate) struct StateHandler {
    map: Arc<DashMap<Uuid, Statettl>>,
}

impl Default for StateHandler {
    fn default() -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);

        let states: Arc<DashMap<Uuid, Statettl>> = Arc::new(DashMap::new());
        let worker_states = Arc::clone(&states);

        let _ = thread::spawn(move || {
            while worker_running.load(Ordering::Relaxed) {
                worker_states.retain(|_, entry| entry.is_still_valid());

                thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        Self { map: states }
    }
}

impl StateHandler {
    pub(crate) fn register_state(&self, id: Uuid, state: State) -> Result<(), StateError> {
        if state == State::None {
            return Ok(());
        }

        match self.map.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(Statettl::new(state));
                Ok(())
            }
            Entry::Occupied(_) => Err(StateError::Collision(id)),
        }
    }

    pub(crate) fn set_next_state(&self, id: Uuid) -> Result<(), StateError> {
        match self.map.entry(id) {
            Entry::Occupied(mut entry) => {
                if entry.get_mut().update().is_none() {
                    entry.remove();
                }

                Ok(())
            }
            Entry::Vacant(_) => Err(StateError::NotFound(id)),
        }
    }
}
