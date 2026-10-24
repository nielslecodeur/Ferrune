use thiserror::Error;
use uuid::Uuid;

use crate::State;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("A state already exists for ID {0}")]
    Collision(Uuid),

    #[error("No state found for ID {0}")]
    NotFound(Uuid),

    #[error("Conversation not started for ID {0}")]
    ConversationNotStarted(Uuid),

    #[error("Unexpected state for ID {id}: expected {expected:?}, received {received:?}")]
    UnexpectedState {
        id: Uuid,
        expected: State,
        received: State,
    },
}
