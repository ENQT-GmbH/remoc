use bytes::Bytes;
use futures::{
    future, pin_mut,
    sink::{Sink, SinkExt},
    stream::{Stream, StreamExt},
    Future, FutureExt,
};
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    error::Error,
    fmt,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::{mpsc, mpsc::Permit, oneshot, Mutex},
    time::{sleep, timeout},
    try_join,
};

use crate::{
    client::{ConnectRequest, ConnectResponse, RawClient},
    credit::{
        credit_monitor_pair, credit_send_pair, ChannelCreditMonitor, CreditProvider, GlobalCreditMonitor,
        GlobalCredits,
    },
    listener::{RawListener, RemoteConnectMsg, Request},
    msg::{Credit, MultiplexMsg},
    port_allocator::{PortAllocator, PortNumber},
    receiver::{PortReceiveMsg, RawReceiver, ReceivedData, ReceivedPortRequests},
    sender::RawSender,
    Cfg, MultiplexError, PROTOCOL_VERSION,
};

/// Multiplexer protocol error.
macro_rules! protocol_err {
    ($msg:expr) => {
        crate::MultiplexError::Protocol($msg.to_string())
    };
}

/// Port state.
#[derive(Debug)]
enum PortState {
    /// OpenPort request has been initiated locally and is waiting for reply
    /// from remote endpoint.
    Connecting {
        /// Channel for providing the response to the local requester.
        response_tx: oneshot::Sender<ConnectResponse>,
    },
    /// Port is connected.
    Connected {
        /// Remote port.
        remote_port: u32,
        /// Credit provider for sending.
        /// Initially present, None when Hangup message has been received.
        sender_credit_provider: CreditProvider,
        /// Receive queue.
        receiver_tx_data: Option<mpsc::UnboundedSender<PortReceiveMsg>>,
        /// Channel-specific credit monitor for received data.
        receiver_credit_monitor: ChannelCreditMonitor,
        /// Local receiver has been closed and ReceiveClose message has been sent to remote endpoint.
        receiver_closed: bool,
        /// Local receiver has been dropped and ReceiveFinish message has been sent to remote endpoint,
        /// thus no more port credits can be returned.
        receiver_dropped: bool,
        /// Sender has been dropped and SendFinish message has been sent to remote endpoint,
        /// thus no more data can be send from this port to remote endpoint.
        sender_dropped: bool,
        /// Remote receiver has been closed, thus no more data should be sent to remote port.
        remote_receiver_closed: Arc<AtomicBool>,
        /// Notification senders for when remote receiver has been closed.
        remote_receiver_closed_notify: Arc<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
        /// Remote receiver has been dropped, thus no more sent data will be processed and
        /// no port credits will be returned.
        remote_receiver_dropped: bool,
    },
}

/// Event from a port to the multiplexer event loop.
#[derive(Debug)]
pub(crate) enum PortEvt {
    /// Connection has been accepted.
    Accepted {
        /// Allocated local port.
        local_port: PortNumber,
        /// Remote port.
        remote_port: u32,
        /// Reply with port sender and receiver.
        port_tx: oneshot::Sender<(RawSender, RawReceiver)>,
    },
    /// Connection was rejected.
    Rejected {
        /// Remote port.
        remote_port: u32,
        /// True if rejection due to no ports available.
        no_ports: bool,
    },
    /// Send message with content.
    SendData {
        /// Remote port that will receive data.
        remote_port: u32,
        /// Data to send.
        data: Bytes,
        /// First chunk of data.
        first: bool,
        /// Last part of data.
        last: bool,
        /// Flow-control credit.
        credit: Credit,
    },
    /// Send ports.
    SendPorts {
        /// Remote port that will receive ports.
        remote_port: u32,
        /// Flow-control credit.
        credit: Credit,
        /// Wait for remote port to become available.
        wait: bool,
        /// Ports to send, together with response channel.
        ports: Vec<(PortNumber, oneshot::Sender<ConnectResponse>)>,
    },
    /// Return channel-specific flow control credits.
    ReturnCredits {
        /// Remote port that will receive credits.
        remote_port: u32,
        /// Number of credits.
        credits: u16,
    },
    /// Sender has been dropped.
    SenderDropped {
        /// Local port.
        local_port: u32,
    },
    /// Receiver has been closed, i.e. remote endpoint should stop sending messages on this channel.
    ReceiverClosed {
        /// Local port.
        local_port: u32,
    },
    /// Receiver has been dropped.
    ReceiverDropped {
        /// Local port.
        local_port: u32,
    },
}

// Global event.
#[derive(Debug)]
enum GlobalEvt {
    /// Connect request from local client.
    ConnectReq(ConnectRequest),
    /// All local clients have been dropped.
    AllClientsDropped,
    /// Local listener has been dropped.
    ListenerDropped,
    /// Event from an open port.
    Port(PortEvt),
    /// Return global credits.
    ReturnGlobalCredits(u16),
    /// Send Goodbye message.
    SendGoodbye,
    /// Flush transport send queue.
    Flush,
}

/// Command to transport send task.
enum SendCmd {
    /// Send attached message.
    Send(TransportMsg),
    /// Flush sink.
    Flush,
}

/// Message with optionally associated data.
struct TransportMsg {
    /// Message.
    msg: MultiplexMsg,
    /// Data chunk, if message is data message.
    data: Option<Bytes>,
}

impl TransportMsg {
    /// Message only.
    fn new(msg: MultiplexMsg) -> Self {
        assert!(!matches!(&msg, &MultiplexMsg::Data { .. }), "MultiplexMsg::Data with missing data");
        Self { msg, data: None }
    }

    /// Message with data.
    fn with_data(msg: MultiplexMsg, data: Bytes) -> Self {
        assert!(matches!(&msg, &MultiplexMsg::Data { .. }), "MultiplexMsg with unexpected data");
        Self { msg, data: Some(data) }
    }
}

/// Channel multiplexer.
pub struct Multiplexer<TransportSink, TransportStream> {
    /// Trace id.
    trace_id: String,
    /// Our configuration.
    local_cfg: Cfg,
    /// Remote configuration.
    remote_cfg: Cfg,
    /// Remote protocol version.
    remote_protocol_version: u8,
    /// Channel for connection requests from local client.
    connect_rx: Option<mpsc::UnboundedReceiver<ConnectRequest>>,
    /// Channels for connection requests from remote endpoint with wait set and not set.
    listen_tx: Option<(mpsc::Sender<RemoteConnectMsg>, mpsc::Sender<RemoteConnectMsg>)>,
    /// Port allocator.
    port_allocator: PortAllocator,
    /// Open local ports.
    ports: HashMap<PortNumber, PortState>,
    /// Sender from channels to event loop.
    channel_tx: mpsc::Sender<PortEvt>,
    /// Channel receiver of event loop.
    channel_rx: Option<mpsc::Receiver<PortEvt>>,
    /// All user clients have been dropped.
    all_clients_dropped: bool,
    /// Remote client has been dropped.
    remote_client_dropped: bool,
    /// Remote listener has been dropped.
    remote_listener_dropped: Arc<AtomicBool>,
    /// Goodbye message has been sent.
    goodbye_sent: bool,
    /// Goodbye message has been received.
    goodbye_received: bool,
    /// Global transmission credits.
    global_tx_credits: GlobalCredits,
    /// Global received credits returner.
    global_rx_credits_monitor: GlobalCreditMonitor,
    /// Receive end of channel for returning received global credits.
    global_rx_credits_return_rx: Option<mpsc::UnboundedReceiver<u16>>,
    /// Transport sender.
    transport_sink: Option<TransportSink>,
    /// Transport receiver.
    transport_stream: Option<TransportStream>,
}

impl<TransportSink, TransportStream> fmt::Debug for Multiplexer<TransportSink, TransportStream> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Multiplexer")
            .field("trace_id", &self.trace_id)
            .field("local_cfg", &self.local_cfg)
            .field("remote_cfg", &self.remote_cfg)
            .field("local_protocol_version", &PROTOCOL_VERSION)
            .field("remote_protocol_version", &self.remote_protocol_version)
            .finish_non_exhaustive()
    }
}

impl<TransportSink, TransportSinkError, TransportStream, TransportStreamError>
    Multiplexer<TransportSink, TransportStream>
where
    TransportSink: Sink<Bytes, Error = TransportSinkError> + Send + Unpin,
    TransportSinkError: Error + Send + Sync + 'static,
    TransportStream: Stream<Item = Result<Bytes, TransportStreamError>> + Send + Unpin,
    TransportStreamError: Error + Send + Sync + 'static,
{
    /// Creates a new multiplexer.
    ///
    /// After creation use the `run` method of the multiplexer to launch the dispatch task.
    pub async fn new(
        cfg: &Cfg, mut transport_sink: TransportSink, mut transport_stream: TransportStream,
    ) -> Result<(Self, RawClient, RawListener), MultiplexError<TransportSinkError, TransportStreamError>> {
        // Check configuration.
        if cfg.max_ports.get() > 2u32.pow(31) {
            return Err(MultiplexError::TooManyPorts);
        }

        // Get trace id.
        let trace_id = match cfg.trace_id.clone() {
            Some(trace_id) => trace_id,
            None => generate_trace_id(),
        };

        // Say hello to remote endpoint and exchange configurations.
        log::trace!("{}: exchanging hello", &trace_id);
        let fut = Self::exchange_hello(&trace_id, cfg, &mut transport_sink, &mut transport_stream);
        let (remote_protocol_version, remote_cfg) = match cfg.connection_timeout {
            Some(dur) => timeout(dur, fut).await.map_err(|_| MultiplexError::Timeout)??,
            None => fut.await?,
        };

        // Create channels.
        let (channel_tx, channel_rx) = mpsc::channel(cfg.shared_send_queue.get().into());
        let (listen_wait_tx, listen_wait_rx) = mpsc::channel(usize::from(cfg.connect_queue.get()) + 1);
        let (listen_no_wait_tx, listen_no_wait_rx) = mpsc::channel(usize::from(cfg.connect_queue.get()) + 1);
        let (connect_tx, connect_rx) = mpsc::unbounded_channel();

        // Create user objects.
        let port_allocator = PortAllocator::new(cfg.max_ports.get());
        let (global_rx_credits_return_tx, global_rx_credits_return_rx) = mpsc::unbounded_channel();
        let remote_listener_dropped = Arc::new(AtomicBool::new(false));
        let multiplexer = Multiplexer {
            trace_id,
            remote_protocol_version,
            local_cfg: cfg.clone(),
            remote_cfg: remote_cfg.clone(),
            connect_rx: Some(connect_rx),
            listen_tx: Some((listen_wait_tx, listen_no_wait_tx)),
            port_allocator: port_allocator.clone(),
            ports: HashMap::new(),
            channel_tx,
            channel_rx: Some(channel_rx),
            remote_client_dropped: false,
            remote_listener_dropped: remote_listener_dropped.clone(),
            all_clients_dropped: false,
            goodbye_sent: false,
            goodbye_received: false,
            global_tx_credits: GlobalCredits::new(remote_cfg.shared_receive_queue),
            global_rx_credits_monitor: GlobalCreditMonitor::new(
                cfg.shared_receive_queue,
                global_rx_credits_return_tx,
            ),
            global_rx_credits_return_rx: Some(global_rx_credits_return_rx),
            transport_sink: Some(transport_sink),
            transport_stream: Some(transport_stream),
        };

        let raw_client = RawClient::new(
            connect_tx,
            remote_cfg.connect_queue.get(),
            port_allocator.clone(),
            remote_listener_dropped,
        );
        let raw_server = RawListener::new(listen_wait_rx, listen_no_wait_rx, port_allocator);

        log::trace!("{}: multiplexer created", &multiplexer.trace_id);
        Ok((multiplexer, raw_client, raw_server))
    }

    /// Feed transport message to sink and log it.
    async fn feed_msg(
        trace_id: &str, msg: TransportMsg, sink: &mut TransportSink,
    ) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        log::trace!("{} ==> {:?}", trace_id, &msg.msg);
        sink.feed(msg.msg.to_vec().into()).await.map_err(MultiplexError::SinkError)?;

        if let Some(data) = msg.data {
            log::trace!("{} ==> [{} bytes data]", trace_id, data.len());
            sink.feed(data).await.map_err(MultiplexError::SinkError)?;
        }

        Ok(())
    }

    /// Flush sink and log it.
    async fn flush(
        trace_id: &str, sink: &mut TransportSink,
    ) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        log::trace!("{} ==> [flush]", trace_id);
        sink.flush().await.map_err(MultiplexError::SinkError)
    }

    /// Receive message and log it.
    async fn recv_msg(
        trace_id: &str, stream: &mut TransportStream,
    ) -> Result<TransportMsg, MultiplexError<TransportSinkError, TransportStreamError>> {
        let msg_data = match stream.next().await {
            Some(Ok(msg_data)) => msg_data,
            Some(Err(err)) => return Err(MultiplexError::StreamError(err)),
            None => return Err(MultiplexError::StreamClosed),
        };

        let msg = MultiplexMsg::from_slice(&msg_data)?;
        log::trace!("{} <== {:?}", trace_id, &msg);

        let data = if let MultiplexMsg::Data { .. } = &msg {
            match stream.next().await {
                Some(Ok(data)) => {
                    log::trace!("{} <== [{} bytes data]", trace_id, data.len());
                    Some(data)
                }
                Some(Err(err)) => return Err(MultiplexError::StreamError(err)),
                None => return Err(MultiplexError::StreamClosed),
            }
        } else {
            None
        };

        Ok(TransportMsg { msg, data })
    }

    /// Exchange Hello message with remote endpoint.
    async fn exchange_hello(
        trace_id: &str, cfg: &Cfg, sink: &mut TransportSink, stream: &mut TransportStream,
    ) -> Result<(u8, Cfg), MultiplexError<TransportSinkError, TransportStreamError>> {
        // Say hello to remote endpoint and send our configuration.
        let send_task = async {
            Self::feed_msg(trace_id, TransportMsg::new(MultiplexMsg::Reset), sink).await?;
            Self::flush(trace_id, sink).await?;
            Self::feed_msg(
                trace_id,
                TransportMsg::new(MultiplexMsg::Hello { version: PROTOCOL_VERSION, cfg: cfg.clone() }),
                sink,
            )
            .await?;
            Self::flush(trace_id, sink).await?;
            Ok(())
        };

        // Receive hello and configuration from remote endpoint.
        let recv_task = async {
            loop {
                match Self::recv_msg(trace_id, stream).await {
                    Ok(TransportMsg { msg: MultiplexMsg::Hello { version, cfg }, .. }) => {
                        break Ok((version, cfg))
                    }
                    Ok(_) => (),
                    Err(MultiplexError::Protocol(_)) => (),
                    Err(err) => return Err(err),
                }
            }
        };

        Ok(try_join!(send_task, recv_task)?.1)
    }

    /// Returns true, when muxer task should terminate because no more
    /// requests are possible.
    fn should_terminate(&self) -> bool {
        let mut terminate = true;

        // Ensures that all ports are closed on both sides.
        terminate &= self.ports.is_empty();
        // Ensures that local clients are all dropped or remote listener is dropped.
        terminate &= self.all_clients_dropped || self.remote_listener_dropped.load(Ordering::SeqCst);
        // Ensures that local listener or all remote clients are dropped.
        terminate &= self.listen_tx.is_none() || self.remote_client_dropped;

        terminate
    }

    /// Create port in port registry and return associated sender and receiver.
    fn create_port(&mut self, local_port: PortNumber, remote_port: u32) -> (RawSender, RawReceiver) {
        let local_port_num = *local_port;
        log::trace!(
            "{}: created port {} connected to remote port {}",
            &self.trace_id,
            &local_port_num,
            remote_port
        );

        let sender_tx = self.channel_tx.clone();
        let (sender_credit_provider, sender_credit_user) =
            credit_send_pair(self.remote_cfg.port_receive_queue.get(), &self.global_tx_credits);

        let receiver_tx = self.channel_tx.clone();
        let (receiver_tx_data, receiver_rx_data) = mpsc::unbounded_channel();
        let (receiver_credit_monitor, receiver_credit_returner) =
            credit_monitor_pair(self.local_cfg.port_receive_queue.get());

        let hangup_notify = Arc::new(Mutex::new(Some(Vec::new())));
        let hangup_recved = Arc::new(AtomicBool::new(false));

        if let Some(PortState::Connected { remote_port, .. }) = self.ports.insert(
            local_port,
            PortState::Connected {
                remote_port,
                sender_credit_provider,
                receiver_tx_data: Some(receiver_tx_data),
                receiver_credit_monitor,
                remote_receiver_closed_notify: hangup_notify.clone(),
                remote_receiver_closed: hangup_recved.clone(),
                receiver_closed: false,
                receiver_dropped: false,
                sender_dropped: false,
                remote_receiver_dropped: false,
            },
        ) {
            panic!(
                "create_port called for local port {} already connected to remote port {}",
                local_port_num, remote_port
            );
        }

        let raw_sender = RawSender::new(
            local_port_num,
            remote_port,
            self.remote_cfg.max_data_size.get(),
            self.remote_cfg.chunk_size.get() as usize,
            sender_tx,
            sender_credit_user,
            Arc::downgrade(&hangup_recved),
            Arc::downgrade(&hangup_notify),
        );

        let raw_receiver = RawReceiver::new(
            local_port_num,
            remote_port,
            self.local_cfg.max_data_size.get(),
            receiver_tx,
            receiver_rx_data,
            receiver_credit_returner,
        );

        (raw_sender, raw_receiver)
    }

    /// Releases a port if no more local requests to it are possible
    /// and no more messages from the remote endpoint can reference it.
    fn maybe_free_port(&mut self, local_port: u32) {
        let mut free = true;

        if let Some(PortState::Connected {
            receiver_tx_data,
            receiver_dropped,
            sender_dropped,
            remote_receiver_dropped,
            ..
        }) = self.ports.get(&local_port)
        {
            // Ensures that local sender is dropped and thus no more
            // Data and SendFinish messages can be sent for port.
            free &= *sender_dropped;
            // Ensures that local receiver is dropped and thus no more
            // PortCredits, ReceiveClose and ReceiveFinish messages can be sent for port.
            free &= *receiver_dropped;
            // Ensures that remote sender has been dropped and thus no more
            // Data and SendFinish messages can be received for port.
            free &= receiver_tx_data.is_none();
            // Ensures that remote receiver is dropped and thus no more
            // PortCredits, ReceiveClose and ReceiveFinish messages can be received for port.
            free &= *remote_receiver_dropped;
        } else {
            panic!("maybe_free_port called for port {} not in connected state.", &local_port);
        }

        if free {
            log::trace!("{}: freed port {}", &self.trace_id, &local_port);
            self.ports.remove(&local_port);
        }
    }

    /// Sends data over the transport sink.
    ///
    /// Automatically sends pings if no data is to be transmitted.
    async fn send_task(
        trace_id: &str, mut sink: &mut TransportSink, ping_interval: Option<Duration>,
        mut rx: mpsc::Receiver<SendCmd>,
    ) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        async fn get_next_ping(ping_interval: Option<Duration>) {
            match ping_interval {
                Some(interval) => sleep(interval).await,
                None => future::pending().await,
            }
        }

        let mut next_ping = get_next_ping(ping_interval).fuse().boxed();

        loop {
            SinkReady::new(&mut sink).await.map_err(MultiplexError::SinkError)?;

            tokio::select! {
                biased;

                cmd_opt = rx.recv() => {
                    match cmd_opt {
                        Some(SendCmd::Send (msg)) => {
                            let is_goodbye = matches!(&msg, TransportMsg {msg: MultiplexMsg::Goodbye, ..});
                            Self::feed_msg(trace_id, msg, sink).await?;
                            if is_goodbye {
                                break;
                            }

                            next_ping = get_next_ping(ping_interval).fuse().boxed();
                        }
                        Some(SendCmd::Flush) => Self::flush(trace_id, sink).await?,
                        None => break,
                    }
                }

                () = &mut next_ping => {
                    Self::feed_msg(trace_id, TransportMsg::new(MultiplexMsg::Ping), sink).await?;
                    Self::flush(trace_id, sink).await?;
                    next_ping = get_next_ping(ping_interval).fuse().boxed();
                }
            }
        }

        // Flushing may fail after Goodbye message has been sent, because the remote
        // endpoint may immediately close the connection.
        let _ = Self::flush(trace_id, sink).await;

        Ok(())
    }

    /// Receives data over the transport sink.
    ///
    /// Watches the connection timeout.
    async fn recv_task(
        trace_id: &str, stream: &mut TransportStream, connection_timeout: Option<Duration>,
        tx: mpsc::Sender<TransportMsg>,
    ) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        async fn get_connection_timeout(connection_timeout: Option<Duration>) {
            match connection_timeout {
                Some(timeout) => sleep(timeout).await,
                None => future::pending().await,
            }
        }

        let mut next_timeout = get_connection_timeout(connection_timeout).fuse().boxed();
        while let Ok(tx_permit) = tx.reserve().await {
            tokio::select! {
                biased;

                msg = Self::recv_msg(trace_id, stream) => {
                    let msg = msg?;
                    let is_goodbye = matches!(&msg, TransportMsg {msg: MultiplexMsg::Goodbye, ..});
                    tx_permit.send(msg);
                    if is_goodbye {
                        break;
                    }

                    next_timeout = get_connection_timeout(connection_timeout).fuse().boxed();
                },

                () = &mut next_timeout => return Err(MultiplexError::Timeout),
            }
        }

        Ok(())
    }

    /// Runs the multiplexer dispatcher.
    ///
    /// The dispatcher terminates when the client, server and all channels have been dropped or
    /// the transport is closed.
    pub async fn run(mut self) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        let trace_id = self.trace_id.clone();
        let mut transport_sink = self.transport_sink.take().unwrap();
        let mut transport_stream = self.transport_stream.take().unwrap();

        log::trace!("{}: multiplexer run", &trace_id);

        // Create send over transport task.
        let (send_tx, send_rx) = mpsc::channel(1);
        let send_task = Self::send_task(
            &trace_id,
            &mut transport_sink,
            self.remote_cfg.connection_timeout.map(|d| d / 2),
            send_rx,
        )
        .fuse();
        pin_mut!(send_task);

        // Create receive over transport task.
        let (recv_tx, mut recv_rx) = mpsc::channel(1);
        let recv_task =
            Self::recv_task(&trace_id, &mut transport_stream, self.local_cfg.connection_timeout, recv_tx).fuse();
        pin_mut!(recv_task);

        // Setup channels.
        let mut channel_rx = self.channel_rx.take().unwrap();
        let mut connect_rx = self.connect_rx.take().unwrap();
        let mut global_rx_credits_return_rx = self.global_rx_credits_return_rx.take().unwrap();
        let mut flushed = false;
        let mut send_task_ended = false;

        while !(self.goodbye_sent && self.goodbye_received && send_task_ended) {
            let send_prep_task = async {
                // Obtain permit to ensure that space is available in transport send queue.
                let permit = match send_tx.reserve().await {
                    Ok(permit) => permit,
                    Err(_) => return None,
                };

                // Select local request for processing.
                let event = tokio::select! {
                    biased;

                    // Global credit return.
                    Some(credits) = global_rx_credits_return_rx.recv() => {
                        flushed = false;
                        GlobalEvt::ReturnGlobalCredits(credits)
                    },

                    // Server dropped.
                    () = async { match &self.listen_tx {
                        Some((listen_wait_tx, _)) => listen_wait_tx.closed().await,
                        None => future::pending().await
                    }} => {
                        // listen_no_wait_tx is closed simultaneously.
                        flushed = false;
                        GlobalEvt::ListenerDropped
                    },

                    // Connection request from client.
                    connect_req_opt = connect_rx.recv(), if !self.all_clients_dropped => {
                        flushed = false;
                        match connect_req_opt {
                            Some(connect_req) => GlobalEvt::ConnectReq(connect_req),
                            None => GlobalEvt::AllClientsDropped,
                        }
                    },

                    // Request from port.
                    Some(msg) = channel_rx.recv() => {
                        flushed = false;
                        GlobalEvt::Port(msg)
                    },

                    // Send Goodbye message and terminate.
                    () = future::ready(()), if self.should_terminate() && !self.goodbye_sent => {
                        GlobalEvt::SendGoodbye
                    }

                    // Flush transport sink if no requests are queued.
                    () = future::ready(()), if !flushed => {
                        flushed = true;
                        GlobalEvt::Flush
                    },
                };

                Some((permit, event))
            };

            tokio::select! {
                // Local send request.
                Some((permit, event)) = send_prep_task => self.handle_event(&trace_id, permit, event).await?,

                // Received message from remote endpoint.
                Some(msg) = recv_rx.recv() => self.handle_received_msg(&trace_id, msg).await?,

                // Send task ended.
                res = &mut send_task => {
                    match res {
                        Ok(()) => send_task_ended = true,
                        Err(err) => return Err(err),
                    }
                }

                // Receive task failed.
                Err(err) = &mut recv_task => return Err(err),
            }
        }

        log::trace!("{}: multiplexer normal exit", &trace_id);
        Ok(())
    }

    /// Handle local event that results in sending a message to the remote endpoint.
    async fn handle_event(
        &mut self, trace_id: &str, permit: Permit<'_, SendCmd>, event: GlobalEvt,
    ) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        log::trace!("{}: processing event {:?}", trace_id, &event);

        fn send_msg(permit: Permit<'_, SendCmd>, msg: MultiplexMsg) {
            permit.send(SendCmd::Send(TransportMsg::new(msg)))
        }

        match event {
            // Process local connect request.
            GlobalEvt::ConnectReq(ConnectRequest { local_port, sent_tx: _sent_tx, response_tx, wait }) => {
                if !self.remote_listener_dropped.load(Ordering::SeqCst) {
                    let local_port_num = *local_port;
                    if self.ports.insert(local_port, PortState::Connecting { response_tx }).is_some() {
                        panic!("ConnectRequest for already used local port {}", local_port_num);
                    }
                    send_msg(permit, MultiplexMsg::OpenPort { client_port: local_port_num, wait });
                } else {
                    let _ = response_tx.send(ConnectResponse::Rejected { no_ports: false });
                }
            }

            // Remote connect request was accepted by local listener.
            GlobalEvt::Port(PortEvt::Accepted { local_port, remote_port, port_tx }) => {
                let local_port_num = *local_port;
                send_msg(
                    permit,
                    MultiplexMsg::PortOpened { client_port: remote_port, server_port: local_port_num },
                );
                let (raw_sender, raw_receiver) = self.create_port(local_port, remote_port);
                let _ = port_tx.send((raw_sender, raw_receiver));
            }

            // Remote connect request was rejected by local listener.
            GlobalEvt::Port(PortEvt::Rejected { remote_port, no_ports }) => {
                send_msg(permit, MultiplexMsg::Rejected { client_port: remote_port, no_ports });
            }

            // Send data from port.
            GlobalEvt::Port(PortEvt::SendData { remote_port, data, first, last, credit }) => {
                permit.send(SendCmd::Send(TransportMsg::with_data(
                    MultiplexMsg::Data { port: remote_port, first, last, credit },
                    data,
                )));
            }

            // Send ports from port.
            GlobalEvt::Port(PortEvt::SendPorts { remote_port, ports, credit, wait }) => {
                let mut port_nums = Vec::new();
                for (port, response_tx) in ports {
                    let port_num = *port;
                    if self.ports.insert(port, PortState::Connecting { response_tx }).is_some() {
                        panic!("SendPorts with already used local port {}", port_num);
                    }
                    port_nums.push(port_num);
                }
                send_msg(permit, MultiplexMsg::PortData { port: remote_port, wait, credit, ports: port_nums });
            }

            // Return port credits to remote endpoint.
            GlobalEvt::Port(PortEvt::ReturnCredits { remote_port, credits }) => {
                send_msg(permit, MultiplexMsg::PortCredits { port: remote_port, credits });
            }

            // Local port sender has been dropped.
            GlobalEvt::Port(PortEvt::SenderDropped { local_port }) => {
                if let Some(PortState::Connected { remote_port, sender_dropped, .. }) =
                    self.ports.get_mut(&local_port)
                {
                    if *sender_dropped {
                        panic!("PortEvt SenderDropped more than once for port {}", &local_port);
                    }
                    *sender_dropped = true;
                    send_msg(permit, MultiplexMsg::SendFinish { port: *remote_port });
                    self.maybe_free_port(local_port);
                } else {
                    panic!("PortEvt SenderDropped for port {} in invalid state", &local_port);
                }
            }

            // Local port receiver has been closed.
            GlobalEvt::Port(PortEvt::ReceiverClosed { local_port }) => {
                if let Some(PortState::Connected { remote_port, receiver_closed, receiver_dropped, .. }) =
                    self.ports.get_mut(&local_port)
                {
                    if *receiver_closed || *receiver_dropped {
                        panic!(
                            "PortEvt ReceiverClosed or ReceiverDropped more than once for port {}",
                            &local_port
                        );
                    }
                    *receiver_closed = true;
                    send_msg(permit, MultiplexMsg::ReceiveClose { port: *remote_port });
                } else {
                    panic!("PortEvt ReceiverClosed for non-connected port {}", &local_port);
                }
            }

            // Local port receiver has been dropped.
            // No port credits can be returned afterwards.
            GlobalEvt::Port(PortEvt::ReceiverDropped { local_port }) => match self.ports.get_mut(&local_port) {
                Some(PortState::Connected { remote_port, receiver_dropped, .. }) => {
                    if *receiver_dropped {
                        panic!("PortEvt ReceiverDropped more than once for port {}.", &local_port);
                    }
                    *receiver_dropped = true;
                    send_msg(permit, MultiplexMsg::ReceiveFinish { port: *remote_port });
                    self.maybe_free_port(local_port);
                }
                _ => {
                    panic!("PortEvt ReceiverDropped for port {} in invalid state.", &local_port);
                }
            },

            // Return global credits to remote endpoint.
            GlobalEvt::ReturnGlobalCredits(credits) => {
                send_msg(permit, MultiplexMsg::GlobalCredits { credits });
            }

            // Process that all clients has been dropped.
            GlobalEvt::AllClientsDropped => {
                self.all_clients_dropped = true;
                send_msg(permit, MultiplexMsg::ClientFinish);
            }

            // Process that server has been dropped.
            GlobalEvt::ListenerDropped => {
                self.listen_tx = None;
                send_msg(permit, MultiplexMsg::ListenerFinish);
            }

            // Send Goodbye message.
            GlobalEvt::SendGoodbye => {
                self.goodbye_sent = true;
                send_msg(permit, MultiplexMsg::Goodbye);
            }

            // Flush transport sink.
            GlobalEvt::Flush => {
                permit.send(SendCmd::Flush);
            }
        }
        Ok(())
    }

    /// Handle message received from remote endpoint.
    async fn handle_received_msg(
        &mut self, _trace_id: &str, received_msg: TransportMsg,
    ) -> Result<(), MultiplexError<TransportSinkError, TransportStreamError>> {
        let TransportMsg { msg, data } = received_msg;
        // log::trace!("{}: processing received message {:?}", trace_id, &msg);

        match msg {
            // Connection reset by remote endpoint.
            MultiplexMsg::Reset => {
                return Err(MultiplexError::Reset);
            }

            // Hello message only allowed when establishing connection.
            MultiplexMsg::Hello { .. } => {
                return Err(protocol_err!(
                    "received Hello message for already established multiplexer connection"
                ));
            }

            //  Nothing to do for ping message.
            MultiplexMsg::Ping => (),

            // Open port request from remote endpoint.
            MultiplexMsg::OpenPort { client_port, wait } => {
                let req = RemoteConnectMsg::Request(Request::new(
                    client_port,
                    wait,
                    self.port_allocator.clone(),
                    self.channel_tx.clone(),
                ));
                if let Some((listen_wait_tx, listen_no_wait_tx)) = &self.listen_tx {
                    let res = if wait { listen_wait_tx.try_send(req) } else { listen_no_wait_tx.try_send(req) };
                    if let Err(mpsc::error::TrySendError::Full(_)) = res {
                        return Err(protocol_err!("remote endpoint sent too many OpenPort requests"));
                    }
                }
            }

            // Port opened response from remote endpoint.
            MultiplexMsg::PortOpened { client_port, server_port } => {
                if let Some((local_port, PortState::Connecting { response_tx })) =
                    self.ports.remove_entry(&client_port)
                {
                    let (raw_sender, raw_receiver) = self.create_port(local_port, server_port);
                    let _ = response_tx.send(ConnectResponse::Accepted(raw_sender, raw_receiver));
                } else {
                    return Err(protocol_err!(format!(
                        "received PortOpened message for port {} not in connecting state",
                        client_port
                    )));
                }
            }

            // Port open rejected response from remote endpoint.
            MultiplexMsg::Rejected { client_port, no_ports } => {
                if let Some(PortState::Connecting { response_tx }) = self.ports.remove(&client_port) {
                    let _ = response_tx.send(ConnectResponse::Rejected { no_ports });
                } else {
                    return Err(protocol_err!(format!(
                        "received Rejected message for port {} not in connecting state",
                        client_port
                    )));
                }
            }

            // Data from remote endpoint.
            MultiplexMsg::Data { port, first, last, credit } => {
                if let Some(PortState::Connected {
                    receiver_tx_data: Some(receiver_tx_data),
                    receiver_credit_monitor,
                    ..
                }) = self.ports.get_mut(&port)
                {
                    let used_credit = match credit {
                        Credit::Global => self.global_rx_credits_monitor.use_one()?,
                        Credit::Port => receiver_credit_monitor.use_one()?,
                    };
                    let _ = receiver_tx_data.send(PortReceiveMsg::Data(ReceivedData {
                        buf: data.unwrap(),
                        first,
                        last,
                        credit: used_credit,
                    }));
                } else {
                    return Err(protocol_err!(format!(
                        "received data for non-connected or finished local port {}",
                        &port
                    )));
                }
            }

            MultiplexMsg::PortData { port, wait, credit, ports } => {
                if let Some(PortState::Connected {
                    receiver_tx_data: Some(receiver_tx_data),
                    receiver_credit_monitor,
                    ..
                }) = self.ports.get_mut(&port)
                {
                    let used_credit = match credit {
                        Credit::Global => self.global_rx_credits_monitor.use_one()?,
                        Credit::Port => receiver_credit_monitor.use_one()?,
                    };
                    let port_allocator = self.port_allocator.clone();
                    let channel_tx = self.channel_tx.clone();
                    let requests = ports
                        .into_iter()
                        .map(|remote_port| {
                            Request::new(remote_port, wait, port_allocator.clone(), channel_tx.clone())
                        })
                        .collect();
                    let _ = receiver_tx_data.send(PortReceiveMsg::PortRequests(ReceivedPortRequests {
                        requests,
                        credit: used_credit,
                    }));
                } else {
                    return Err(protocol_err!(format!(
                        "received port data for non-connected or finished local port {}",
                        &port
                    )));
                }
            }

            // Give flow credits to a port.
            MultiplexMsg::PortCredits { port, credits } => {
                if let Some(PortState::Connected { sender_credit_provider, .. }) = self.ports.get_mut(&port) {
                    sender_credit_provider.provide(credits)?;
                } else {
                    return Err(protocol_err!(format!(
                        "received port credits message for port {} not in connected state",
                        &port
                    )));
                }
            }

            // Remote endpoint indicates that it will send no more data for port.
            MultiplexMsg::SendFinish { port } => {
                if let Some(PortState::Connected { receiver_tx_data, .. }) = self.ports.get_mut(&port) {
                    if let Some(receiver_tx_data) = receiver_tx_data.take() {
                        let _ = receiver_tx_data.send(PortReceiveMsg::Finished);
                        self.maybe_free_port(port);
                    } else {
                        return Err(protocol_err!(format!(
                            "received SendFinish message for local port {} more than once",
                            &port
                        )));
                    }
                } else {
                    return Err(protocol_err!(format!(
                        "received SendFinish message for local port {} not in connected state",
                        &port
                    )));
                }
            }

            // Remote hang up indicates that it does not want to receive any more data on a port.
            MultiplexMsg::ReceiveClose { port } => {
                if let Some(PortState::Connected {
                    sender_credit_provider,
                    remote_receiver_closed_notify,
                    remote_receiver_closed,
                    ..
                }) = self.ports.get_mut(&port)
                {
                    if !remote_receiver_closed.load(Ordering::SeqCst) {
                        // Disable credits provider.
                        sender_credit_provider.close(true);

                        // Send hangup notifications.
                        remote_receiver_closed.store(true, Ordering::SeqCst);
                        let notifies = remote_receiver_closed_notify.lock().await.take().unwrap();
                        for tx in notifies {
                            let _ = tx.send(());
                        }

                        self.maybe_free_port(port);
                    } else {
                        return Err(protocol_err!(format!(
                            "received more than one ReceiveClose message for port {}",
                            &port
                        )));
                    }
                } else {
                    return Err(protocol_err!(format!(
                        "received ReceiveClose message for port {} not in connected state",
                        &port
                    )));
                }
            }

            // Remote hang up indicates that it will not process any more data on a port.
            MultiplexMsg::ReceiveFinish { port } => {
                if let Some(PortState::Connected {
                    sender_credit_provider,
                    remote_receiver_closed_notify,
                    remote_receiver_closed,
                    remote_receiver_dropped,
                    ..
                }) = self.ports.get_mut(&port)
                {
                    if !remote_receiver_closed.load(Ordering::SeqCst) {
                        // Disable credits provider.
                        sender_credit_provider.close(false);

                        // Send hangup notifications.
                        remote_receiver_closed.store(true, Ordering::SeqCst);
                        let notifies = remote_receiver_closed_notify.lock().await.take().unwrap();
                        for tx in notifies {
                            let _ = tx.send(());
                        }
                    }

                    *remote_receiver_dropped = true;
                    self.maybe_free_port(port);
                } else {
                    return Err(protocol_err!(format!(
                        "received ReceiveFinish message for port {} not in connected state",
                        &port
                    )));
                }
            }

            // Give global flow credits.
            MultiplexMsg::GlobalCredits { credits } => {
                self.global_tx_credits.provide(credits)?;
            }

            // Remote endpoint will send no more connect requests.
            MultiplexMsg::ClientFinish => {
                if let Some((listen_wait_tx, listen_no_wait_tx)) = &self.listen_tx {
                    // One additonal slot is reserved in listen queue for sending the client
                    // dropped notification.
                    let mut failed = false;
                    if let Err(mpsc::error::TrySendError::Full(_)) =
                        listen_wait_tx.try_send(RemoteConnectMsg::ClientDropped)
                    {
                        failed = true;
                    }
                    if let Err(mpsc::error::TrySendError::Full(_)) =
                        listen_no_wait_tx.try_send(RemoteConnectMsg::ClientDropped)
                    {
                        failed = true;
                    }
                    if failed {
                        return Err(protocol_err!(
                            "remote endpoint sent too many OpenPort or ClientFinish requests"
                        ));
                    }
                }

                self.remote_client_dropped = true;
            }

            // Remote endpoint will process no more connect requests.
            MultiplexMsg::ListenerFinish => {
                self.remote_listener_dropped.store(true, Ordering::SeqCst);
            }

            // Remote endpoint terminates connection.
            MultiplexMsg::Goodbye => {
                if self.should_terminate() {
                    self.goodbye_received = true;
                } else {
                    return Err(protocol_err!("received Goodbye prematurely"));
                }
            }
        }

        Ok(())
    }
}

impl<TransportSink, TransportStream> Drop for Multiplexer<TransportSink, TransportStream> {
    fn drop(&mut self) {
        // Should be present to ensure correct drop order.
    }
}

/// A future that resolves when the sink is ready to receive a value.
struct SinkReady<S, Item> {
    sink: S,
    _item: PhantomData<Item>,
}

impl<S, Item> SinkReady<S, Item> {
    fn new(sink: S) -> Self {
        Self { sink, _item: PhantomData }
    }
}

impl<S, Item> Future for SinkReady<S, Item>
where
    S: Sink<Item> + Unpin,
    Item: Unpin,
{
    type Output = Result<(), S::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        Pin::into_inner(self).sink.poll_ready_unpin(cx)
    }
}

/// Generate trace id.
fn generate_trace_id() -> String {
    lazy_static! {
        static ref ID: AtomicU16 = AtomicU16::new(0);
    }

    let id = ID.fetch_add(1, Ordering::SeqCst);
    format!("{:04x}", id)
}
