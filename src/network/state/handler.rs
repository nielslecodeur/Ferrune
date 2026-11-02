use std::sync::Arc;

use dashmap::{DashMap, Entry};
use uuid::Uuid;

use super::{State, StateError, StateTtl};
use crate::consts::STATE_CLEANUP_INTERVAL;

pub(in crate::network) struct StateHandler {
    map: Arc<DashMap<Uuid, StateTtl>>,
}

impl Default for StateHandler {
    /// Must be called from within a Tokio runtime: it spawns a task that purges
    /// expired conversations. The task stops once the handler is dropped.
    fn default() -> Self {
        let map: Arc<DashMap<Uuid, StateTtl>> = Arc::new(DashMap::new());
        let weak_map = Arc::downgrade(&map);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(STATE_CLEANUP_INTERVAL);

            loop {
                interval.tick().await;

                let Some(map) = weak_map.upgrade() else {
                    break;
                };
                map.retain(|_, entry| entry.is_still_valid());
            }
        });

        Self { map }
    }
}

impl StateHandler {
    pub(in crate::network) fn get_state(&self, id: Uuid) -> Option<State> {
        self.map.get(&id).map(|entry| entry.state())
    }

    pub(in crate::network) fn register_state(
        &self,
        id: Uuid,
        state: State,
    ) -> Result<(), StateError> {
        if state == State::None {
            return Ok(());
        }

        match self.map.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(StateTtl::new(state));
                Ok(())
            }
            Entry::Occupied(_) => Err(StateError::Collision(id)),
        }
    }

    pub(in crate::network) fn set_next_state(&self, id: Uuid) -> Result<(), StateError> {
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
