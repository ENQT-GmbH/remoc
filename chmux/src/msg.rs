use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::{
    io::{self, ErrorKind},
    num::{NonZeroU16, NonZeroU32, NonZeroUsize},
    time::Duration,
};

use crate::MultiplexError;

macro_rules! invalid_data {
    ($msg:expr) => {
        io::Error::new(io::ErrorKind::InvalidData, format!("invalid value for {} received", $msg))
    };
}

/// Magic identifier.
pub const MAGIC: &[u8; 6] = b"CHMUX\0";

/// Message between two multiplexed endpoints.
#[derive(Debug)]
pub enum MultiplexMsg {
    /// Reset message.
    Reset,
    /// Hello message.
    Hello {
        // Magic identifier "CHMUX\0".
        /// Protocol version of side that sends the message.
        version: u8,
        /// Configuration of side that sends the message.
        cfg: Cfg,
    },
    /// Ping to keep connection alive when there is no data to send.
    Ping,
    /// Open connection on specified client port and assign a server port.
    OpenPort {
        /// Requesting client port.
        client_port: u32,
        // Flags u8.
        /// Wait for server port to become available.
        wait: bool,
    },
    /// Connection accepted and server port assigned.
    PortOpened {
        /// Requesting client port.
        client_port: u32,
        /// Assigned server port.
        server_port: u32,
    },
    /// Connection refused because server has no ports available.
    Rejected {
        /// Requesting client port.
        client_port: u32,
        // Flags u8.
        /// Rejected because no server ports was available and `wait` was not specified.
        no_ports: bool,
    },
    /// Data for specified port.
    ///
    /// This is followed by one data packet.
    Data {
        /// Port of side that receives this message.
        port: u32,
        // Flags u8.
        /// First chunk of data.
        ///
        /// If there are chunks buffered at the moment, they are from a cancelled transmission
        /// and should be dropped.
        first: bool,
        /// Last part of data.
        last: bool,
        /// Flow-control credit.
        credit: Credit,
    },
    /// Ports sent over a port.
    PortData {
        /// Port of side that receives this message.
        port: u32,
        // Flags u8.
        /// Wait for server port to become available.
        wait: bool,
        /// Flow-control credit.
        credit: Credit,
        /// Ports
        ports: Vec<u32>,
    },
    /// Give flow credits to a port.
    PortCredits {
        /// Port of side that receives this message.
        port: u32,
        /// Number of credits.
        credits: u16,
    },
    /// No more data will be sent to specifed remote port.
    SendFinish {
        /// Port of side that receives this message.
        port: u32,
    },
    /// Not interested on receiving any more data from specified remote port,
    /// but already sent message will still be processed.
    ReceiveClose {
        /// Port of side that receives this message.
        port: u32,
    },
    /// No more messages for this port will be accepted.
    ReceiveFinish {
        /// Port of side that receives this message.
        port: u32,
    },
    /// Give global flow credits.
    GlobalCredits {
        /// Number of credits.
        credits: u16,
    },
    /// All clients have been dropped, therefore no more OpenPort requests will occur.
    ClientFinish,
    /// Listener has been dropped, therefore no more OpenPort requests will be handled.
    ListenerFinish,
    /// Multiplexer is terminating because all clients are dropped,
    /// server is dropped and no ports are open.
    Goodbye,
}

pub const MSG_RESET: u8 = 1;
pub const MSG_HELLO: u8 = 2;
pub const MSG_PING: u8 = 3;
pub const MSG_OPEN_PORT: u8 = 4;
pub const MSG_PORT_OPENED: u8 = 5;
pub const MSG_REJECTED: u8 = 6;
pub const MSG_DATA: u8 = 7;
pub const MSG_PORT_DATA: u8 = 8;
pub const MSG_PORT_CREDITS: u8 = 9;
pub const MSG_SEND_FINISH: u8 = 10;
pub const MSG_RECEIVE_CLOSE: u8 = 11;
pub const MSG_RECEIVE_FINISH: u8 = 12;
pub const MSG_GLOBAL_CREDITS: u8 = 13;
pub const MSG_CLIENT_FINISH: u8 = 14;
pub const MSG_LISTENER_FINISH: u8 = 15;
pub const MSG_GOODBYE: u8 = 16;

pub const MSG_OPEN_PORT_FLAG_WAIT: u8 = 0b00000001;

pub const MSG_REJECTED_FLAG_NO_PORTS: u8 = 0b00000001;

pub const MSG_DATA_FLAG_FIRST: u8 = 0b00000001;
pub const MSG_DATA_FLAG_LAST: u8 = 0b00000010;
pub const MSG_DATA_CREDIT_GLOBAL: u8 = 0b00000100;

pub const MSG_PORT_DATA_CREDIT_GLOBAL: u8 = 0b00000010;
pub const MSG_PORT_DATA_FLAG_WAIT: u8 = 0b00000001;

impl MultiplexMsg {
    pub(crate) fn write(&self, mut writer: impl io::Write) -> Result<(), io::Error> {
        match self {
            MultiplexMsg::Reset => {
                writer.write_u8(MSG_RESET)?;
            }
            MultiplexMsg::Hello { version, cfg } => {
                writer.write_u8(MSG_HELLO)?;
                writer.write_all(MAGIC)?;
                writer.write_u8(*version)?;
                cfg.write(&mut writer)?;
            }
            MultiplexMsg::Ping => {
                writer.write_u8(MSG_PING)?;
            }
            MultiplexMsg::OpenPort { client_port, wait } => {
                writer.write_u8(MSG_OPEN_PORT)?;
                writer.write_u32::<LE>(*client_port)?;
                writer.write_u8(if *wait { MSG_OPEN_PORT_FLAG_WAIT } else { 0 })?;
            }
            MultiplexMsg::PortOpened { client_port, server_port } => {
                writer.write_u8(MSG_PORT_OPENED)?;
                writer.write_u32::<LE>(*client_port)?;
                writer.write_u32::<LE>(*server_port)?;
            }
            MultiplexMsg::Rejected { client_port, no_ports } => {
                writer.write_u8(MSG_REJECTED)?;
                writer.write_u32::<LE>(*client_port)?;
                writer.write_u8(if *no_ports { MSG_REJECTED_FLAG_NO_PORTS } else { 0 })?;
            }
            MultiplexMsg::Data { port, first, last, credit } => {
                writer.write_u8(MSG_DATA)?;
                writer.write_u32::<LE>(*port)?;
                let mut flags = 0;
                if *first {
                    flags |= MSG_DATA_FLAG_FIRST;
                }
                if *last {
                    flags |= MSG_DATA_FLAG_LAST;
                }
                if let Credit::Global = credit {
                    flags |= MSG_DATA_CREDIT_GLOBAL;
                }
                writer.write_u8(flags)?;
            }
            MultiplexMsg::PortData { port, wait, credit, ports } => {
                writer.write_u8(MSG_PORT_DATA)?;
                let mut flags = 0;
                if *wait {
                    flags |= MSG_PORT_DATA_FLAG_WAIT;
                }
                if let Credit::Global = credit {
                    flags |= MSG_PORT_DATA_CREDIT_GLOBAL;
                }
                writer.write_u8(flags)?;
                writer.write_u32::<LE>(*port)?;
                for p in ports {
                    writer.write_u32::<LE>(*p)?;
                }
            }
            MultiplexMsg::PortCredits { port, credits } => {
                writer.write_u8(MSG_PORT_CREDITS)?;
                writer.write_u32::<LE>(*port)?;
                writer.write_u16::<LE>(*credits)?;
            }
            MultiplexMsg::SendFinish { port } => {
                writer.write_u8(MSG_SEND_FINISH)?;
                writer.write_u32::<LE>(*port)?;
            }
            MultiplexMsg::ReceiveClose { port } => {
                writer.write_u8(MSG_RECEIVE_CLOSE)?;
                writer.write_u32::<LE>(*port)?;
            }
            MultiplexMsg::ReceiveFinish { port } => {
                writer.write_u8(MSG_RECEIVE_FINISH)?;
                writer.write_u32::<LE>(*port)?;
            }
            MultiplexMsg::GlobalCredits { credits } => {
                writer.write_u8(MSG_GLOBAL_CREDITS)?;
                writer.write_u16::<LE>(*credits)?;
            }
            MultiplexMsg::ClientFinish => {
                writer.write_u8(MSG_CLIENT_FINISH)?;
            }
            MultiplexMsg::ListenerFinish => {
                writer.write_u8(MSG_LISTENER_FINISH)?;
            }
            MultiplexMsg::Goodbye => {
                writer.write_u8(MSG_GOODBYE)?;
            }
        }
        Ok(())
    }

    pub(crate) fn read(mut reader: impl io::Read) -> Result<Self, io::Error> {
        let msg = match reader.read_u8()? {
            MSG_RESET => Self::Reset,
            MSG_HELLO => {
                let mut magic = vec![0; MAGIC.len()];
                reader.read_exact(&mut magic)?;
                if magic != MAGIC {
                    return Err(invalid_data!("invalid magic"));
                }
                Self::Hello { version: reader.read_u8()?, cfg: Cfg::read(&mut reader)? }
            }
            MSG_PING => Self::Ping,
            MSG_OPEN_PORT => Self::OpenPort {
                client_port: reader.read_u32::<LE>()?,
                wait: reader.read_u8()? & MSG_OPEN_PORT_FLAG_WAIT != 0,
            },
            MSG_PORT_OPENED => {
                Self::PortOpened { client_port: reader.read_u32::<LE>()?, server_port: reader.read_u32::<LE>()? }
            }
            MSG_REJECTED => Self::Rejected {
                client_port: reader.read_u32::<LE>()?,
                no_ports: reader.read_u8()? & MSG_REJECTED_FLAG_NO_PORTS != 0,
            },
            MSG_DATA => {
                let port = reader.read_u32::<LE>()?;
                let flags = reader.read_u8()?;
                Self::Data {
                    port,
                    first: flags & MSG_DATA_FLAG_FIRST != 0,
                    last: flags & MSG_DATA_FLAG_LAST != 0,
                    credit: if flags & MSG_DATA_CREDIT_GLOBAL != 0 { Credit::Global } else { Credit::Port },
                }
            }
            MSG_PORT_DATA => {
                let port = reader.read_u32::<LE>()?;
                let flags = reader.read_u8()?;
                let wait = flags & MSG_PORT_DATA_FLAG_WAIT != 0;
                let credit = if flags & MSG_PORT_DATA_CREDIT_GLOBAL != 0 { Credit::Global } else { Credit::Port };
                let mut ports = Vec::new();
                loop {
                    match reader.read_u32::<LE>() {
                        Ok(p) => ports.push(p),
                        Err(err) if err.kind() == ErrorKind::UnexpectedEof => break,
                        Err(err) => return Err(err),
                    }
                }
                Self::PortData { port, wait, credit, ports }
            }
            MSG_PORT_CREDITS => {
                Self::PortCredits { port: reader.read_u32::<LE>()?, credits: reader.read_u16::<LE>()? }
            }
            MSG_GLOBAL_CREDITS => Self::GlobalCredits { credits: reader.read_u16::<LE>()? },
            MSG_SEND_FINISH => Self::SendFinish { port: reader.read_u32::<LE>()? },
            MSG_RECEIVE_CLOSE => Self::ReceiveClose { port: reader.read_u32::<LE>()? },
            MSG_RECEIVE_FINISH => Self::ReceiveFinish { port: reader.read_u32::<LE>()? },
            MSG_CLIENT_FINISH => Self::ClientFinish,
            MSG_LISTENER_FINISH => Self::ListenerFinish,
            MSG_GOODBYE => Self::Goodbye,
            _ => return Err(invalid_data!("invalid message id")),
        };
        Ok(msg)
    }

    pub(crate) fn to_vec(&self) -> Vec<u8> {
        let mut data = Vec::new();
        self.write(&mut data).expect("message serialization failed");
        data
    }

    pub(crate) fn from_slice<SinkError, StreamError>(
        data: &[u8],
    ) -> Result<Self, MultiplexError<SinkError, StreamError>> {
        Self::read(data).map_err(|err| MultiplexError::Protocol(err.to_string()))
    }
}

/// Flow control credit.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credit {
    /// Global credit.
    Global,
    /// Channel-specific credit.
    Port,
}

/// Multiplexer configuration.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cfg {
    /// Time after which connection is closed when no data is.
    ///
    /// Pings are send automatically when this is enabled and no data is transmitted.
    pub connection_timeout: Option<Duration>,
    /// Maximum number of open port.
    ///
    /// This must not exceed 2^31 = 2147483648.
    /// By default this is 16384.
    pub max_ports: NonZeroU32,
    /// Maximum receive data size in bytes.
    ///
    /// By default this is 128 MB.
    pub max_data_size: NonZeroUsize,
    /// Size of a chunk of data in bytes.
    ///
    /// By default this is 16 kB.
    pub chunk_size: NonZeroU32,
    /// Length of receive queue of each port.
    /// Each element holds a chunk.
    ///
    /// By default this is 8.
    pub port_receive_queue: NonZeroU16,
    /// Length of receive queue shared between all ports.
    /// Each element holds a chunk.
    ///
    /// By default this is 1024.
    pub shared_receive_queue: u16,
    /// Length of global send queue.
    /// Each element holds a chunk.
    ///
    /// This limit the number of chunks sendable by using [RawSender::try_send].
    /// By default this is 32.
    pub shared_send_queue: NonZeroU16,
    /// Length of connection request queue.
    ///
    /// By default this is 128,
    pub connect_queue: NonZeroU16,
    /// Identifier for trace logging.
    // Not exchanged.
    pub trace_id: Option<String>,
}

impl Cfg {
    /// Default multiplexer configuration.
    pub const DEFAULT: Self = Self {
        connection_timeout: Some(Duration::from_secs(60)),
        max_ports: unsafe { NonZeroU32::new_unchecked(16384) },
        max_data_size: unsafe { NonZeroUsize::new_unchecked(134_217_728) },
        chunk_size: unsafe { NonZeroU32::new_unchecked(16384) },
        port_receive_queue: unsafe { NonZeroU16::new_unchecked(8) },
        shared_receive_queue: 1024,
        shared_send_queue: unsafe { NonZeroU16::new_unchecked(32) },
        connect_queue: unsafe { NonZeroU16::new_unchecked(128) },
        trace_id: None,
    };

    pub(crate) fn write(&self, mut writer: impl io::Write) -> Result<(), io::Error> {
        writer.write_u64::<LE>(
            self.connection_timeout.unwrap_or_default().as_millis().min(u64::MAX as u128) as u64
        )?;
        writer.write_u32::<LE>(self.max_ports.get())?;
        writer.write_u64::<LE>(self.max_data_size.get() as u64)?;
        writer.write_u32::<LE>(self.chunk_size.get())?;
        writer.write_u16::<LE>(self.port_receive_queue.get())?;
        writer.write_u16::<LE>(self.shared_receive_queue)?;
        writer.write_u16::<LE>(self.shared_send_queue.get())?;
        writer.write_u16::<LE>(self.connect_queue.get())?;
        Ok(())
    }

    pub(crate) fn read(mut reader: impl io::Read) -> Result<Self, io::Error> {
        let this = Self {
            connection_timeout: match reader.read_u64::<LE>()? {
                0 => None,
                millis => Some(Duration::from_millis(millis)),
            },
            max_ports: NonZeroU32::new(reader.read_u32::<LE>()?).ok_or_else(|| invalid_data!("max_ports"))?,
            max_data_size: NonZeroUsize::new(reader.read_u64::<LE>()?.min(usize::MAX as u64) as usize)
                .ok_or_else(|| invalid_data!("max_data_size"))?,
            chunk_size: NonZeroU32::new(reader.read_u32::<LE>()?).ok_or_else(|| invalid_data!("chunk_size"))?,
            port_receive_queue: NonZeroU16::new(reader.read_u16::<LE>()?)
                .ok_or_else(|| invalid_data!("port_receive_queue"))?,
            shared_receive_queue: reader.read_u16::<LE>()?,
            shared_send_queue: NonZeroU16::new(reader.read_u16::<LE>()?)
                .ok_or_else(|| invalid_data!("shared_send_queue"))?,
            connect_queue: NonZeroU16::new(reader.read_u16::<LE>()?)
                .ok_or_else(|| invalid_data!("connect_queue"))?,
            trace_id: None,
        };
        Ok(this)
    }
}

impl Default for Cfg {
    fn default() -> Self {
        Self::DEFAULT
    }
}
