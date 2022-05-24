/// Interlocks sender and receiver against both being sent.
pub(crate) struct Interlock {
    pub sender: Location,
    pub receiver: Location,
}

/// Location of a sender or receiver.
pub(crate) enum Location {
    Local,
    Sending(tokio::sync::oneshot::Receiver<()>),
    Remote,
}

impl Location {
    /// True if location is local.
    pub fn check_local(&mut self) -> bool {
        match self {
            Self::Local => true,
            Self::Sending(rx) => match rx.try_recv() {
                Ok(()) => {
                    *self = Self::Remote;
                    false
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    *self = Self::Local;
                    true
                }
            },
            Self::Remote => false,
        }
    }

    // Start sending and return confirmation channel.
    pub fn start_send(&mut self) -> tokio::sync::oneshot::Sender<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        *self = Self::Sending(rx);
        tx
    }
}
