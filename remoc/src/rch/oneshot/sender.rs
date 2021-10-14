use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use super::super::{buffer, mpsc};
use crate::{
    codec::{self},
    RemoteSend,
};

/// An error occured during sending over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError<T> {
    /// The remote end closed the channel.
    Closed(T),
    /// Communication with the remote endpoint failed.
    Failed,
}

impl<T> SendError<T> {
    /// True, if remote endpoint has closed the channel.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed(_) => write!(f, "channel is closed"),
            Self::Failed => write!(f, "send error"),
        }
    }
}

impl<T> From<mpsc::TrySendError<T>> for SendError<T> {
    fn from(err: mpsc::TrySendError<T>) -> Self {
        match err {
            mpsc::TrySendError::Closed(err) => Self::Closed(err),
            _ => Self::Failed,
        }
    }
}

impl<T> Error for SendError<T> where T: fmt::Debug {}

/// Sends a value to the associated receiver.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: codec::Codec"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: codec::Codec"))]
pub struct Sender<T, Codec = codec::Default>(pub(crate) mpsc::Sender<T, Codec, buffer::Custom<1>>);

impl<T, Codec> fmt::Debug for Sender<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T, Codec> Sender<T, Codec>
where
    T: RemoteSend,
    Codec: codec::Codec,
{
    /// Sends a value over this channel.
    pub fn send(self, value: T) -> Result<(), SendError<T>> {
        self.0.try_send(value).map_err(|err| err.into())
    }

    /// Completes when the receiver has been closed or dropped.
    pub async fn closed(&self) {
        self.0.closed().await
    }

    /// Returns whether the receiver has been closed or dropped.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}
