#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    None,

    StartingConnection,
    WaitingForChainId,
    WaitingForRegistration,

    WaitingForPeerAddress,
}

impl State {
    pub(crate) fn next(&self) -> Option<Self> {
        match self {
            State::StartingConnection => Some(Self::WaitingForChainId),
            State::WaitingForChainId => Some(State::WaitingForRegistration),
            State::None | State::WaitingForRegistration | State::WaitingForPeerAddress => None,
        }
    }

    pub(crate) fn is_conversation_start(&self) -> bool {
        match self {
            State::StartingConnection | State::WaitingForPeerAddress => true,
            State::None | State::WaitingForChainId | State::WaitingForRegistration => false,
        }
    }
}
