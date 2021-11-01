//! Channel multiplexer configuration.

use std::time::Duration;

use super::msg::MAX_MSG_LENGTH;

/// Behavior when ports are exhausted and a connect is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PortsExhausted {
    /// Immediately fail connect request.
    Fail,
    /// Wait for a port to become available with an optional timeout.
    Wait(Option<Duration>),
}

/// Channel multiplexer configuration.
///
/// In most cases the default configuration ([Cfg::default]) is fine and should be used.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cfg {
    /// Time after which connection is closed when no data is.
    ///
    /// Pings are send automatically when this is enabled and no data is transmitted.
    /// By default this is 60 seconds.
    pub connection_timeout: Option<Duration>,
    /// Maximum number of open ports.
    ///
    /// This must not exceed 2^31 = 2147483648.
    /// By default this is 16384.
    pub max_ports: u32,
    /// Default behavior when ports are exhausted and a connect is requested.
    ///
    /// This can be overridden on a per-request basis.
    /// By default this is wait with a timeout of 60 seconds.
    pub ports_exhausted: PortsExhausted,
    /// Maximum size of received data per message in bytes.
    ///
    /// [Receiver::recv_chunk](super::Receiver::recv_chunk) and [remote channels](crate::rch)
    /// are not affected by this limit.
    /// This can be configured on a per-receiver basis.
    ///
    /// By default this is 64 kB.
    pub max_data_size: usize,
    /// Maximum port requests received per message.
    ///
    /// [Remote channels](crate::rch)  are not affected by this limit.
    /// This can be configured on a per-receiver basis.
    ///
    /// By default this is 128.
    pub max_received_ports: usize,
    /// Size of a chunk of data in bytes.
    ///
    /// By default this is 16 kB.
    /// This must be at least 4 bytes.
    /// This must not exceed 2^32 - 16 = 4294967279.
    pub chunk_size: u32,
    /// Size of receive buffer of each port in bytes.
    /// The size of the received data can exceed this value.
    ///
    /// By default this is 64 kB.
    /// This must be at least 4 bytes.
    pub receive_buffer: u32,
    /// Length of global send queue.
    /// Each element holds a chunk.
    ///
    /// This limit the number of chunks sendable by using
    /// [Sender::try_send](super::Sender::try_send).
    /// By default this is 32.
    /// This must not be zero.
    pub shared_send_queue: usize,
    /// Maximum number of outstanding connection requests.
    ///
    /// By default this is 128,
    /// This must not be zero.
    pub connect_queue: u16,
    #[doc(hidden)]
    pub _non_exhaustive: (),
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            connection_timeout: Some(Duration::from_secs(60)),
            max_ports: 16384,
            ports_exhausted: PortsExhausted::Wait(Some(Duration::from_secs(60))),
            max_data_size: 65_536,
            max_received_ports: 128,
            chunk_size: 16384,
            receive_buffer: 65536,
            shared_send_queue: 32,
            connect_queue: 128,
            _non_exhaustive: (),
        }
    }
}

impl Cfg {
    /// Checks the configuration.
    ///
    /// # Panics
    /// Panics if the configuration is invalid.
    pub(crate) fn check(&self) {
        if self.max_ports > 2u32.pow(31) {
            panic!("maximum ports must not exceed 2^31");
        }

        if self.chunk_size < 4 {
            panic!("chunk size must be at least 4");
        }

        if self.receive_buffer < 4 {
            panic!("receive buffer must be at least 4 bytes");
        }

        if self.shared_send_queue == 0 {
            panic!("shared send queue length must not be zero");
        }

        if self.connect_queue == 0 {
            panic!("connect queue length must not be zero");
        }
    }

    /// Returns the maximum size of a frame that can be received by a
    /// channel multiplexer using this configuration.
    ///
    /// # Panics
    /// Panics if the configuration is invalid.
    pub fn max_frame_length(&self) -> u32 {
        (MAX_MSG_LENGTH as u32).checked_add(self.chunk_size).expect("maximum frame size exceeds u32::MAX")
    }
}
