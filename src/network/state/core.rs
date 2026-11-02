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

    SendingTransaction,
    WaitingForTransactionConfirmation,

    AskingHeight,
    WaitingForHeight,

    AskingBlocks,
    WaitingForBlocks,

    Pinging,
    WaitingForPong,

    Disconnecting,
}

impl State {
    pub(super) fn next(&self) -> Option<Self> {
        match self {
            State::StartingConnection => Some(Self::WaitingForChainId),
            State::WaitingForChainId => Some(State::WaitingForRegistration),
            State::WaitingForRegistration => Some(State::WaitingForConfirmation),
            State::AskingPeerAddress => Some(State::WaitingForPeerAddress),
            State::SendingBlock => Some(State::WaitingForBlockConfirmation),
            State::SendingTransaction => Some(State::WaitingForTransactionConfirmation),
            State::AskingHeight => Some(State::WaitingForHeight),
            State::AskingBlocks => Some(State::WaitingForBlocks),
            State::Pinging => Some(State::WaitingForPong),
            State::None
            | State::WaitingForConfirmation
            | State::WaitingForPeerAddress
            | State::WaitingForBlockConfirmation
            | State::WaitingForTransactionConfirmation
            | State::WaitingForHeight
            | State::WaitingForBlocks
            | State::WaitingForPong
            | State::Disconnecting => None,
        }
    }

    pub(in crate::network) fn is_conversation_start(&self) -> bool {
        match self {
            State::StartingConnection
            | State::AskingPeerAddress
            | State::SendingBlock
            | State::SendingTransaction
            | State::AskingHeight
            | State::AskingBlocks
            | State::Pinging
            | State::Disconnecting => true,
            State::None
            | State::WaitingForChainId
            | State::WaitingForRegistration
            | State::WaitingForConfirmation
            | State::WaitingForPeerAddress
            | State::WaitingForBlockConfirmation
            | State::WaitingForTransactionConfirmation
            | State::WaitingForHeight
            | State::WaitingForBlocks
            | State::WaitingForPong => false,
        }
    }
}
