#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    None,

    StartingConnection,
    WaitingForChainId,
    WaitingForRegistration,
}

impl State {
    pub(crate) fn next(&self) -> Option<Self> {
        match self {
            State::None => None,
            State::StartingConnection => Some(Self::WaitingForChainId),
            State::WaitingForChainId => Some(State::WaitingForRegistration),
            State::WaitingForRegistration => None,
        }
    }

    pub(crate) fn is_conversation_start(&self) -> bool {
        match self {
            State::StartingConnection => true,
            State::None | State::WaitingForChainId | State::WaitingForRegistration => false,
        }
    }
}
