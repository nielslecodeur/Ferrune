#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    None,

    WaitingForChainId,
    WaitingForRegistration,
}

impl State {
    pub(crate) fn next(&self) -> Option<Self> {
        match self {
            State::None => None,
            State::WaitingForChainId => Some(State::WaitingForRegistration),
            State::WaitingForRegistration => None,
        }
    }
}
