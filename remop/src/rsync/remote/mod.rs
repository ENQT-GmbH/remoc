//! Wrapper around a chmux port that allows sending and receiving of values containing ports.

mod io;
mod receiver;
mod sender;

pub use receiver::{PortDeserializer, Receiver, RecvError};
pub use sender::{PortSerializer, SendError, SendErrorKind, Sender};

/// Chunk queue length for big data (de-)serialization.
const BIG_DATA_CHUNK_QUEUE: usize = 32;

/// Limit for counting big data instances.
const BIG_DATA_LIMIT: i8 = 16;
