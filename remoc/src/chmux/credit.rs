use futures::{future::BoxFuture, FutureExt};
use std::{
    mem,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::{
    mpsc::{self, error::TrySendError},
    oneshot,
};

use super::{mux::PortEvt, ChMuxError, SendError};

// ===========================================================================
// Credit accounting for sending data
// ===========================================================================

/// Assigned credits.
///
/// The credits can be used to send data.
/// Unused dropped credits are returned to the the credit providers.
#[derive(Default)]
pub(crate) struct AssignedCredits {
    port: u32,
    port_inner: Weak<Mutex<ChannelCreditsInner>>,
}

impl AssignedCredits {
    /// Create with specified number of credits.
    fn new(port: u32, port_inner: Weak<Mutex<ChannelCreditsInner>>) -> Self {
        Self { port, port_inner }
    }

    /// True if no credits are contained.
    pub fn is_empty(&self) -> bool {
        self.port == 0
    }

    /// Available credits.
    pub fn available(&self) -> u32 {
        self.port
    }

    /// Takes credits out for sending data.
    ///
    /// Panics when insufficient credits are available.
    pub fn take(&mut self, credits: u32) {
        if self.port >= credits {
            self.port -= credits;
        } else {
            panic!("insufficient AssignedCredits")
        }
    }
}

impl Drop for AssignedCredits {
    fn drop(&mut self) {
        if self.port > 0 {
            if let Some(port) = self.port_inner.upgrade() {
                let mut port = port.lock().unwrap();
                port.credits += self.port;
            }
        }
    }
}

#[derive(Debug)]
struct ChannelCreditsInner {
    credits: u32,
    closed: Option<bool>,
    notify: Vec<oneshot::Sender<()>>,
}

/// Provides credits for sending over a channel.
#[derive(Debug)]
pub(crate) struct CreditProvider(Arc<Mutex<ChannelCreditsInner>>);

impl CreditProvider {
    /// Provides the given count of channel credits for consumption.
    pub fn provide<SinkError, StreamError>(
        &self, credits: u32,
    ) -> Result<(), ChMuxError<SinkError, StreamError>> {
        let notify = {
            let mut inner = self.0.lock().unwrap();

            match inner.credits.checked_add(credits) {
                Some(new_credits) => inner.credits = new_credits,
                None => return Err(ChMuxError::Protocol("credits overflow".to_string())),
            };

            mem::take(&mut inner.notify)
        };

        for tx in notify {
            let _ = tx.send(());
        }

        Ok(())
    }

    /// Closes the channel.
    pub fn close(&self, gracefully: bool) {
        let notify = {
            let mut inner = self.0.lock().unwrap();

            inner.closed = Some(gracefully);

            mem::take(&mut inner.notify)
        };

        for tx in notify {
            let _ = tx.send(());
        }
    }
}

/// Requests and consumes credits for sending over a channel.
pub(crate) struct CreditUser {
    channel: Weak<Mutex<ChannelCreditsInner>>,
}

impl CreditUser {
    /// Requests credits for sending.
    /// Blocks until at least `min_req` credits become available.
    pub async fn request(&self, req: u32, min_req: u32) -> Result<AssignedCredits, SendError> {
        debug_assert!(req > 0);

        loop {
            let rx_channel = {
                let channel = match self.channel.upgrade() {
                    Some(channel) => channel,
                    None => return Err(SendError::ChMux),
                };
                let mut channel = channel.lock().unwrap();
                if let Some(gracefully) = channel.closed {
                    return Err(SendError::Closed { gracefully });
                }

                if channel.credits >= min_req {
                    let channel_taken = channel.credits.min(req);
                    channel.credits -= channel_taken;

                    return Ok(AssignedCredits::new(channel_taken, self.channel.clone()));
                } else {
                    let (tx_channel, rx_channel) = oneshot::channel();
                    channel.notify.push(tx_channel);
                    rx_channel
                }
            };

            let _ = rx_channel.await;
        }
    }

    /// Requests the specified number of credits for sending without blocking.
    /// Returns requested credits if fully available, otherwise None.
    pub fn try_request(&self, req: u32) -> Result<Option<AssignedCredits>, SendError> {
        debug_assert!(req > 0);

        let channel = match self.channel.upgrade() {
            Some(channel) => channel,
            None => return Err(SendError::ChMux),
        };
        let mut channel = channel.lock().unwrap();
        if let Some(gracefully) = channel.closed {
            return Err(SendError::Closed { gracefully });
        }

        if channel.credits >= req {
            channel.credits -= req;
            Ok(Some(AssignedCredits::new(req, self.channel.clone())))
        } else {
            Ok(None)
        }
    }
}

/// Creates a pair of credit provider and credit user, initially filled
/// with the specified number of credits.
pub(crate) fn credit_send_pair(initial_credits: u32) -> (CreditProvider, CreditUser) {
    let inner =
        Arc::new(Mutex::new(ChannelCreditsInner { credits: initial_credits, closed: None, notify: Vec::new() }));

    let user = CreditUser { channel: Arc::downgrade(&inner) };
    let provider = CreditProvider(inner);
    (provider, user)
}

// ===========================================================================
// Credit accounting and return scheduling for received data
// ===========================================================================

/// Represents monitored used credits.
pub(crate) struct UsedCredit(u32);

#[derive(Debug)]
struct ChannelCreditMonitorInner {
    used: u32,
    limit: u32,
}

/// Monitors channel-specific credits.
#[derive(Debug)]
pub(crate) struct ChannelCreditMonitor(Arc<Mutex<ChannelCreditMonitorInner>>);

impl ChannelCreditMonitor {
    /// Use channel-specific credits.
    pub fn use_credits<SinkError, StreamError>(
        &self, credits: u32,
    ) -> Result<UsedCredit, ChMuxError<SinkError, StreamError>> {
        let mut inner = self.0.lock().unwrap();
        match inner.used.checked_add(credits) {
            Some(new_used) if new_used <= inner.limit => {
                inner.used = new_used;
                Ok(UsedCredit(credits))
            }
            _ => Err(ChMuxError::Protocol("remote endpoint used too many channel flow credits".to_string())),
        }
    }
}

/// Queues channel credits for return to the sending side.
pub(crate) struct ChannelCreditReturner {
    monitor: Weak<Mutex<ChannelCreditMonitorInner>>,
    to_return: u32,
    return_fut: Option<BoxFuture<'static, ()>>,
}

impl ChannelCreditReturner {
    /// Starts returning channel-specific credit.
    ///
    /// poll_return_flush must have completed (Poll::Ready) before this function is called.
    pub fn start_return(&mut self, credit: UsedCredit, remote_port: u32, tx: &mpsc::Sender<PortEvt>) {
        assert!(self.return_fut.is_none(), "start_return_one called without poll_return_flush");

        if let Some(monitor) = self.monitor.upgrade() {
            let mut monitor = monitor.lock().unwrap();

            monitor.used -= credit.0;
            self.to_return += credit.0;

            // Make sure remote endpoint has at least 4 credits (size of u32),
            // to be able to send a port data message with one port chunk.
            let threshold = if monitor.limit >= 8 { monitor.limit / 2 } else { 1 };

            if self.to_return >= threshold {
                let msg = PortEvt::ReturnCredits { remote_port, credits: self.to_return };
                self.to_return = 0;

                if let Err(TrySendError::Full(msg)) = tx.try_send(msg) {
                    let tx = tx.clone();
                    self.return_fut = Some(
                        async move {
                            let _ = tx.send(msg).await;
                        }
                        .boxed(),
                    );
                }
            }
        }
    }

    /// Completes returning of credits.
    pub async fn return_flush(&mut self) {
        if let Some(return_fut) = &mut self.return_fut {
            return_fut.await;
            self.return_fut = None;
        }
    }
}

/// A pair of ChannelCreditMonitor and ChannelCreditReturner.
pub(crate) fn credit_monitor_pair(limit: u32) -> (ChannelCreditMonitor, ChannelCreditReturner) {
    let monitor = ChannelCreditMonitor(Arc::new(Mutex::new(ChannelCreditMonitorInner { used: 0, limit })));
    let returner = ChannelCreditReturner { monitor: Arc::downgrade(&monitor.0), to_return: 0, return_fut: None };
    (monitor, returner)
}
