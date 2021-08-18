use crate::{
    base_sender::{BaseMsg, BaseSender},
    codec::{CodecT, DeserializationError, SerializationError},
    remote_receiver::{PortDeserializer, ReceiveError, RemoteReceiver},
    remote_sender::{ObtainSenderError, PortSerializer, RemoteSender, SendError},
    Obtainer,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    error::Error,
    marker::PhantomData,
    mem,
    rc::{Rc, Weak},
    sync::Arc,
};

use chmux::{Connect, DataBuf, ListenerError, PortAllocator, PortNumber, Receiver, Received, Request};
use futures::{future::BoxFuture, Future, FutureExt};
use serde::{de::DeserializeOwned, ser, Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};

pub enum ReceiverSource<T, Codec> {
    Local(tokio::sync::mpsc::Receiver<T>),
    Remote(RemoteReceiver<T, Codec>),
}

impl<T, Codec> ReceiverSource<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    pub async fn recv(&mut self) -> Result<Option<T>, ReceiveError> {
        let item = match self {
            Self::Local(r) => r.recv().await,
            Self::Remote(r) => r.recv().await?,
        };
        Ok(item)
    }

    pub async fn close(&mut self) {
        match self {
            Self::Local(r) => r.close(),
            Self::Remote(r) => r.close().await,
        }
    }
}

pub struct BaseReceiver<T, Codec> {
    rx: ReceiverSource<T, Codec>,
    source_rx: tokio::sync::mpsc::UnboundedReceiver<tokio::sync::oneshot::Receiver<ReceiverSource<T, Codec>>>,
}

// hmm, add a mutex with closed state?
// or make it a oneshot receiver that can be closed
// this way no new receiver can be received.
// but this is again problematic

// so we have no indication when source change can end
// we could make the serializer keep the close loop alive

impl<T, Codec> BaseReceiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Receives an item.
    pub async fn recv(&mut self) -> Result<Option<T>, ReceiveError> {
        loop {
            match self.rx.recv().await? {
                Some(item) => return Ok(Some(item)),
                None => match self.source_rx.recv().await {
                    Some(source_rx) => {
                        if let Ok(source) = source_rx.await {
                            self.rx = source
                        }
                    }
                    None => return Ok(None),
                },
            }
        }
    }

    /// Closes the channel, so that no more items can be sent.
    pub async fn close(&mut self) {
        // TODO: verify that closing a chmux channel returns after it has been closed.
        self.rx.close().await;

        let (source_tx, mut source_rx) = tokio::sync::mpsc::unbounded_channel();
        mem::swap(&mut source_rx, &mut self.source_rx);
        source_rx.close();

        tokio::spawn(async move {
            while let Some(rx) = source_rx.recv().await {
                if let Ok(mut source) = rx.await {
                    source.close().await;

                    let (tx, rx) = oneshot::channel();
                    tx.send(source);
                    source_tx.send(rx);
                }
            }
        }).await;
    }
}

impl<T, Codec> Serialize for BaseReceiver<T, Codec>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.rx {
            // Yes, we would have to process source_rx.
            // And even then one may be in transit.
            // So it would have to be similar to close?
            // Question is what to send...
            // Maybe a lock is not avoidable
            // So do we have a possible solution here?
            // All received data would have to be forwarded.
            // And then after forwarding, the send target can be switched.
            // But the question is if we really need that?
            // Probably not
        }
    }
}

pub fn base_channel<T, Codec>(buffer: usize) -> (BaseSender<T, Codec>, BaseReceiver<T, Codec>)
where
    T: Serialize + DeserializeOwned + 'static,
{
    //    let (sender, receiver) = mpsc::channel(buffer);
    //    let base_sender = BaseSender { sender, _phantom: PhantomData };
    //    let base_receiver = BaseReceiver { receiver, _phantom: PhantomData };
    //    (base_sender, base_receiver)
    todo!()
}
