use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use std::{
    io::{self, ErrorKind},
    time::Duration,
};

use super::{Cfg, ChMuxError};

fn invalid_data(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("invalid value for {msg} received"))
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
        cfg: ExchangedCfg,
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
        /// Port id
        id: Option<u32>,
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
        /// Last chunk of data.
        last: bool,
    },
    /// Ports sent over a port.
    PortData {
        /// Port of side that receives this message.
        port: u32,
        // Flags u8.
        /// First chunk of ports.
        ///
        /// If there are chunks buffered at the moment, they are from a cancelled transmission
        /// and should be dropped.
        first: bool,
        /// Last chunk of ports.
        last: bool,
        /// Wait for server port to become available.
        wait: bool,
        /// Ports
        ports: Vec<u32>,
        /// Port ids
        ids: Option<Vec<u32>>,
    },
    /// Give flow credits to a port.
    PortCredits {
        /// Port of side that receives this message.
        port: u32,
        /// Number of credits in bytes.
        credits: u32,
    },
    /// No more data will be sent to specified remote port.
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
    /// All clients have been dropped, therefore no more OpenPort requests will occur.
    ClientFinish,
    /// Listener has been dropped, therefore no more OpenPort requests will be handled.
    ListenerFinish,
    /// Terminate connection.
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
pub const MSG_CLIENT_FINISH: u8 = 13;
pub const MSG_LISTENER_FINISH: u8 = 14;
pub const MSG_GOODBYE: u8 = 15;

pub const MSG_OPEN_PORT_FLAG_WAIT: u8 = 0b0000_0001;
pub const MSG_OPEN_PORT_FLAG_ID: u8 = 0b0000_0010;

pub const MSG_REJECTED_FLAG_NO_PORTS: u8 = 0b0000_0001;

pub const MSG_DATA_FLAG_FIRST: u8 = 0b0000_0001;
pub const MSG_DATA_FLAG_LAST: u8 = 0b0000_0010;

pub const MSG_PORT_DATA_FLAG_FIRST: u8 = 0b0000_0001;
pub const MSG_PORT_DATA_FLAG_LAST: u8 = 0b0000_0010;
pub const MSG_PORT_DATA_FLAG_WAIT: u8 = 0b0000_0100;
pub const MSG_PORT_DATA_FLAG_IDS: u8 = 0b0000_1000;

/// Maximum message length.
///
/// Currently this is 16 to reserve space for further use.
/// Port data, limited by the maximum chunk size, may be append to a message.
pub const MAX_MSG_LENGTH: usize = 16;

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
            MultiplexMsg::OpenPort { client_port, wait, id } => {
                writer.write_u8(MSG_OPEN_PORT)?;
                writer.write_u32::<LE>(*client_port)?;
                let mut flags = 0;
                if *wait {
                    flags |= MSG_OPEN_PORT_FLAG_WAIT
                };
                if id.is_some() {
                    flags |= MSG_OPEN_PORT_FLAG_ID;
                }
                writer.write_u8(flags)?;
                if let Some(id) = id {
                    writer.write_u32::<LE>(*id)?;
                }
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
            MultiplexMsg::Data { port, first, last } => {
                writer.write_u8(MSG_DATA)?;
                writer.write_u32::<LE>(*port)?;
                let mut flags = 0;
                if *first {
                    flags |= MSG_DATA_FLAG_FIRST;
                }
                if *last {
                    flags |= MSG_DATA_FLAG_LAST;
                }
                writer.write_u8(flags)?;
            }
            MultiplexMsg::PortData { port, first, last, wait, ports, ids } => {
                writer.write_u8(MSG_PORT_DATA)?;
                writer.write_u32::<LE>(*port)?;
                let mut flags = 0;
                if *first {
                    flags |= MSG_PORT_DATA_FLAG_FIRST;
                }
                if *last {
                    flags |= MSG_PORT_DATA_FLAG_LAST;
                }
                if *wait {
                    flags |= MSG_PORT_DATA_FLAG_WAIT;
                }
                if ids.is_some() {
                    flags |= MSG_PORT_DATA_FLAG_IDS;
                }
                writer.write_u8(flags)?;
                match ids {
                    Some(ids) => {
                        assert_eq!(ports.len(), ids.len(), "ports and port ids must have same length");
                        for (p, id) in ports.iter().zip(ids) {
                            writer.write_u32::<LE>(*p)?;
                            writer.write_u32::<LE>(*id)?;
                        }
                    }
                    None => {
                        for p in ports {
                            writer.write_u32::<LE>(*p)?;
                        }
                    }
                }
            }
            MultiplexMsg::PortCredits { port, credits } => {
                writer.write_u8(MSG_PORT_CREDITS)?;
                writer.write_u32::<LE>(*port)?;
                writer.write_u32::<LE>(*credits)?;
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
                    return Err(invalid_data("invalid magic"));
                }
                Self::Hello { version: reader.read_u8()?, cfg: ExchangedCfg::read(&mut reader)? }
            }
            MSG_PING => Self::Ping,
            MSG_OPEN_PORT => {
                let client_port = reader.read_u32::<LE>()?;
                let flags = reader.read_u8()?;
                let wait = flags & MSG_OPEN_PORT_FLAG_WAIT != 0;
                let mut id = (flags & MSG_OPEN_PORT_FLAG_ID != 0).then_some(0);
                if let Some(id) = &mut id {
                    *id = reader.read_u32::<LE>()?;
                }
                Self::OpenPort { client_port, wait, id }
            }
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
                }
            }
            MSG_PORT_DATA => {
                let port = reader.read_u32::<LE>()?;
                let flags = reader.read_u8()?;
                let first = flags & MSG_PORT_DATA_FLAG_FIRST != 0;
                let last = flags & MSG_PORT_DATA_FLAG_LAST != 0;
                let wait = flags & MSG_PORT_DATA_FLAG_WAIT != 0;
                let mut ids = (flags & MSG_PORT_DATA_FLAG_IDS != 0).then_some(Vec::new());
                let mut ports = Vec::new();
                loop {
                    match reader.read_u32::<LE>() {
                        Ok(p) => ports.push(p),
                        Err(err) if err.kind() == ErrorKind::UnexpectedEof => break,
                        Err(err) => return Err(err),
                    }
                    if let Some(ids) = &mut ids {
                        ids.push(reader.read_u32::<LE>()?);
                    }
                }
                Self::PortData { port, first, last, wait, ports, ids }
            }
            MSG_PORT_CREDITS => {
                Self::PortCredits { port: reader.read_u32::<LE>()?, credits: reader.read_u32::<LE>()? }
            }
            MSG_SEND_FINISH => Self::SendFinish { port: reader.read_u32::<LE>()? },
            MSG_RECEIVE_CLOSE => Self::ReceiveClose { port: reader.read_u32::<LE>()? },
            MSG_RECEIVE_FINISH => Self::ReceiveFinish { port: reader.read_u32::<LE>()? },
            MSG_CLIENT_FINISH => Self::ClientFinish,
            MSG_LISTENER_FINISH => Self::ListenerFinish,
            MSG_GOODBYE => Self::Goodbye,
            _ => return Err(invalid_data("invalid message id")),
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
    ) -> Result<Self, ChMuxError<SinkError, StreamError>> {
        Self::read(data).map_err(|err| ChMuxError::Protocol(err.to_string()))
    }
}

/// Multiplexer configuration exchanged with remote endpoint.
#[derive(Clone, Debug)]
pub struct ExchangedCfg {
    /// Time after which connection is closed when no data is.
    pub connection_timeout: Option<Duration>,
    /// Size of a chunk of data in bytes.
    pub chunk_size: u32,
    /// Size of receive buffer of each port in bytes.
    pub port_receive_buffer: u32,
    /// Length of connection request queue.
    pub connect_queue: u16,
}

impl ExchangedCfg {
    pub(crate) fn write(&self, mut writer: impl io::Write) -> Result<(), io::Error> {
        writer.write_u64::<LE>(
            self.connection_timeout.unwrap_or_default().as_millis().min(u64::MAX as u128) as u64
        )?;
        writer.write_u32::<LE>(self.chunk_size)?;
        writer.write_u32::<LE>(self.port_receive_buffer)?;
        writer.write_u16::<LE>(self.connect_queue)?;
        Ok(())
    }

    pub(crate) fn read(mut reader: impl io::Read) -> Result<Self, io::Error> {
        let this = Self {
            connection_timeout: match reader.read_u64::<LE>()? {
                0 => None,
                millis => Some(Duration::from_millis(millis)),
            },
            chunk_size: match reader.read_u32::<LE>()? {
                cs if cs >= 4 => cs,
                _ => return Err(invalid_data("chunk_size")),
            },
            port_receive_buffer: match reader.read_u32::<LE>()? {
                prb if prb >= 4 => prb,
                _ => return Err(invalid_data("port_receive_buffer")),
            },
            connect_queue: reader.read_u16::<LE>()?,
        };
        Ok(this)
    }
}

impl From<&Cfg> for ExchangedCfg {
    fn from(cfg: &Cfg) -> Self {
        Self {
            connection_timeout: cfg.connection_timeout,
            chunk_size: cfg.chunk_size,
            port_receive_buffer: cfg.receive_buffer,
            connect_queue: cfg.connect_queue,
        }
    }
}
