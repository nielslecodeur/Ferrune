#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    None,

    StartingConnection,
    WaitingForChainId,
    WaitingForRegistration,
    WaitingForConfirmation,

    AskingPeerAddress,
    WaitingForPeerAddress,

    SendingBlock,
    WaitingForBlockConfirmation,
}

impl State {
    pub(super) fn next(&self) -> Option<Self> {
        match self {
            State::StartingConnection => Some(Self::WaitingForChainId),
            State::WaitingForChainId => Some(State::WaitingForRegistration),
            State::WaitingForRegistration => Some(State::WaitingForConfirmation),
            State::AskingPeerAddress => Some(State::WaitingForPeerAddress),
            State::SendingBlock => Some(State::WaitingForBlockConfirmation),
            State::None
            | State::WaitingForConfirmation
            | State::WaitingForPeerAddress
            | State::WaitingForBlockConfirmation => None,
        }
    }

    pub(in crate::network) fn is_conversation_start(&self) -> bool {
        match self {
            State::StartingConnection | State::AskingPeerAddress | State::SendingBlock => true,
            State::None
            | State::WaitingForChainId
            | State::WaitingForRegistration
            | State::WaitingForConfirmation
            | State::WaitingForPeerAddress
            | State::WaitingForBlockConfirmation => false,
        }
    }
}
