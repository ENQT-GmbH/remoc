use std::{
    num::{NonZeroU16, NonZeroU32, NonZeroUsize},
    time::Duration,
};

/// Behavior when ports are exhausted and a connect is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PortsExhausted {
    /// Immediately fail connect request.
    Fail,
    /// Wait for a port to become available with an optional timeout.
    Wait(Option<Duration>),
}

/// Multiplexer configuration.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cfg {
    /// Identifier for trace logging.
    pub trace_id: Option<String>,
    /// Time after which connection is closed when no data is.
    ///
    /// Pings are send automatically when this is enabled and no data is transmitted.
    /// By default this is 60 seconds.
    pub connection_timeout: Option<Duration>,
    /// Maximum number of open ports.
    ///
    /// This must not exceed 2^31 = 2147483648.
    /// By default this is 16384.
    pub max_ports: NonZeroU32,
    /// Default behavior when ports are exhausted and a connect is requested.
    ///
    /// This can be overridden on a per-request basis.
    /// By default this is wait with a timeout of 60 seconds.
    pub ports_exhausted: PortsExhausted,
    /// Maximum buffered receive data size in bytes.
    ///
    /// By default this is 64 kB.
    pub max_data_size: NonZeroUsize,
    /// Size of a chunk of data in bytes.
    ///
    /// By default this is 16 kB.
    pub chunk_size: NonZeroU32,
    /// Size of receive buffer of each port in bytes.
    ///
    /// By default this is 64 kB.
    pub port_receive_buffer: NonZeroU32,
    /// Length of global send queue.
    /// Each element holds a chunk.
    ///
    /// This limit the number of chunks sendable by using [Sender::try_send].
    /// By default this is 32.
    pub shared_send_queue: NonZeroU16,
    /// Length of connection request queue.
    ///
    /// By default this is 128,
    pub connect_queue: NonZeroU16,
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            trace_id: None,
            connection_timeout: Some(Duration::from_secs(60)),
            max_ports: NonZeroU32::new(16384).unwrap(),
            ports_exhausted: PortsExhausted::Wait(Some(Duration::from_secs(60))),
            max_data_size: NonZeroUsize::new(134_217_728).unwrap(),
            chunk_size: NonZeroU32::new(16384).unwrap(),
            port_receive_buffer: NonZeroU32::new(65536).unwrap(),
            shared_send_queue: NonZeroU16::new(32).unwrap(),
            connect_queue: NonZeroU16::new(128).unwrap(),
        }
    }
}
