use futures::channel::{mpsc, oneshot};
use futures::sink::{Sink, SinkExt};
use futures::stream::{self, Stream, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::client::{Client, ConnectToRemoteServiceRequest, ConnectToRemoteServiceResponse};
use crate::codec::{CodecFactory, Serializer};
use crate::number_allocator::{NumberAllocator, NumberAllocatorExhaustedError};
use crate::receive_buffer::{
    ChannelReceiverBufferCfg, ChannelReceiverBufferDequeuer, ChannelReceiverBufferEnqueuer,
};
use crate::receiver::RawReceiver;
use crate::send_lock::{channel_send_lock, ChannelSendLockAuthority, ChannelSendLockRequester};
use crate::sender::RawSender;
use crate::server::{RemoteConnectToServiceRequest, Server};

/// Channel multiplexer error.
#[derive(Debug)]
pub enum MultiplexError {
    /// An error was encountered while sending data to the transport sink.
    TransportSinkError(Box<dyn Error + Send + 'static>),
    /// An error was encountered while receiving data from the transport stream.
    TransportStreamError(Box<dyn Error + Send + 'static>),
    /// The transport stream was close while multiplex channels were active or the
    /// multiplex client was not dropped.
    TransportStreamClosed,
    /// A multiplex protocol error occured.
    ProtocolError(String),
    /// The multiplexer ports were exhausted.
    PortsExhausted,
    /// Error serializing protocol message.
    SerializationError(Box<dyn Error + Send + 'static>),
    /// Error deserializing protocol message.
    DeserializationError(Box<dyn Error + Send + 'static>),
}

impl From<NumberAllocatorExhaustedError> for MultiplexError {
    fn from(_err: NumberAllocatorExhaustedError) -> Self {
        Self::PortsExhausted
    }
}

impl fmt::Display for MultiplexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::TransportSinkError(err) => write!(f, "Error sending multiplex message: {:?}", err),
            Self::TransportStreamError(err) => write!(f, "Error receiving multiplex message: {:?}", err),
            Self::TransportStreamClosed => write!(f, "Transport was closed."),
            Self::ProtocolError(err) => write!(f, "Multiplex protocol error: {}", err),
            Self::PortsExhausted => write!(f, "The maximum number of multiplex ports has been reached."),
            Self::SerializationError(err) => write!(f, "Error serializing multiplex message: {:?}", err),
            Self::DeserializationError(err) => write!(f, "Error deserializing multiplex message: {:?}", err),
        }
    }
}

impl Error for MultiplexError {}

/// Multiplexer configuration.
#[derive(Clone, Debug)]
pub struct Cfg {
    /// Receive queue length of each channel.
    pub channel_rx_queue_length: usize,
    /// Service request receive queue length.
    pub service_request_queue_length: usize,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg { channel_rx_queue_length: 100, service_request_queue_length: 10 }
    }
}

/// Message between two multiplexed endpoints.
#[derive(Debug, Serialize, Deserialize)]
pub enum MultiplexMsg<Content> {
    /// Open connection to service on specified client port.
    RequestService { service: Content, client_port: u32 },
    /// Connection accepted.
    Accepted { client_port: u32, server_port: u32 },
    /// Connection to service refused.
    Rejected { client_port: u32 },
    /// Pause sending data from specified port.
    Pause { port: u32 },
    /// Resume sending data from specified port.
    Resume { port: u32 },
    /// No more data will be sent to specifed remote port.
    Finish { port: u32 },
    /// Acknowledgement that Finish has been received for the specified remote port.
    FinishAck { port: u32 },
    /// Not interested on receiving any more data from specified remote port.
    Hangup { port: u32, gracefully: bool },
    /// Data for specified port.
    Data { port: u32, content: Content },
}

/// All necessary data to instantiate the user portion of a channel.
pub struct ChannelData<Content>
where
    Content: Send,
{
    pub local_port: u32,
    pub remote_port: u32,
    pub tx: mpsc::Sender<ChannelMsg<Content>>,
    pub tx_lock: ChannelSendLockRequester,
    pub rx_buffer: ChannelReceiverBufferDequeuer<Content>,
    pub hangup_notify: Arc<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
}

impl<Content> ChannelData<Content>
where
    Content: 'static + Send,
{
    pub fn instantiate(self) -> (RawSender<Content>, RawReceiver<Content>) {
        let ChannelData { local_port, remote_port, tx, tx_lock, rx_buffer, hangup_notify } = self;

        let sender = RawSender::new(local_port, remote_port, tx.clone(), tx_lock, hangup_notify);
        let receiver = RawReceiver::new(local_port, remote_port, tx, rx_buffer);
        (sender, receiver)
    }
}

/// Port state.
enum PortState<Content>
where
    Content: Send,
{
    /// Service request has been initiated locally and is waiting for reply
    /// from remote endpoint.
    LocalRequestingRemoteService {
        /// Channel for providing the response to the local requester.
        response_tx: oneshot::Sender<ConnectToRemoteServiceResponse<Content>>,
    },
    /// Service request has been received from remote endpoint and is
    /// awaiting local decision.
    RemoteRequestingLocalService {
        /// Remote port.
        remote_port: u32,
        /// Transmission lock authority.
        tx_lock: ChannelSendLockAuthority,
        /// Receive buffer enqueuer.
        rx_buffer: ChannelReceiverBufferEnqueuer<Content>,
        /// Notification senders for when Hangup message has been received.
        hangup_notify: Arc<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
    },
    /// Port is connected.
    Connected {
        /// Remote port.
        remote_port: u32,
        /// Transmission lock authority.
        /// Initially present, None when Hangup message has been received.
        tx_lock: Option<ChannelSendLockAuthority>,
        /// Receive buffer enqueuer.
        /// Initially present, None when Finish message has been received.
        rx_buffer: Option<ChannelReceiverBufferEnqueuer<Content>>,
        /// Notification senders for when Hangup message has been received.
        hangup_notify: Arc<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
        /// Pause request has been sent to remote endpoint.
        rx_paused: bool,
        /// Receiver has been dropped and Hangup message has been sent to remote endpoint.
        rx_hangedup: bool,
        /// Sender has been dropped, thus no more data can be send from this port to remote endpoint.
        tx_finished: bool,
        /// Finished message has been acknowledged by remote endpoint.
        tx_finished_ack: bool,
    },
}

/// Message from a channel to the multiplexer event loop.
pub enum ChannelMsg<Content> {
    /// Send message with content.
    SendMsg { remote_port: u32, content: Content },
    /// Sender has been dropped.
    SenderDropped { local_port: u32 },
    /// Receiver has been dropped.
    ReceiverClosed { local_port: u32, gracefully: bool },
    /// The channel receive buffer has reached the resume length from above.
    ReceiveBufferReachedResumeLength { local_port: u32 },
    /// Connection has been accepted.
    Accepted { local_port: u32 },
    /// Connection has been rejected.
    Rejected { local_port: u32 },
}

/// Multiplexer event loop event.
enum LoopEvent<Content>
where
    Content: Send,
{
    /// A message from a local channel.
    ChannelMsg(ChannelMsg<Content>),
    /// Received message from transport.
    ReceiveMsg(Result<MultiplexMsg<Content>, MultiplexError>),
    /// Transport stream is finished.
    TransportFinished,
    /// MultiplexClient has been dropped.
    ClientDropped,
    /// MultiplexServer has been dropped.
    ServerDropped,
    /// Connect to service with specified id on the remote side.
    ConnectToRemoteServiceRequest(ConnectToRemoteServiceRequest<Content>),
}

/// Channel multiplexer.
pub struct Multiplexer<Content, ContentCodec, TransportType, TransportCodec, TransportSink, TransportStream>
where
    Content: Send,
{
    /// Configuration
    cfg: Cfg,
    /// Codec factory for message content.
    content_codec: ContentCodec,
    /// Transport sink.
    transport_tx: Pin<Box<TransportSink>>,
    /// Channel to send server connection requests to.
    serve_tx: mpsc::Sender<(Content, RemoteConnectToServiceRequest<Content, ContentCodec>)>,
    /// Open local ports.
    ports: HashMap<u32, PortState<Content>>,
    /// Port number allocator.
    port_pool: NumberAllocator,
    /// Channel to send requests from user parts of sender/receiver to event loop.
    channel_tx: mpsc::Sender<ChannelMsg<Content>>,
    /// Receiver of event loop.
    event_rx: Pin<Box<dyn Stream<Item = LoopEvent<Content>> + Send>>,
    /// Serializer applied for transport.
    transport_serializer: Box<dyn Serializer<MultiplexMsg<Content>, TransportType>>,
    /// All user clients have been dropped.
    all_clients_dropped: bool,
    /// User server has been dropped.
    server_dropped: bool,
    _transport_codec_ghost: PhantomData<TransportCodec>,
    _transport_stream_ghost: PhantomData<TransportStream>,
}

/// Buffer size of internal communication channels.
const INTERNAL_CHANNEL_BUFFER: usize = 10;

impl<Content, ContentCodec, TransportType, TransportCodec, TransportSink, TransportStream> fmt::Debug
    for Multiplexer<Content, ContentCodec, TransportType, TransportCodec, TransportSink, TransportStream>
where
    Content: Send,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Multiplexer {{cfg={:?}}}", &self.cfg)
    }
}

impl<
        Content,
        ContentCodec,
        TransportType,
        TransportCodec,
        TransportSink,
        TransportStream,
        TransportSinkError,
        TransportStreamError,
    > Multiplexer<Content, ContentCodec, TransportType, TransportCodec, TransportSink, TransportStream>
where
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: CodecFactory<Content>,
    TransportType: Send + 'static,
    TransportCodec: CodecFactory<TransportType>,
    TransportSink: Sink<TransportType, Error = TransportSinkError> + Send + 'static,
    TransportStream: Stream<Item = Result<TransportType, TransportStreamError>> + Send + 'static,
    TransportSinkError: Error + Send + 'static,
    TransportStreamError: Error + Send + 'static,
{
    /// Creates a new multiplexer.
    /// 
    /// See the `codecs` module for provided transport and content codecs.
    /// 
    /// After creation use the `run` method of the multiplexer to launch the dispatch task.
    pub fn new<Service>(
        cfg: &Cfg, content_codec: &ContentCodec, transport_codec: &TransportCodec, transport_tx: TransportSink,
        transport_rx: TransportStream,
    ) -> (
        Multiplexer<Content, ContentCodec, TransportType, TransportCodec, TransportSink, TransportStream>,
        Client<Service, Content, ContentCodec>,
        Server<Service, Content, ContentCodec>,
    )
    where
        Service: Serialize + DeserializeOwned + 'static,
    {
        // Check configuration.
        assert!(cfg.channel_rx_queue_length >= 2, "Channel receive queue length must be at least 2.");
        assert!(cfg.service_request_queue_length >= 1, "Service request queue length must be at least 1.");

        // Create serializer and deserializer for message transport encoding.
        let transport_serializer = transport_codec.serializer();
        let transport_deserializer = transport_codec.deserializer();

        // Create internal communication channels.
        let (channel_tx, channel_rx) = mpsc::channel(INTERNAL_CHANNEL_BUFFER);
        let (connect_tx, connect_rx) = mpsc::channel(INTERNAL_CHANNEL_BUFFER);
        let (serve_tx, serve_rx) = mpsc::channel(cfg.service_request_queue_length);
        let (server_drop_tx, server_drop_rx) = mpsc::channel(INTERNAL_CHANNEL_BUFFER);

        // Decode received messages.
        let transport_rx = transport_rx
            .map(move |msg| match msg {
                Ok(wire) => match transport_deserializer.deserialize(wire) {
                    Ok(msg) => LoopEvent::ReceiveMsg(Ok(msg)),
                    Err(err) => LoopEvent::ReceiveMsg(Err(MultiplexError::DeserializationError(err))),
                },
                Err(err) => LoopEvent::ReceiveMsg(Err(MultiplexError::TransportStreamError(Box::new(err)))),
            })
            .chain(stream::once(async { LoopEvent::TransportFinished }));

        // Create event loop stream.
        let channel_rx = channel_rx.map(LoopEvent::ChannelMsg);
        let connect_rx = connect_rx
            .map(LoopEvent::ConnectToRemoteServiceRequest)
            .chain(stream::once(async { LoopEvent::ClientDropped }));
        let server_drop_rx = server_drop_rx.map(|()| LoopEvent::ServerDropped);
        let event_rx =
            stream::select(transport_rx, stream::select(channel_rx, stream::select(connect_rx, server_drop_rx)));

        // Create user objects.
        let multiplexer = Multiplexer {
            cfg: cfg.clone(),
            content_codec: content_codec.clone(),
            transport_tx: Box::pin(transport_tx),
            serve_tx,
            ports: HashMap::new(),
            port_pool: NumberAllocator::new(),
            channel_tx,
            event_rx: Box::pin(event_rx),
            transport_serializer,
            all_clients_dropped: false,
            server_dropped: false,
            _transport_codec_ghost: PhantomData,
            _transport_stream_ghost: PhantomData,
        };
        let client = Client::new(connect_tx, &multiplexer.content_codec);
        let server = Server::new(serve_rx, server_drop_tx, &multiplexer.content_codec);

        (multiplexer, client, server)
    }

    fn make_channel_receiver_buffer_cfg(&self, local_port: u32) -> ChannelReceiverBufferCfg<Content> {
        ChannelReceiverBufferCfg {
            resume_length: self.cfg.channel_rx_queue_length / 2,
            pause_length: self.cfg.channel_rx_queue_length,
            block_length: self.cfg.channel_rx_queue_length * 2,
            resume_notify_tx: self.channel_tx.clone(),
            local_port,
        }
    }

    async fn transport_send(&mut self, msg: MultiplexMsg<Content>) -> Result<(), MultiplexError> {
        let content = self.transport_serializer.serialize(msg).map_err(MultiplexError::SerializationError)?;
        self.transport_tx.send(content).await.map_err(|err| MultiplexError::TransportSinkError(Box::new(err)))
    }

    fn should_terminate(&self) -> bool {
        self.all_clients_dropped && self.server_dropped && self.ports.is_empty()
    }

    fn maybe_free_port(&mut self, local_port: u32) -> bool {
        let free = if let Some(PortState::Connected {
            tx_lock,
            rx_buffer,
            rx_hangedup,
            tx_finished,
            tx_finished_ack,
            ..
        }) = self.ports.get(&local_port)
        {
            tx_lock.is_none() && rx_buffer.is_none() && *rx_hangedup && *tx_finished && *tx_finished_ack
        } else {
            panic!("maybe_free_port called for port {} not in connected state.", &local_port);
        };
        if free {
            self.ports.remove(&local_port);
        }
        self.should_terminate()
    }

    /// Runs the multiplexer dispatcher.
    /// 
    /// The dispatches terminates when the client, server and all channels have been dropped or
    /// the transport is closed. 
    pub async fn run(mut self) -> Result<(), MultiplexError> {
        loop {
            match self.event_rx.next().await {
                // Send data from channel.
                Some(LoopEvent::ChannelMsg(ChannelMsg::SendMsg { remote_port, content })) => {
                    self.transport_send(MultiplexMsg::Data { port: remote_port, content }).await?;
                }

                // Local channel sender has been dropped.
                Some(LoopEvent::ChannelMsg(ChannelMsg::SenderDropped { local_port })) => {
                    if let Some(PortState::Connected { remote_port, tx_finished, .. }) =
                        self.ports.get_mut(&local_port)
                    {
                        if *tx_finished {
                            panic!("ChannelMsg SenderDropped more than once for port {}.", &local_port);
                        }
                        *tx_finished = true;
                        let msg = MultiplexMsg::Finish { port: *remote_port };
                        self.transport_send(msg).await?;
                        if self.maybe_free_port(local_port) {
                            break;
                        }
                    } else {
                        panic!("ChannelMsg SenderDropped for non-connected port {}.", &local_port);
                    }
                }

                // Local channel receiver has been closed.
                Some(LoopEvent::ChannelMsg(ChannelMsg::ReceiverClosed { local_port, gracefully })) => {
                    if let Some(PortState::Connected { remote_port, rx_hangedup, .. }) =
                        self.ports.get_mut(&local_port)
                    {
                        if *rx_hangedup {
                            panic!("ChannelMsg ReceiverClosed more than once for port {}.", &local_port);
                        }
                        *rx_hangedup = true;
                        let msg = MultiplexMsg::Hangup { port: *remote_port, gracefully };
                        self.transport_send(msg).await?;
                        if self.maybe_free_port(local_port) {
                            break;
                        }
                    } else {
                        panic!("ChannelMsg ReceiverClosed for non-connected port {}.", &local_port);
                    }
                }

                // Receive buffer reached resume threshold.
                Some(LoopEvent::ChannelMsg(ChannelMsg::ReceiveBufferReachedResumeLength { local_port })) => {
                    if let Some(PortState::Connected { remote_port, rx_paused, rx_buffer, .. }) =
                        self.ports.get_mut(&local_port)
                    {
                        // Send resume message only, when no Finish message has been received.
                        if let Some(rx_buffer) = rx_buffer.as_ref() {
                            if *rx_paused && rx_buffer.resumeable().await {
                                *rx_paused = false;
                                let msg = MultiplexMsg::Resume { port: *remote_port };
                                self.transport_send(msg).await?;
                            }
                        } else {
                            panic!(
                                "ChannelMsg ReceiveBufferReachedResumeLength for already finished port {}.",
                                &local_port
                            );
                        }
                    } else {
                        panic!(
                            "ChannelMsg ReceiveBufferReachedResumeLength for non-connected port {}.",
                            &local_port
                        );
                    }
                }

                // Remote service request was accepted by local.
                Some(LoopEvent::ChannelMsg(ChannelMsg::Accepted { local_port })) => {
                    if let Some(PortState::RemoteRequestingLocalService {
                        remote_port,
                        tx_lock,
                        rx_buffer,
                        hangup_notify,
                    }) = self.ports.remove(&local_port)
                    {
                        self.ports.insert(
                            local_port,
                            PortState::Connected {
                                remote_port,
                                tx_lock: Some(tx_lock),
                                rx_buffer: Some(rx_buffer),
                                hangup_notify,
                                rx_paused: false,
                                rx_hangedup: false,
                                tx_finished: false,
                                tx_finished_ack: false,
                            },
                        );
                        self.transport_send(MultiplexMsg::Accepted {
                            client_port: remote_port,
                            server_port: local_port,
                        })
                        .await?;
                    } else {
                        panic!("ChannelMsg Accepted for non-requesting port {}.", &local_port);
                    }
                }

                // Remote service request was rejected by local.
                Some(LoopEvent::ChannelMsg(ChannelMsg::Rejected { local_port })) => {
                    if let Some(PortState::RemoteRequestingLocalService { remote_port, .. }) =
                        self.ports.remove(&local_port)
                    {
                        self.port_pool.release(local_port);
                        self.transport_send(MultiplexMsg::Rejected { client_port: remote_port }).await?;
                    } else {
                        panic!("ChannelMsg Rejected for non-requesting port {}.", &local_port);
                    }
                }

                // Process request service message from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::RequestService { service, client_port }))) => {
                    let server_port = self.port_pool.allocate()?;
                    let (tx_lock_authority, tx_lock_requester) = channel_send_lock();
                    let (rx_buffer_enqueuer, rx_buffer_dequeuer) =
                        self.make_channel_receiver_buffer_cfg(server_port).instantiate();
                    let hangup_notify = Arc::new(Mutex::new(Some(Vec::new())));
                    self.ports.insert(
                        server_port,
                        PortState::RemoteRequestingLocalService {
                            remote_port: client_port,
                            tx_lock: tx_lock_authority,
                            rx_buffer: rx_buffer_enqueuer,
                            hangup_notify: hangup_notify.clone(),
                        },
                    );
                    let req = RemoteConnectToServiceRequest::new(
                        ChannelData {
                            local_port: server_port,
                            remote_port: client_port,
                            tx: self.channel_tx.clone(),
                            tx_lock: tx_lock_requester,
                            rx_buffer: rx_buffer_dequeuer,
                            hangup_notify,
                        },
                        &self.content_codec,
                    );
                    let _ = self.serve_tx.send((service, req)).await;
                }

                // Process accepted service request message from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Accepted { client_port, server_port }))) => {
                    if let Some(PortState::LocalRequestingRemoteService { response_tx }) =
                        self.ports.remove(&client_port)
                    {
                        let (tx_lock_authority, tx_lock_requester) = channel_send_lock();
                        let (rx_buffer_enqueuer, rx_buffer_dequeuer) =
                            self.make_channel_receiver_buffer_cfg(client_port).instantiate();
                        let hangup_notify = Arc::new(Mutex::new(Some(Vec::new())));
                        self.ports.insert(
                            client_port,
                            PortState::Connected {
                                remote_port: server_port,
                                tx_lock: Some(tx_lock_authority),
                                rx_buffer: Some(rx_buffer_enqueuer),
                                hangup_notify: hangup_notify.clone(),
                                rx_paused: false,
                                rx_hangedup: false,
                                tx_finished: false,
                                tx_finished_ack: false,
                            },
                        );
                        let channel_data = ChannelData {
                            local_port: client_port,
                            remote_port: server_port,
                            tx: self.channel_tx.clone(),
                            tx_lock: tx_lock_requester,
                            rx_buffer: rx_buffer_dequeuer,
                            hangup_notify,
                        };
                        let (raw_sender, raw_receiver) = channel_data.instantiate();
                        let _ =
                            response_tx.send(ConnectToRemoteServiceResponse::Accepted(raw_sender, raw_receiver));
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received accepted message for port {} not in LocalRequestingRemoteService state.",
                            client_port
                        )));
                    }
                }

                // Process rejected service request message from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Rejected { client_port }))) => {
                    if let Some(PortState::LocalRequestingRemoteService { response_tx }) =
                        self.ports.remove(&client_port)
                    {
                        self.port_pool.release(client_port);
                        let _ = response_tx.send(ConnectToRemoteServiceResponse::Rejected);
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received rejected message for port {} not in LocalRequestingRemoteService state.",
                            client_port
                        )));
                    }
                }

                // Process pause sending data request from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Pause { port }))) => {
                    if let Some(PortState::Connected { tx_lock, .. }) = self.ports.get_mut(&port) {
                        // Ignore request after receiving hangup message, since no more data will be transmitted anyway.
                        if let Some(tx_lock) = tx_lock.as_mut() {
                            tx_lock.pause().await;
                        }
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received pause message for port {} not in Connected state.",
                            &port
                        )));
                    }
                }

                // Process resume sending data request from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Resume { port }))) => {
                    if let Some(PortState::Connected { tx_lock, .. }) = self.ports.get_mut(&port) {
                        // Ignore request after receiving hangup message, since no more data will be transmitted anyway.
                        if let Some(tx_lock) = tx_lock.as_mut() {
                            tx_lock.resume().await;
                        }
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received resume message for port {} not in Connected state.",
                            &port
                        )));
                    }
                }

                // Process finish information from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Finish { port }))) => {
                    if let Some(PortState::Connected { rx_buffer, remote_port, .. }) = self.ports.get_mut(&port) {
                        if let Some(rx_buffer) = rx_buffer.take() {
                            rx_buffer.close().await;
                            let msg = MultiplexMsg::FinishAck { port: *remote_port };
                            self.transport_send(msg).await?;
                            if self.maybe_free_port(port) {
                                break;
                            }
                        } else {
                            return Err(MultiplexError::ProtocolError(format!(
                                "Received Finish message for port {} more than once.",
                                &port
                            )));
                        }
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received Finish message for port {} not in Connected state.",
                            &port
                        )));
                    }
                }

                // Process acknowledgement that our finish message has been received by remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::FinishAck { port }))) => {
                    if let Some(PortState::Connected { tx_finished_ack, .. }) = self.ports.get_mut(&port) {
                        *tx_finished_ack = true;
                        if self.maybe_free_port(port) {
                            break;
                        }
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received FinishAck message for port {} not in Connected state.",
                            &port
                        )));
                    }
                }

                // Process hangup information from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Hangup { port, gracefully }))) => {
                    if let Some(PortState::Connected { tx_lock, hangup_notify, .. }) = self.ports.get_mut(&port) {
                        if let Some(tx_lock) = tx_lock.take() {
                            tx_lock.close(gracefully).await;

                            // Send hangup notifications.
                            let notifies = hangup_notify.lock().unwrap().take().unwrap();
                            for tx in notifies {
                                let _ = tx.send(());
                            }

                            if self.maybe_free_port(port) {
                                break;
                            }
                        } else {
                            return Err(MultiplexError::ProtocolError(format!(
                                "Received more than one Hangup message for port {}.",
                                &port
                            )));
                        }
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received Hangup message for port {} not in Connected state.",
                            &port
                        )));
                    }
                }

                // Process data from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Data { port, content }))) => {
                    if let Some(PortState::Connected { rx_buffer, rx_paused, remote_port, .. }) =
                        self.ports.get_mut(&port)
                    {
                        if let Some(rx_buffer) = rx_buffer.as_mut() {
                            let pause_required = rx_buffer.enqueue(content).await;
                            if pause_required && !*rx_paused {
                                let msg = MultiplexMsg::Pause { port: *remote_port };
                                *rx_paused = true;
                                self.transport_send(msg).await?;
                            }
                        } else {
                            return Err(MultiplexError::ProtocolError(format!(
                                "Received data after finish for port {}.",
                                &port
                            )));
                        }
                    } else {
                        return Err(MultiplexError::ProtocolError(format!(
                            "Received data for non-connected port {}.",
                            &port
                        )));
                    }
                }

                // Process receive error.
                Some(LoopEvent::ReceiveMsg(Err(rx_err))) => {
                    return Err(rx_err);
                }

                // Process local request to connect to remote service.
                Some(LoopEvent::ConnectToRemoteServiceRequest(ConnectToRemoteServiceRequest {
                    service,
                    response_tx,
                })) => {
                    let client_port = self.port_pool.allocate()?;
                    self.ports.insert(client_port, PortState::LocalRequestingRemoteService { response_tx });
                    self.transport_send(MultiplexMsg::RequestService { service, client_port }).await?;
                }

                // Process that client has been dropped.
                Some(LoopEvent::ClientDropped) => {
                    self.all_clients_dropped = true;
                    if self.should_terminate() {
                        break;
                    }
                }

                // Process that server has been dropped.
                Some(LoopEvent::ServerDropped) => {
                    self.server_dropped = true;
                    if self.should_terminate() {
                        break;
                    }
                }

                // Transport stream has been closed.
                Some(LoopEvent::TransportFinished) => {
                    // If client is dropped and no multiplex connections are active,
                    // treat transport closure as normal disconnection of remote
                    // endpoint.
                    if self.all_clients_dropped && self.ports.is_empty() {
                        break;
                    } else {
                        return Err(MultiplexError::TransportStreamClosed);
                    }
                }

                // All event streams have ended.
                None => panic!("Unexpected end of event stream."),
            }
        }
        Ok(())
    }
}
