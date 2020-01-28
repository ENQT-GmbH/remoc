use futures::channel::{oneshot, mpsc};
use futures::stream::{self, Stream, StreamExt};
use futures::sink::{Sink, SinkExt};
use std::fmt;
use std::pin::Pin;
use std::collections::HashMap;
use std::marker::PhantomData;
use serde::{Serialize, Deserialize};

use crate::number_allocator::{NumberAllocator, NumberAllocatorExhaustedError};
use crate::send_lock::{ChannelSendLockAuthority, ChannelSendLockRequester, channel_send_lock};
use crate::receive_buffer::{ChannelReceiverBufferEnqueuer, ChannelReceiverBufferDequeuer, ChannelReceiverBufferCfg};
use crate::sender::RawSender;
use crate::receiver::RawReceiver;
use crate::server::{Server, RemoteConnectToServiceRequest};
use crate::client::{Client, ConnectToRemoteServiceRequest, ConnectToRemoteServiceResponse};
use crate::codec::CodecFactory;

/// Channel multiplexer error.
#[derive(Debug, Clone)]
pub enum MultiplexRunError<TransportSinkError, TransportStreamError> {
    /// An error was encountered while sending data to the transport sink.
    TransportSinkError(TransportSinkError),
    /// An error was encountered while receiving data from the transport stream.
    TransportStreamError(TransportStreamError),
    /// The transport stream was close while multiplex channels were active or the 
    /// multiplex client was not dropped.
    TransportStreamClosed,
    /// A multiplex protocol error occured.
    ProtocolError(String),
    /// The multiplexer ports were exhausted.
    PortsExhausted
}

impl<TransportSinkError, TransportStreamError> From<NumberAllocatorExhaustedError> for MultiplexRunError<TransportSinkError, TransportStreamError> {
    fn from(_err: NumberAllocatorExhaustedError) -> Self {
        Self::PortsExhausted
    }
}

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
        Cfg {
            channel_rx_queue_length: 100,
            service_request_queue_length: 10,
        }
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
    Data {
        port: u32,
        content: Content,
    },
}


pub struct ChannelData<Content> where Content: Send {
    pub local_port: u32,
    pub remote_port: u32,
    pub tx: mpsc::Sender<ChannelMsg<Content>>,
    pub tx_lock: ChannelSendLockRequester,
    pub rx_buffer: ChannelReceiverBufferDequeuer<Content>,
}

impl<Content> ChannelData<Content> where Content: 'static + Send {
    pub fn instantiate(self) -> (RawSender<Content>, RawReceiver<Content>) {
        let ChannelData {
            local_port, remote_port, tx, tx_lock, rx_buffer
        } = self;
        
        let sender = RawSender::new(
            local_port,
            remote_port,
            tx.clone(),
            tx_lock
        );
        let receiver = RawReceiver::new(
            local_port,
            remote_port,
            tx,
            rx_buffer
        );
        (sender, receiver)
    }
}



/// Port state.
enum PortState<Content> where Content: Send {
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


pub enum ChannelMsg<Content> {
    /// Send message with content.
    SendMsg {
        remote_port: u32,
        content: Content,
    },
    /// Sender has been dropped.
    SenderDropped { local_port: u32 },
    /// Receiver has been dropped.
    ReceiverClosed { local_port: u32, gracefully: bool },
    /// The channel receive buffer has reached the resume length from above.
    ReceiveBufferReachedResumeLength {local_port: u32},
    /// Connection has been accepted.
    Accepted {local_port: u32},
    /// Connection has been rejected.
    Rejected {local_port: u32 },
}

enum LoopEvent<Content, TransportStreamError> where Content: Send {
    /// A message from a local channel.
    ChannelMsg (ChannelMsg<Content>),
    /// Received message from transport.
    ReceiveMsg(Result<MultiplexMsg<Content>, TransportStreamError>),
    /// Transport stream is finished.
    TransportFinished,
    /// MultiplexClient has been dropped.
    ClientDropped,
    /// MultiplexServer has been dropped.
    ServerDropped,
    /// Connect to service with specified id on the remote side.
    ConnectToRemoteServiceRequest (ConnectToRemoteServiceRequest<Content>),
}

/// Channel multiplexer.
pub struct Multiplexer<Content, Codec, TransportSink, TransportStream, TransportSinkError, TransportStreamError> where Content: Send {
    cfg: Cfg,
    codec_factory: Codec,
    transport_tx: Pin<Box<TransportSink>>,
    serve_tx: mpsc::Sender<(Content, RemoteConnectToServiceRequest<Content, Codec>)>,

    ports: HashMap<u32, PortState<Content>>,
    port_pool: NumberAllocator,
    channel_tx: mpsc::Sender<ChannelMsg<Content>>,
    event_rx: Pin<Box<dyn Stream<Item=LoopEvent<Content, TransportStreamError>> + Send>>,

    client_dropped: bool,
    server_dropped: bool,

    _transport_stream_ghost: PhantomData<TransportStream>,
    _transport_sink_error_ghost: PhantomData<TransportSinkError>,
}

impl<Content, Codec, TransportSink, TransportStream, TransportSinkError, TransportStreamError> fmt::Debug for Multiplexer<Content, Codec, TransportSink, TransportStream, TransportSinkError, TransportStreamError> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Multiplexer {{cfg={:?}}}", &self.cfg)
    }
}

impl<Content, Codec, TransportSink, TransportStream, TransportSinkError, TransportStreamError>
    Multiplexer<Content, Codec, TransportSink, TransportStream, TransportSinkError, TransportStreamError>
where     
    Content: Send + 'static,
    Codec: CodecFactory<Content>,
    TransportSink: Sink<MultiplexMsg<Content>, Error = TransportSinkError> + Send + 'static,
    TransportStream: Stream<Item = Result<MultiplexMsg<Content>, TransportStreamError>> + Send + 'static,
    TransportSinkError: 'static,
    TransportStreamError: 'static,
{
    pub fn new<Service>(cfg: Cfg, codec_factory: Codec, transport_tx: TransportSink, transport_rx: TransportStream) -> 
        (Multiplexer<Content, Codec, TransportSink, TransportStream, TransportSinkError, TransportStreamError>,
         Client<Service, Content, Codec>, Server<Service, Content, Codec>)
    {
        // Check configuration.
        assert!(
            cfg.channel_rx_queue_length >= 2,
            "Channel receive queue length must be at least 2."
        );
        assert!(cfg.service_request_queue_length >= 1,
            "Service request queue length must be at least 1.");

        // Create internal communication channels.
        let (channel_tx, channel_rx) = mpsc::channel(1);
        let (connect_tx, connect_rx) = mpsc::channel(1);
        let (serve_tx, serve_rx) = mpsc::channel(cfg.service_request_queue_length);
        let (server_drop_tx, server_drop_rx) = mpsc::channel(1);

        // Create event loop stream.
        let transport_rx = transport_rx.map(LoopEvent::ReceiveMsg).chain(stream::once(async {LoopEvent::TransportFinished}));
        let channel_rx = channel_rx.map(LoopEvent::ChannelMsg);
        let connect_rx = connect_rx.map(LoopEvent::ConnectToRemoteServiceRequest).chain(stream::once(async {LoopEvent::ClientDropped}));
        let server_drop_rx = server_drop_rx.map(|()| LoopEvent::ServerDropped);
        let event_rx = stream::select(transport_rx, stream::select(channel_rx, stream::select(connect_rx, server_drop_rx)));
        
        // Create user objects.
        let multiplexer = Multiplexer {
            cfg,
            codec_factory,
            transport_tx: Box::pin(transport_tx),
            serve_tx,
            ports: HashMap::new(),
            port_pool: NumberAllocator::new(),
            channel_tx,
            event_rx: Box::pin(event_rx),
            client_dropped: false,
            server_dropped: false,
            _transport_stream_ghost: PhantomData,
            _transport_sink_error_ghost: PhantomData,
        };
        let client = Client::new(connect_tx, &multiplexer.codec_factory);
        let server = Server::new(serve_rx, server_drop_tx, &multiplexer.codec_factory);

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

    async fn transport_send(&mut self, msg: MultiplexMsg<Content>) -> Result<(), MultiplexRunError<TransportSinkError, TransportStreamError>> {
        self.transport_tx.send(msg).await.map_err(MultiplexRunError::TransportSinkError)        
    }

    fn should_terminate(&self) -> bool {
        self.client_dropped && self.server_dropped && self.ports.is_empty()
    }

    fn maybe_free_port(&mut self, local_port: u32) -> bool {
        let free =
            if let Some(PortState::Connected {tx_lock, rx_buffer, rx_hangedup, tx_finished, tx_finished_ack, ..}) = self.ports.get(&local_port) {
                tx_lock.is_none() && rx_buffer.is_none() && *rx_hangedup && *tx_finished && *tx_finished_ack
            } else {
                panic!("maybe_free_port called for port {} not in connected state.", &local_port);
            };
        if free {
            self.ports.remove(&local_port);
        }
        self.should_terminate()
    }

    pub async fn run(mut self) -> Result<(), MultiplexRunError<TransportSinkError, TransportStreamError>> {
        loop {
            match self.event_rx.next().await {
                // Send data from channel.
                Some(LoopEvent::ChannelMsg(ChannelMsg::SendMsg {remote_port, content})) => {
                    self.transport_send(MultiplexMsg::Data {port: remote_port, content}).await?;
                }

                // Local channel sender has been dropped.
                Some(LoopEvent::ChannelMsg(ChannelMsg::SenderDropped {local_port})) => {
                    if let Some(PortState::Connected {remote_port, tx_finished, ..}) = self.ports.get_mut(&local_port) {
                        if *tx_finished {
                            panic!("ChannelMsg SenderDropped more than once for port {}.", &local_port);
                        }
                        *tx_finished = true;
                        let msg = MultiplexMsg::Finish {port: *remote_port};
                        self.transport_send(msg).await?;
                        if self.maybe_free_port(local_port) { break }
                    } else {
                        panic!("ChannelMsg SenderDropped for non-connected port {}.", &local_port);
                    }
                }

                // Local channel receiver has been closed.
                Some(LoopEvent::ChannelMsg(ChannelMsg::ReceiverClosed {local_port, gracefully})) => {
                    if let Some(PortState::Connected {remote_port, rx_hangedup, ..}) = self.ports.get_mut(&local_port) {
                        if *rx_hangedup {
                            panic!("ChannelMsg ReceiverClosed more than once for port {}.", &local_port);
                        }
                        *rx_hangedup = true;
                        let msg = MultiplexMsg::Hangup {port: *remote_port, gracefully};
                        self.transport_send(msg).await?; 
                        if self.maybe_free_port(local_port) { break }
                    } else {
                        panic!("ChannelMsg ReceiverClosed for non-connected port {}.", &local_port);
                    }
                }

                // Receive buffer reached resume threshold.
                Some(LoopEvent::ChannelMsg(ChannelMsg::ReceiveBufferReachedResumeLength {local_port})) => {
                    if let Some(PortState::Connected {remote_port, rx_paused, rx_buffer, ..}) = self.ports.get_mut(&local_port) {
                        // Send resume message only, when no Finish message has been received.
                        if let Some(rx_buffer) = rx_buffer.as_ref() {
                            if *rx_paused && rx_buffer.resumeable().await {
                                *rx_paused = false;
                                let msg = MultiplexMsg::Resume {port: *remote_port};
                                self.transport_send(msg).await?;                    
                            }
                        } else {
                            panic!("ChannelMsg ReceiveBufferReachedResumeLength for already finished port {}.", &local_port);    
                        }
                    } else {
                        panic!("ChannelMsg ReceiveBufferReachedResumeLength for non-connected port {}.", &local_port);
                    }
                }

                // Remote service request was accepted by local.
                Some(LoopEvent::ChannelMsg(ChannelMsg::Accepted {local_port})) => {
                    if let Some(PortState::RemoteRequestingLocalService {remote_port, tx_lock, rx_buffer}) = self.ports.remove(&local_port) {
                        self.ports.insert(local_port, PortState::Connected {
                            remote_port,
                            tx_lock: Some(tx_lock),
                            rx_buffer: Some(rx_buffer),
                            rx_paused: false,
                            rx_hangedup: false,
                            tx_finished: false,
                            tx_finished_ack: false,
                        });
                        self.transport_send(MultiplexMsg::Accepted {client_port: remote_port, server_port: local_port}).await?;    
                    } else {
                        panic!("ChannelMsg Accepted for non-requesting port {}.", &local_port);
                    }
                }

                // Remote service request was rejected by local.
                Some(LoopEvent::ChannelMsg(ChannelMsg::Rejected {local_port})) => {
                    if let Some(PortState::RemoteRequestingLocalService {remote_port, ..}) = self.ports.remove(&local_port) {
                        self.port_pool.release(local_port);
                        self.transport_send(MultiplexMsg::Rejected {client_port: remote_port}).await?;
                    } else {
                        panic!("ChannelMsg Rejected for non-requesting port {}.", &local_port);
                    }
                }
                
                // Process request service message from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::RequestService {service, client_port}))) => {
                    let server_port = self.port_pool.allocate()?;
                    let (tx_lock_authority, tx_lock_requester) = channel_send_lock();
                    let (rx_buffer_enqueuer, rx_buffer_dequeuer) = self.make_channel_receiver_buffer_cfg(server_port).instantiate();
                    self.ports.insert(server_port, PortState::RemoteRequestingLocalService {
                        remote_port: client_port,
                        tx_lock: tx_lock_authority,
                        rx_buffer: rx_buffer_enqueuer
                    });
                    let req = RemoteConnectToServiceRequest::new(ChannelData {
                        local_port: server_port,
                        remote_port: client_port,
                        tx: self.channel_tx.clone(),
                        tx_lock: tx_lock_requester,
                        rx_buffer: rx_buffer_dequeuer,
                    }, &self.codec_factory);
                    let _ = self.serve_tx.send((service, req)).await;
                },

                // Process accepted service request message from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Accepted {client_port, server_port}))) => {
                    if let Some(PortState::LocalRequestingRemoteService {response_tx}) = self.ports.remove(&client_port) {
                        let (tx_lock_authority, tx_lock_requester) = channel_send_lock();
                        let (rx_buffer_enqueuer, rx_buffer_dequeuer) = self.make_channel_receiver_buffer_cfg(client_port).instantiate();                        
                        self.ports.insert(client_port, PortState::Connected {
                            remote_port: server_port,
                            tx_lock: Some(tx_lock_authority),
                            rx_buffer: Some(rx_buffer_enqueuer),
                            rx_paused: false,
                            rx_hangedup: false,
                            tx_finished: false,
                            tx_finished_ack: false,
                        });
                        let channel_data = ChannelData {
                            local_port: client_port,
                            remote_port: server_port,
                            tx: self.channel_tx.clone(),
                            tx_lock: tx_lock_requester,
                            rx_buffer: rx_buffer_dequeuer,
                        };
                        let (raw_sender, raw_receiver) = channel_data.instantiate();
                        let _ = response_tx.send(ConnectToRemoteServiceResponse::Accepted (raw_sender, raw_receiver)); 
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received accepted message for port {} not in LocalRequestingRemoteService state.", client_port)));
                    }                
                }

                // Process rejected service request message from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Rejected {client_port}))) => {
                    if let Some(PortState::LocalRequestingRemoteService {response_tx}) = self.ports.remove(&client_port) {
                        self.port_pool.release(client_port);
                        let _ = response_tx.send(ConnectToRemoteServiceResponse::Rejected);
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received rejected message for port {} not in LocalRequestingRemoteService state.", client_port)));
                    }    
                }

                // Process pause sending data request from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Pause {port}))) => {
                    if let Some(PortState::Connected {tx_lock, ..}) = self.ports.get_mut(&port) {
                        // Ignore request after receiving hangup message, since no more data will be transmitted anyway.
                        if let Some(tx_lock) = tx_lock.as_mut() {
                            tx_lock.pause().await;
                        }
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received pause message for port {} not in Connected state.", &port)));
                    }    
                }

                // Process resume sending data request from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Resume {port}))) => {
                    if let Some(PortState::Connected {tx_lock, ..}) = self.ports.get_mut(&port) {
                        // Ignore request after receiving hangup message, since no more data will be transmitted anyway.
                        if let Some(tx_lock) = tx_lock.as_mut() {
                            tx_lock.resume().await;
                        }
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received resume message for port {} not in Connected state.", &port)));
                    }    
                }

                // Process finish information from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Finish {port}))) => {
                    if let Some(PortState::Connected {rx_buffer, remote_port, ..}) = self.ports.get_mut(&port) {
                        if let Some(rx_buffer) = rx_buffer.take() {
                            rx_buffer.close().await;
                            let msg = MultiplexMsg::FinishAck {port: *remote_port};
                            self.transport_send(msg).await?;        
                            if self.maybe_free_port(port) { break }
                        } else {
                            return Err(MultiplexRunError::ProtocolError(format!("Received Finish message for port {} more than once.", &port)));
                        }
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received Finish message for port {} not in Connected state.", &port)));
                    } 
                }

                // Process acknowledgement that our finish message has been received by remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::FinishAck {port}))) => {       
                    if let Some(PortState::Connected {tx_finished_ack, ..}) = self.ports.get_mut(&port) {
                        *tx_finished_ack = true;
                        if self.maybe_free_port(port) { break }
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received FinishAck message for port {} not in Connected state.", &port)));
                    } 
                }

                // Process hangup information from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Hangup {port, gracefully}))) => {                    
                    if let Some(PortState::Connected {tx_lock, ..}) = self.ports.get_mut(&port) {
                        if let Some(tx_lock) = tx_lock.take() {
                            tx_lock.close(gracefully).await;  
                            if self.maybe_free_port(port) { break }
                        } else {
                            return Err(MultiplexRunError::ProtocolError(format!("Received more than one Hangup message for port {}.", &port)));
                        }
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received Hangup message for port {} not in Connected state.", &port)));
                    } 
                }

                // Process data from remote endpoint.
                Some(LoopEvent::ReceiveMsg(Ok(MultiplexMsg::Data {port, content}))) => {  
                    if let Some(PortState::Connected {rx_buffer, rx_paused, remote_port, ..}) = self.ports.get_mut(&port) {
                        if let Some(rx_buffer) = rx_buffer.as_mut() {
                            let pause_required = rx_buffer.enqueue(content).await;
                            if pause_required && !*rx_paused {
                                let msg = MultiplexMsg::Pause {port: *remote_port};
                                *rx_paused = true;
                                self.transport_send(msg).await?;
                            }
                        } else {
                            return Err(MultiplexRunError::ProtocolError(format!("Received data after finish for port {}.", &port)));
                        }
                    } else {
                        return Err(MultiplexRunError::ProtocolError(format!("Received data for non-connected port {}.", &port)));
                    }
                }

                // Process receive error.
                Some(LoopEvent::ReceiveMsg(Err(rx_err))) => {
                    return Err(MultiplexRunError::TransportStreamError(rx_err));
                }

                // Process local request to connect to remote service.
                Some(LoopEvent::ConnectToRemoteServiceRequest (ConnectToRemoteServiceRequest {service, response_tx})) => {
                    let client_port = self.port_pool.allocate()?;
                    self.ports.insert(client_port, PortState::LocalRequestingRemoteService {response_tx});
                    self.transport_send(MultiplexMsg::RequestService {service, client_port}).await?;
                }

                // Process that client has been dropped.
                Some(LoopEvent::ClientDropped) => {
                    self.client_dropped = true;
                    if self.should_terminate() { break }
                }

                // Process that server has been dropped.
                Some(LoopEvent::ServerDropped) => {
                    self.server_dropped = true;
                    if self.should_terminate() { break }
                }

                // Transport stream has been closed.
                Some(LoopEvent::TransportFinished) => {
                    // If client is dropped and no multiplex connections are active,
                    // treat transport closure as normal disconnection of remote
                    // endpoint.
                    if self.client_dropped && self.ports.is_empty() {
                        break
                    } else {
                        return Err(MultiplexRunError::TransportStreamClosed)
                    }
                }

                // All event streams have ended.
                None => break
            }
        }
        Ok(())
    }
}
