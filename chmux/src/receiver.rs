use bytes::{Buf, Bytes, BytesMut};
use futures::{
    ready,
    stream::Stream,
    task::{Context, Poll},
};
use std::{collections::VecDeque, error::Error, fmt, mem, pin::Pin};
use tokio::sync::{mpsc, oneshot};

use crate::{
    credit::{ChannelCreditReturner, UsedCredit},
    multiplexer::PortEvt,
    Request,
};

/// An error occured during receiving a message.
#[derive(Debug)]
pub enum ReceiveError {
    /// Multiplexer terminated.
    Multiplexer,
    /// Data exceeds maximum size.
    ExceedsMaxDataSize {
        /// Actual data size or below.
        data_size: usize,
        /// Maximum allowed data size.
        max_size: usize,
    },
}

impl ReceiveError {
    /// Returns true, if error is due to multiplexer being terminated.
    pub fn is_terminated(&self) -> bool {
        match self {
            Self::Multiplexer => true,
            Self::ExceedsMaxDataSize { .. } => false,
        }
    }
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Multiplexer => write!(f, "multiplexer terminated"),
            Self::ExceedsMaxDataSize { data_size, max_size } => {
                write!(f, "data (at least {} bytes) exceeds maximum allowed size ({} bytes)", data_size, max_size)
            }
        }
    }
}

impl Error for ReceiveError {}

/// Container for received data.
pub(crate) struct ReceivedData {
    /// Received data.
    pub buf: Bytes,
    /// First chunk of data.
    pub first: bool,
    /// Last part of data.
    pub last: bool,
    /// Flow-control credit.
    pub credit: UsedCredit,
}

/// Container for received port open requests.
pub(crate) struct ReceivedPortRequests {
    /// Port open requests.
    pub requests: Vec<Request>,
    /// Flow-control credit.
    pub credit: UsedCredit,
}

/// Port receive message.
pub(crate) enum PortReceiveMsg {
    /// Data has been received.
    Data(ReceivedData),
    /// Ports have been received.
    PortRequests(ReceivedPortRequests),
    /// Sender has closed its end.
    Finished,
}

/// A buffer containing received data.
#[derive(Clone)]
pub struct DataBuf {
    bufs: VecDeque<Bytes>,
    remaining: usize,
}

impl fmt::Debug for DataBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DataBuf").field("remaining", &self.remaining).finish_non_exhaustive()
    }
}

impl DataBuf {
    fn new() -> Self {
        Self { bufs: VecDeque::new(), remaining: 0 }
    }

    fn push(&mut self, buf: Bytes, max_size: usize) -> Result<(), ReceiveError> {
        match self.remaining.checked_add(buf.len()) {
            Some(new_size) if new_size <= max_size => {
                self.bufs.push_back(buf);
                self.remaining = new_size;
                Ok(())
            }
            Some(new_size) => Err(ReceiveError::ExceedsMaxDataSize { data_size: new_size, max_size }),
            None => Err(ReceiveError::ExceedsMaxDataSize { data_size: usize::MAX, max_size }),
        }
    }
}

impl Default for DataBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl Buf for DataBuf {
    fn remaining(&self) -> usize {
        self.remaining
    }

    fn chunk(&self) -> &[u8] {
        match self.bufs.front() {
            Some(buf) => buf.chunk(),
            None => &[],
        }
    }

    fn advance(&mut self, mut cnt: usize) {
        while cnt > 0 {
            match self.bufs.front_mut() {
                Some(buf) if buf.len() > cnt => {
                    self.remaining -= cnt;
                    buf.advance(cnt);
                    cnt = 0;
                }
                Some(buf) => {
                    self.remaining -= buf.len();
                    cnt -= buf.len();
                    self.bufs.pop_front();
                }
                None => {
                    panic!("cannot advance beyond end of data");
                }
            }
        }
    }
}

impl From<DataBuf> for BytesMut {
    fn from(mut data: DataBuf) -> Self {
        let mut continuous = BytesMut::with_capacity(data.remaining);
        while let Some(buf) = data.bufs.pop_front() {
            continuous.extend_from_slice(&buf);
        }
        continuous
    }
}

impl From<DataBuf> for Bytes {
    fn from(mut data: DataBuf) -> Self {
        if data.bufs.len() == 1 {
            data.bufs.pop_front().unwrap()
        } else {
            BytesMut::from(data).into()
        }
    }
}

impl From<DataBuf> for Vec<u8> {
    fn from(mut data: DataBuf) -> Self {
        let mut continuous = Vec::with_capacity(data.remaining);
        while let Some(buf) = data.bufs.pop_front() {
            continuous.extend_from_slice(&buf);
        }
        continuous
    }
}

impl From<Bytes> for DataBuf {
    fn from(data: Bytes) -> Self {
        let remaining = data.len();
        let mut bufs = VecDeque::new();
        bufs.push_back(data);
        Self { bufs, remaining }
    }
}

/// Received entity.
pub enum Received {
    /// Binary data.
    Data(DataBuf),
    /// Port open requests.
    Requests(Vec<Request>),
}

/// Receives byte data over a channel.
pub struct Receiver {
    local_port: u32,
    remote_port: u32,
    max_data_size: usize,
    tx: mpsc::Sender<PortEvt>,
    rx: mpsc::UnboundedReceiver<PortReceiveMsg>,
    data_buf: DataBuf,
    credits: ChannelCreditReturner,
    closed: bool,
    finished: bool,
    _drop_tx: oneshot::Sender<()>,
}

impl fmt::Debug for Receiver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver")
            .field("local_port", &self.local_port)
            .field("remote_port", &self.remote_port)
            .field("closed", &self.closed)
            .field("finished", &self.finished)
            .finish()
    }
}

impl Receiver {
    pub(crate) fn new(
        local_port: u32, remote_port: u32, max_data_size: usize, tx: mpsc::Sender<PortEvt>,
        rx: mpsc::UnboundedReceiver<PortReceiveMsg>, credits: ChannelCreditReturner,
    ) -> Self {
        let (_drop_tx, drop_rx) = oneshot::channel();
        let tx_drop = tx.clone();
        tokio::spawn(async move {
            let _ = drop_rx.await;
            let _ = tx_drop.send(PortEvt::ReceiverDropped { local_port }).await;
        });

        Self {
            local_port,
            remote_port,
            max_data_size,
            tx,
            rx,
            data_buf: DataBuf::new(),
            credits,
            closed: false,
            finished: false,
            _drop_tx,
        }
    }

    /// The local port number.
    pub fn local_port(&self) -> u32 {
        self.local_port
    }

    /// The remote port number.
    pub fn remote_port(&self) -> u32 {
        self.remote_port
    }

    /// Receives data or ports over the channel.
    ///
    /// Waits for data to become available.
    pub async fn recv_any(&mut self) -> Result<Option<Received>, ReceiveError> {
        if self.finished {
            return Ok(None);
        }

        loop {
            self.credits.return_flush().await;

            let data = match self.rx.recv().await {
                Some(PortReceiveMsg::Data(data)) => data,
                Some(PortReceiveMsg::PortRequests(req)) => {
                    self.data_buf = DataBuf::new();
                    self.credits.start_return_one(req.credit, self.remote_port, &self.tx);
                    return Ok(Some(Received::Requests(req.requests)));
                }
                Some(PortReceiveMsg::Finished) => {
                    self.finished = true;
                    return Ok(None);
                }
                None => return Err(ReceiveError::Multiplexer),
            };

            self.credits.start_return_one(data.credit, self.remote_port, &self.tx);

            if data.first {
                self.data_buf = DataBuf::new();
            }

            if let Err(err) = self.data_buf.push(data.buf, self.max_data_size) {
                self.finished = true;
                self.data_buf = DataBuf::new();
                return Err(err);
            }

            if data.last {
                return Ok(Some(Received::Data(mem::take(&mut self.data_buf))));
            }
        }
    }

    /// Receives data over the channel.
    ///
    /// Waits for data to become available.
    /// Received port open requests are silently rejected.
    pub async fn recv(&mut self) -> Result<Option<DataBuf>, ReceiveError> {
        loop {
            match self.recv_any().await? {
                Some(Received::Data(data)) => break Ok(Some(data)),
                Some(Received::Requests(_)) => (),
                None => break Ok(None),
            }
        }
    }

    /// Polls to receive data or ports on this channel.
    pub fn poll_recv_any(&mut self, cx: &mut Context) -> Poll<Result<Option<Received>, ReceiveError>> {
        if self.finished {
            return Poll::Ready(Ok(None));
        }

        loop {
            ready!(self.credits.poll_return_flush(cx));

            let data = match ready!(self.rx.poll_recv(cx)) {
                Some(PortReceiveMsg::Data(data)) => data,
                Some(PortReceiveMsg::PortRequests(req)) => {
                    self.data_buf = DataBuf::new();
                    self.credits.start_return_one(req.credit, self.remote_port, &self.tx);
                    return Poll::Ready(Ok(Some(Received::Requests(req.requests))));
                }
                Some(PortReceiveMsg::Finished) => {
                    self.finished = true;
                    return Poll::Ready(Ok(None));
                }
                None => return Poll::Ready(Err(ReceiveError::Multiplexer)),
            };

            self.credits.start_return_one(data.credit, self.remote_port, &self.tx);

            if data.first {
                self.data_buf = DataBuf::new();
            }

            if let Err(err) = self.data_buf.push(data.buf, self.max_data_size) {
                self.finished = true;
                self.data_buf = DataBuf::new();
                return Poll::Ready(Err(err));
            }

            if data.last {
                return Poll::Ready(Ok(Some(Received::Data(mem::take(&mut self.data_buf)))));
            }
        }
    }

    /// Polls to receive on this channel.
    ///
    /// Received port open requests are silently rejected.
    pub fn poll_recv(&mut self, cx: &mut Context) -> Poll<Result<Option<DataBuf>, ReceiveError>> {
        loop {
            match ready!(self.poll_recv_any(cx))? {
                Some(Received::Data(data)) => break Poll::Ready(Ok(Some(data))),
                Some(Received::Requests(_)) => (),
                None => break Poll::Ready(Ok(None)),
            }
        }
    }

    /// Closes the sender at the remote endpoint, preventing it from sending new data.
    /// Already sent message will still be received.
    pub async fn close(&mut self) {
        if !self.closed {
            let _ = self.tx.send(PortEvt::ReceiverClosed { local_port: self.local_port }).await;
            self.closed = true;
        }
    }

    /// Convert this into a stream.
    pub fn into_stream(self) -> ReceiverStream {
        ReceiverStream(self)
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        // required for correct drop order
    }
}

/// A stream receiving byte data over a channel.
pub struct ReceiverStream(Receiver);

impl ReceiverStream {
    /// Closes the sender at the remote endpoint, preventing it from sending new data.
    /// Already sent message will still be received.
    pub async fn close(&mut self) {
        self.0.close().await
    }
}

impl Stream for ReceiverStream {
    type Item = Result<DataBuf, ReceiveError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        Poll::Ready(ready!(Pin::into_inner(self).0.poll_recv(cx)).transpose())
    }
}
