//! Messages exchanged between read/write locks and the owner.

use serde::{Deserialize, Serialize};

use super::super::{mpsc, oneshot, watch, RemoteSend};
use crate::codec::CodecT;

/// A read request from a lock to the owner.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: CodecT"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: CodecT"))]
pub struct ReadRequest<T, Codec> {
    /// Channel for sending the value.
    pub(crate) value_tx: oneshot::Sender<Value<T, Codec>, Codec>,
}

/// A write request from a lock to the owner.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: CodecT"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: CodecT"))]
pub struct WriteRequest<T, Codec> {
    /// Channel for sending current value.
    pub(crate) value_tx: oneshot::Sender<T, Codec>,
    /// Channel for receiving modified value.
    pub(crate) new_value_rx: oneshot::Receiver<T, Codec>,
    /// Channel for confirming that modified value has been stored.
    pub(crate) confirm_tx: oneshot::Sender<(), Codec>,
}

/// A value together with invalidation channels.
#[derive(Clone, Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: CodecT"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: CodecT"))]
pub struct Value<T, Codec> {
    /// The shared value.
    pub(crate) value: T,
    /// Notification channel that all instances of this value have been dropped.
    pub(crate) dropped_tx: mpsc::Sender<(), Codec, 1>,
    /// Notification channel that value has been invalidated by the owner.
    pub(crate) invalid_rx: watch::Receiver<bool, Codec>,
}

impl<T, Codec> Value<T, Codec>
where
    T: RemoteSend,
    Codec: CodecT,
{
    /// True, if value is valid.
    pub(crate) fn is_valid(&self) -> bool {
        if self.dropped_tx.is_closed() {
            return false;
        }

        match self.invalid_rx.borrow() {
            Ok(invalid) if !*invalid => (),
            _ => return false,
        }

        true
    }
}
