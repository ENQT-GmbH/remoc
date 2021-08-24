// Only difference here will be that we wrap the channels into remote sender / remote receiver.
// And maybe wrap the methods?

// Yes => should be done so here.
// But why?
// We could avoid that if we publish the remote sender.
// But then this channel is less user friendly.

use crate::interlock::{Interlock, Location};

#[derive(Debug, Clone)]
pub enum ConnectError {
    /// The corresponding sender or receiver has been dropped.
    Dropped,
    /// Error initiating chmux connection.
    Connect(chmux::ConnectError),
    /// Error accepting chmux connection.
    Accept(chmux::ListenerError),
}

//mod receiver;
//mod sender;
