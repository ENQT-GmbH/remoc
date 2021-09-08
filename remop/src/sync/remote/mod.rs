mod io;
mod receiver;
mod sender;

pub(crate) use receiver::{PortDeserializer, ReceiveError, Receiver};
pub(crate) use sender::{PortSerializer, SendError, SendErrorKind, Sender};

/// Chunk queue length for big data (de-)serialization.
const BIG_DATA_CHUNK_QUEUE: usize = 32;

/// Limit for counting big data instances.
const BIG_DATA_LIMIT: i8 = 16;
