use futures::task::noop_waker;
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    error::Error,
    fmt, mem,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use super::{
    super::{mpsc, remote},
    BroadcastMsg, Receiver,
};
use crate::{chmux, codec::CodecT, rsync::RemoteSend};

/// An error occured during sending over a broadcast channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError<T> {
    /// All receivers have been dropped.
    Closed(T),
    /// Sending to a remote endpoint failed.
    RemoteSend(remote::SendErrorKind),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening to a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed(_) => write!(f, "no subscribers"),
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl<T> Error for SendError<T> where T: fmt::Debug {}

impl<T, R> TryFrom<mpsc::TrySendError<T>> for SendError<R> {
    type Error = mpsc::TrySendError<T>;

    fn try_from(err: mpsc::TrySendError<T>) -> Result<Self, Self::Error> {
        match err {
            mpsc::TrySendError::RemoteSend(err) => Ok(Self::RemoteSend(err)),
            mpsc::TrySendError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            mpsc::TrySendError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
            mpsc::TrySendError::RemoteForward => Ok(Self::RemoteForward),
            other => Err(other),
        }
    }
}

/// Sending-half of the broadcast channel.
///
/// Cannot be sent over a remote channel.
/// Use [feeder](Self::feeder) to obtain an mpsc sender that feeds this
/// broadcast sender and can be sent over a remote channel.
#[derive(Clone)]
pub struct Sender<T, Codec> {
    inner: Arc<Mutex<SenderInner<T, Codec>>>,
}

struct SenderInner<T, Codec> {
    subs: Vec<mpsc::Sender<BroadcastMsg<T>, Codec, 1>>,
    ready_tx: tokio::sync::mpsc::UnboundedSender<mpsc::Sender<BroadcastMsg<T>, Codec, 1>>,
    ready_rx: tokio::sync::mpsc::UnboundedReceiver<mpsc::Sender<BroadcastMsg<T>, Codec, 1>>,
    not_ready: usize,
}

impl<T, Codec> fmt::Debug for Sender<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T, Codec> Sender<T, Codec>
where
    T: RemoteSend + Clone,
    Codec: CodecT,
{
    /// Creates a new sender.
    pub(crate) fn new() -> Self {
        let (ready_tx, ready_rx) = tokio::sync::mpsc::unbounded_channel();
        let inner = SenderInner { subs: Vec::new(), ready_tx, ready_rx, not_ready: 0 };
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    /// Attempts to send a value to all active receivers.
    ///
    /// No back-pressure is provided.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut inner = self.inner.lock().unwrap();

        // Fetch subscribers that have become ready again.
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        while let Poll::Ready(Some(sub)) = inner.ready_rx.poll_recv(&mut cx) {
            inner.subs.push(sub);
            inner.not_ready -= 1;
        }

        let mut keep = Vec::new();
        let mut last_err = None;

        // Broadcast value to all subscribers that are ready.
        let subs = mem::take(&mut inner.subs);
        for sub in subs {
            match sub.try_send(BroadcastMsg::Value(value.clone())) {
                Ok(()) => keep.push(sub),
                Err(mpsc::TrySendError::Full(BroadcastMsg::Value(_))) => {
                    // Spawn task that waits for subscriber to become ready again,
                    // then add it back to subscriber list.
                    let ready_tx = inner.ready_tx.clone();
                    tokio::spawn(async move {
                        let _ = sub.send(BroadcastMsg::Lagged).await;
                        // Make sure subscriber has space for next message.
                        let _permit = sub.reserve().await;
                        let _ = ready_tx.send(sub);
                    });
                    inner.not_ready += 1;
                }
                Err(mpsc::TrySendError::Closed(_)) => (),
                Err(err) => last_err = Some(err),
            }
        }
        inner.subs = keep;

        // Return detailed error if last subscriber was disconnected because of error.
        if !(inner.subs.is_empty() && inner.not_ready == 0) {
            Ok(())
        } else {
            match last_err {
                Some(err) => match err.try_into() {
                    Ok(err) => Err(err),
                    Err(_) => unreachable!("unconvertable error"),
                },
                None => Err(SendError::Closed(value)),
            }
        }
    }

    /// Creates a new receiver that will receive values sent after this call to subscribe.
    pub fn subscribe<const RECV_BUFFER: usize>(&self, send_buffer: usize) -> Receiver<T, Codec, RECV_BUFFER> {
        let mut inner = self.inner.lock().unwrap();

        let (tx, rx) = mpsc::channel(send_buffer);
        inner.subs.push(tx);
        Receiver::new(rx)
    }

    /// Creates an mpsc sender that feeds values to this broadcast sender.
    ///
    /// The mpsc sender can be sent over a remote channel.
    /// All feeders are disconnected once all receivers are disconnected.
    pub fn feeder<const SEND_BUFFER: usize>(&self) -> mpsc::Sender<T, Codec, SEND_BUFFER> {
        let (tx, mut rx): (_, mpsc::Receiver<_, _, 1>) = mpsc::channel(1);
        let this = self.clone();

        tokio::spawn(async move {
            while let Ok(Some(value)) = rx.recv().await {
                if this.send(value).is_err() {
                    break;
                }
            }
        });

        tx
    }

    /// Returns the number of active receivers.
    pub fn receiver_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();

        inner.subs.len() + inner.not_ready
    }
}

impl<T, Codec> Drop for Sender<T, Codec> {
    fn drop(&mut self) {
        // empty
    }
}
