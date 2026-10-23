use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("A state already exists for ID {0}")]
    Collision(Uuid),

    #[error("No state found for ID {0}")]
    NotFound(Uuid),
}
