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

/// An error occured during receiving a data message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ReceiveError {
    /// Multiplexer terminated.
    Multiplexer,
    /// Data exceeds maximum size.
    ExceedsMaxDataSize(usize),
    /// Received ports exceed maximum count.
    ExceedsMaxPortCount(usize),
}

impl ReceiveError {
    /// Returns true, if error is due to multiplexer being terminated.
    pub fn is_terminated(&self) -> bool {
        matches!(self, Self::Multiplexer)
    }
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Multiplexer => write!(f, "multiplexer terminated"),
            Self::ExceedsMaxDataSize(max_size) => {
                write!(f, "data exceeds maximum allowed size of {} bytes", max_size)
            }
            Self::ExceedsMaxPortCount(max_count) => {
                write!(f, "port message exceeds maximum allowed count of {} ports", max_count)
            }
        }
    }
}

impl Error for ReceiveError {}

/// An error occured during receiving a message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ReceiveAnyError {
    /// Multiplexer terminated.
    Multiplexer,
}

impl ReceiveAnyError {
    /// Returns true, if error is due to multiplexer being terminated.
    pub fn is_terminated(&self) -> bool {
        matches!(self, Self::Multiplexer)
    }
}

impl fmt::Display for ReceiveAnyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Multiplexer => write!(f, "multiplexer terminated"),
        }
    }
}

impl Error for ReceiveAnyError {}

/// An error occured during receiving chunks of a message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ReceiveChunkError {
    /// Multiplexer terminated.
    Multiplexer,
    /// Remote endpoint cancelled transmission.
    Cancelled,
}

impl ReceiveChunkError {
    /// Returns true, if error is due to multiplexer being terminated.
    pub fn is_terminated(&self) -> bool {
        matches!(self, Self::Multiplexer)
    }
}

impl fmt::Display for ReceiveChunkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Multiplexer => write!(f, "multiplexer terminated"),
            Self::Cancelled => write!(f, "transmission cancelled"),
        }
    }
}

/// Container for received data.
pub(crate) struct ReceivedData {
    /// Received data.
    pub buf: Bytes,
    /// First chunk of data.
    pub first: bool,
    /// Last chunk of data.
    pub last: bool,
    /// Flow-control credit.
    pub credit: UsedCredit,
}

/// Container for received port open requests.
pub(crate) struct ReceivedPortRequests {
    /// Port open requests.
    pub requests: Vec<Request>,
    /// First chunk of ports.
    pub first: bool,
    /// Last chunk of ports.
    pub last: bool,
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

    fn try_push(&mut self, buf: Bytes, max_size: usize) -> Result<(), Bytes> {
        match self.remaining.checked_add(buf.len()) {
            Some(new_size) if new_size <= max_size => {
                self.bufs.push_back(buf);
                self.remaining = new_size;
                Ok(())
            }
            _ => Err(buf),
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
    /// Data was received that exceeds the receive buffer size.
    ///
    /// Use [Receiver::recv_chunk] to receive data in chunks.
    BigData,
    /// Port open requests.
    Requests(Vec<Request>),
}

enum Receiving {
    Nothing,
    Data(DataBuf),
    Chunks { chunks: VecDeque<Bytes>, completed: bool },
    Requests(Vec<Request>),
}

impl Default for Receiving {
    fn default() -> Self {
        Self::Nothing
    }
}

/// Receives byte data over a channel.
pub struct Receiver {
    local_port: u32,
    remote_port: u32,
    max_data_size: usize,
    max_ports: usize,
    tx: mpsc::Sender<PortEvt>,
    rx: mpsc::UnboundedReceiver<PortReceiveMsg>,
    receiving: Receiving,
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
            .field("max_data_size", &self.max_data_size)
            .field("max_port_count", &self.max_ports)
            .field("closed", &self.closed)
            .field("finished", &self.finished)
            .finish()
    }
}

impl Receiver {
    pub(crate) fn new(
        local_port: u32, remote_port: u32, max_data_size: usize, max_port_count: usize,
        tx: mpsc::Sender<PortEvt>, rx: mpsc::UnboundedReceiver<PortReceiveMsg>, credits: ChannelCreditReturner,
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
            max_ports: max_port_count,
            tx,
            rx,
            receiving: Receiving::Nothing,
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

    /// Maximum data size in bytes to receive per message.
    ///
    /// The default value is specified by [Cfg::max_data_size].
    ///
    /// [recv_chunk] is not affected by this limit.
    pub fn max_data_size(&self) -> usize {
        self.max_data_size
    }

    /// Set maximum data size in bytes to receive per message.
    ///
    /// [recv_chunk] is not affected by this limit.
    pub fn set_max_data_size(&mut self, max_data_size: usize) {
        self.max_data_size = max_data_size;
    }

    /// Maximum port count per message.
    ///
    /// The default value is specified by [Cfg::max_received_ports].
    pub fn max_ports(&self) -> usize {
        self.max_ports
    }

    /// Set maximum port count per message.
    pub fn set_max_ports(&mut self, max_ports: usize) {
        self.max_ports = max_ports;
    }

    /// Receives data over the channel.
    ///
    /// Waits for data to become available.
    /// Received port open requests are silently rejected.
    pub async fn recv(&mut self) -> Result<Option<DataBuf>, ReceiveError> {
        loop {
            match self.recv_any().await? {
                Some(Received::Data(data)) => break Ok(Some(data)),
                Some(Received::BigData) => break Err(ReceiveError::ExceedsMaxDataSize(self.max_data_size)),
                Some(Received::Requests(_)) => (),
                None => break Ok(None),
            }
        }
    }

    /// Receives chunks of a message over the channel.
    pub async fn recv_chunk(&mut self) -> Result<Option<Bytes>, ReceiveChunkError> {
        if self.finished {
            return Ok(None);
        }

        loop {
            self.credits.return_flush().await;

            match &mut self.receiving {
                // Chunks from receive operation started by recv_any available.
                Receiving::Chunks { chunks, .. } if !chunks.is_empty() => {
                    return Ok(Some(chunks.pop_front().unwrap()))
                }

                // Previous received chunk was last of message.
                Receiving::Chunks { completed: true, .. } => {
                    self.receiving = Receiving::Nothing;
                    return Ok(None);
                }

                // Try to receive next chunk.
                _ => match self.rx.recv().await {
                    Some(PortReceiveMsg::Data(data)) => {
                        self.credits.start_return(data.credit, self.remote_port, &self.tx);

                        match (&self.receiving, data.first) {
                            // First segment without last segment indicates that last transmission
                            // was cancelled.
                            (Receiving::Chunks { .. }, true) => {
                                self.receiving =
                                    Receiving::Chunks { chunks: vec![data.buf].into(), completed: data.last };
                                return Err(ReceiveChunkError::Cancelled);
                            }
                            // Either continuation or start of transmission.
                            (Receiving::Chunks { .. }, false) | (_, true) => {
                                self.receiving =
                                    Receiving::Chunks { chunks: VecDeque::new(), completed: data.last };
                                return Ok(Some(data.buf));
                            }
                            // Ignore transmission without start.
                            (_, false) => (),
                        }
                    }

                    // Either aborted transmission or port data to ignore.
                    Some(PortReceiveMsg::PortRequests(req)) => {
                        self.credits.start_return(req.credit, self.remote_port, &self.tx);
                        if let Receiving::Chunks { .. } = &self.receiving {
                            self.receiving = Receiving::Nothing;
                            return Err(ReceiveChunkError::Cancelled);
                        }
                    }

                    // Port closure.
                    Some(PortReceiveMsg::Finished) => {
                        self.finished = true;
                        if let Receiving::Chunks { .. } = &self.receiving {
                            self.receiving = Receiving::Nothing;
                            return Err(ReceiveChunkError::Cancelled);
                        } else {
                            return Ok(None);
                        }
                    }

                    None => return Err(ReceiveChunkError::Multiplexer),
                },
            }
        }
    }

    /// Receives data or ports over the channel.
    pub async fn recv_any(&mut self) -> Result<Option<Received>, ReceiveError> {
        if self.finished {
            return Ok(None);
        }

        loop {
            self.credits.return_flush().await;

            let received = match self.rx.recv().await {
                // Data message.
                Some(PortReceiveMsg::Data(data)) => {
                    self.credits.start_return(data.credit, self.remote_port, &self.tx);

                    if data.first {
                        self.receiving = Receiving::Data(DataBuf::new());
                    }

                    if let Receiving::Data(mut data_buf) = mem::take(&mut self.receiving) {
                        // Try to add data to buffer.
                        match data_buf.try_push(data.buf, self.max_data_size) {
                            // Data fits into buffer.
                            Ok(()) => {
                                if data.last {
                                    return Ok(Some(Received::Data(data_buf)));
                                } else {
                                    self.receiving = Receiving::Data(data_buf);
                                }
                            }

                            // Maximum message size has been reached.
                            Err(buf) => {
                                data_buf.bufs.push_back(buf);
                                self.receiving =
                                    Receiving::Chunks { chunks: data_buf.bufs, completed: data.last };
                                return Ok(Some(Received::BigData));
                            }
                        }
                    }
                }

                // Port connection requests.
                Some(PortReceiveMsg::PortRequests(req)) => {
                    self.credits.start_return(req.credit, self.remote_port, &self.tx);

                    if req.first {
                        self.receiving = Receiving::Requests(Vec::new());
                    }

                    if let Receiving::Requests(mut requests) = mem::take(&mut self.receiving) {
                        requests.extend(req.requests);

                        if requests.len() > self.max_ports {
                            self.receiving = Receiving::Nothing;
                            return Err(ReceiveError::ExceedsMaxPortCount(self.max_ports));
                        }

                        if req.last {
                            return Ok(Some(Received::Requests(requests)));
                        } else {
                            self.receiving = Receiving::Requests(requests);
                        }
                    }
                }

                // Port closure.
                Some(PortReceiveMsg::Finished) => {
                    self.finished = true;
                    return Ok(None);
                }

                None => return Err(ReceiveError::Multiplexer),
            };
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

// impl Stream for ReceiverStream {
//     type Item = Result<DataBuf, ReceiveError>;
//
//     fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
//         Poll::Ready(ready!(Pin::into_inner(self).0.poll_recv(cx)).transpose())
//     }
// }
