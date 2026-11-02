#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    None,

    StartingConnection,
    WaitingForChainId,
    WaitingForRegistration,

    AskingPeerAddress,
    WaitingForPeerAddress,
}

impl State {
    pub(super) fn next(&self) -> Option<Self> {
        match self {
            State::StartingConnection => Some(Self::WaitingForChainId),
            State::WaitingForChainId => Some(State::WaitingForRegistration),
            State::AskingPeerAddress => Some(State::WaitingForPeerAddress),
            State::None | State::WaitingForRegistration | State::WaitingForPeerAddress => None,
        }
    }

    pub(in crate::network) fn is_conversation_start(&self) -> bool {
        match self {
            State::StartingConnection | State::AskingPeerAddress => true,
            State::None
            | State::WaitingForChainId
            | State::WaitingForRegistration
            | State::WaitingForPeerAddress => false,
        }
    }
}
