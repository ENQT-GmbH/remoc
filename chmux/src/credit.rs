use futures::{
    future::{self, BoxFuture},
    ready, FutureExt,
};
use std::{
    mem,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
};
use tokio::sync::{
    mpsc::{self, error::TrySendError},
    oneshot,
};

use crate::{msg::Credit, multiplexer::PortEvt, MultiplexError, SendError};

// ===========================================================================
// Credit accounting for sending data
// ===========================================================================

/// Assigned credits.
///
/// The credits can be used to send data.
/// Unused dropped credits are returned to the the credit providers.
#[derive(Default)]
pub(crate) struct AssignedCredits {
    port: u16,
    port_inner: Weak<Mutex<ChannelCreditsInner>>,
    global: u16,
    global_inner: Weak<Mutex<GlobalCreditsInner>>,
}

impl AssignedCredits {
    /// Create with specified number of credits.
    fn new(
        port: u16, port_inner: Weak<Mutex<ChannelCreditsInner>>, global: u16,
        global_inner: Weak<Mutex<GlobalCreditsInner>>,
    ) -> Self {
        Self { port, port_inner, global, global_inner }
    }

    /// True if no credits are contained.
    pub fn is_empty(&self) -> bool {
        self.port == 0 && self.global == 0
    }

    /// Takes one credit out for sending data.
    pub fn take_one(&mut self) -> Credit {
        if self.global > 0 {
            self.global -= 1;
            Credit::Global
        } else if self.port > 0 {
            self.port -= 1;
            Credit::Port
        } else {
            panic!("AssignedCredits is empty")
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

        if self.global > 0 {
            if let Some(global) = self.global_inner.upgrade() {
                let mut global = global.lock().unwrap();
                global.credits += self.global;
            }
        }
    }
}

/// Provides global credits for sending over a channel.
#[derive(Clone)]
pub(crate) struct GlobalCredits(Arc<Mutex<GlobalCreditsInner>>);

struct GlobalCreditsInner {
    credits: u16,
    notify: Vec<oneshot::Sender<()>>,
}

impl GlobalCredits {
    /// Creates with the specified number of initial credits.
    pub fn new(initial_credits: u16) -> Self {
        Self(Arc::new(Mutex::new(GlobalCreditsInner { credits: initial_credits, notify: Vec::new() })))
    }

    /// Provides the given count of global credits for consumption.
    pub fn provide<SinkError, StreamError>(
        &self, credits: u16,
    ) -> Result<(), MultiplexError<SinkError, StreamError>> {
        let notify = {
            let mut inner = self.0.lock().unwrap();

            match inner.credits.checked_add(credits) {
                Some(new_credits) => inner.credits = new_credits,
                None => return Err(MultiplexError::Protocol("credits overflow".to_string())),
            };

            mem::take(&mut inner.notify)
        };

        for tx in notify {
            let _ = tx.send(());
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ChannelCreditsInner {
    credits: u16,
    closed: Option<bool>,
    notify: Vec<oneshot::Sender<()>>,
}

/// Provides credits for sending over a channel.
#[derive(Debug)]
pub(crate) struct CreditProvider(Arc<Mutex<ChannelCreditsInner>>);

impl CreditProvider {
    /// Provides the given count of channel credits for consumption.
    pub fn provide<SinkError, StreamError>(
        &self, credits: u16,
    ) -> Result<(), MultiplexError<SinkError, StreamError>> {
        let notify = {
            let mut inner = self.0.lock().unwrap();

            match inner.credits.checked_add(credits) {
                Some(new_credits) => inner.credits = new_credits,
                None => return Err(MultiplexError::Protocol("credits overflow".to_string())),
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
    global: Weak<Mutex<GlobalCreditsInner>>,
}

impl CreditUser {
    /// Requests credits for sending.
    /// Blocks until at least one credit becomes available.
    pub async fn request(&self, mut req: u16) -> Result<AssignedCredits, SendError> {
        loop {
            let (rx_channel, rx_global) = {
                let channel = match self.channel.upgrade() {
                    Some(channel) => channel,
                    None => return Err(SendError::Multiplexer),
                };
                let mut channel = channel.lock().unwrap();
                if let Some(gracefully) = channel.closed {
                    return Err(SendError::Closed { gracefully });
                }

                let global = match self.global.upgrade() {
                    Some(global) => global,
                    None => return Err(SendError::Multiplexer),
                };
                let mut global = global.lock().unwrap();

                if global.credits > 0 || channel.credits > 0 {
                    let global_taken = if global.credits >= req { req } else { global.credits };
                    global.credits -= global_taken;
                    req -= global_taken;

                    let channel_taken = if channel.credits >= req { req } else { channel.credits };
                    channel.credits -= channel_taken;

                    return Ok(AssignedCredits::new(
                        channel_taken,
                        self.channel.clone(),
                        global_taken,
                        self.global.clone(),
                    ));
                } else {
                    let (tx_channel, rx_channel) = oneshot::channel();
                    channel.notify.push(tx_channel);

                    let (tx_global, rx_global) = oneshot::channel();
                    global.notify.push(tx_global);

                    (rx_channel, rx_global)
                }
            };

            future::select(rx_channel, rx_global).await;
        }
    }

    /// Requests the specified number of credits for sending without blocking.
    /// Returns true, if requested credits are available, otherwise false.
    pub fn try_request(&self, req: u16) -> Result<Option<AssignedCredits>, SendError> {
        let channel = match self.channel.upgrade() {
            Some(channel) => channel,
            None => return Err(SendError::Multiplexer),
        };
        let mut channel = channel.lock().unwrap();
        if let Some(gracefully) = channel.closed {
            return Err(SendError::Closed { gracefully });
        }

        let global = match self.global.upgrade() {
            Some(global) => global,
            None => return Err(SendError::Multiplexer),
        };
        let mut global = global.lock().unwrap();

        if global.credits + channel.credits >= req {
            let global_taken = if global.credits >= req { req } else { global.credits };
            global.credits -= global_taken;

            let channel_taken = req - global_taken;
            channel.credits -= channel_taken;

            Ok(Some(AssignedCredits::new(channel_taken, self.channel.clone(), global_taken, self.global.clone())))
        } else {
            Ok(None)
        }
    }
}

/// Creates a pair of credit provider and credit user, initially filled
/// with the specified number of credits.
pub(crate) fn credit_send_pair(initial_credits: u16, global: &GlobalCredits) -> (CreditProvider, CreditUser) {
    let inner =
        Arc::new(Mutex::new(ChannelCreditsInner { credits: initial_credits, closed: None, notify: Vec::new() }));

    let user = CreditUser { channel: Arc::downgrade(&inner), global: Arc::downgrade(&global.0) };
    let provider = CreditProvider(inner);
    (provider, user)
}

// ===========================================================================
// Credit accounting and return scheduling for received data
// ===========================================================================

/// Monitors global credits.
pub(crate) struct GlobalCreditMonitor(Arc<Mutex<GlobalCreditMonitorInner>>);

struct GlobalCreditMonitorInner {
    used: u16,
    limit: u16,
    to_return: u16,
    return_tx: mpsc::UnboundedSender<u16>,
}

impl GlobalCreditMonitor {
    /// Creates a new global credit monitor.
    ///
    /// Credits will be returned over the specified channel when they go over the high level.
    pub fn new(limit: u16, return_tx: mpsc::UnboundedSender<u16>) -> Self {
        // We can use an UnboundedSender here because the number of credits to return
        // is bounded by the number of available credits.
        Self(Arc::new(Mutex::new(GlobalCreditMonitorInner { used: 0, limit, to_return: 0, return_tx })))
    }

    /// Use one global credit.
    pub fn use_one<SinkError, StreamError>(&self) -> Result<UsedCredit, MultiplexError<SinkError, StreamError>> {
        let mut inner = self.0.lock().unwrap();
        match inner.used.checked_add(1) {
            Some(new_used) if new_used <= inner.limit => {
                inner.used = new_used;
                Ok(UsedCredit { credit: Credit::Global, monitor: Arc::downgrade(&self.0) })
            }
            _ => Err(MultiplexError::Protocol("remote endpoint used too many global flow credits".to_string())),
        }
    }
}

#[derive(Debug)]
struct ChannelCreditMonitorInner {
    used: u16,
    limit: u16,
}

/// Monitors channel-specific credits.
#[derive(Debug)]
pub(crate) struct ChannelCreditMonitor(Arc<Mutex<ChannelCreditMonitorInner>>);

impl ChannelCreditMonitor {
    /// Use one channel-specific credit.
    pub fn use_one<SinkError, StreamError>(&self) -> Result<UsedCredit, MultiplexError<SinkError, StreamError>> {
        let mut inner = self.0.lock().unwrap();
        match inner.used.checked_add(1) {
            Some(new_used) if new_used <= inner.limit => {
                inner.used = new_used;
                Ok(UsedCredit { credit: Credit::Port, monitor: Weak::new() })
            }
            _ => Err(MultiplexError::Protocol("remote endpoint used too many channel flow credits".to_string())),
        }
    }
}

/// Queues channel credits for return to the sending side.
pub(crate) struct ChannelCreditReturner {
    monitor: Weak<Mutex<ChannelCreditMonitorInner>>,
    to_return: u16,
    return_fut: Option<BoxFuture<'static, ()>>,
}

impl ChannelCreditReturner {
    /// Starts returning one global or channel-specific credit.
    /// Global credits are returned immediately.
    ///
    /// poll_return_flush must have completed (Poll::Ready) before this function is called.
    pub fn start_return_one(&mut self, credit: UsedCredit, remote_port: u32, tx: &mpsc::Sender<PortEvt>) {
        assert!(self.return_fut.is_none(), "start_return_one called without poll_return_flush");

        match credit.credit {
            Credit::Port => {
                if let Some(monitor) = self.monitor.upgrade() {
                    let mut monitor = monitor.lock().unwrap();

                    monitor.used -= 1;
                    self.to_return += 1;

                    if self.to_return >= (monitor.limit / 2).max(1) {
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
            Credit::Global => drop(credit),
        }
    }

    /// Completes returning of credits.
    pub async fn return_flush(&mut self) {
        if let Some(return_fut) = &mut self.return_fut {
            return_fut.await;
            self.return_fut = None;
        }
    }

    /// Polls to complete returning of credits.
    pub fn poll_return_flush(&mut self, cx: &mut Context) -> Poll<()> {
        if let Some(return_fut) = &mut self.return_fut {
            ready!(return_fut.poll_unpin(cx));
            self.return_fut = None;
        }

        Poll::Ready(())
    }
}

/// A pair of ChannelCreditMonitor and ChannelCreditReturner.
pub(crate) fn credit_monitor_pair(limit: u16) -> (ChannelCreditMonitor, ChannelCreditReturner) {
    let monitor = ChannelCreditMonitor(Arc::new(Mutex::new(ChannelCreditMonitorInner { used: 0, limit })));
    let returner = ChannelCreditReturner { monitor: Arc::downgrade(&monitor.0), to_return: 0, return_fut: None };
    (monitor, returner)
}

/// Holds a monitored used credit.
pub(crate) struct UsedCredit {
    credit: Credit,
    monitor: Weak<Mutex<GlobalCreditMonitorInner>>,
}

impl UsedCredit {
    /// Credit type.
    #[allow(dead_code)]
    pub fn credit(&self) -> Credit {
        self.credit
    }
}

impl Drop for UsedCredit {
    fn drop(&mut self) {
        if let Credit::Global = self.credit {
            if let Some(monitor) = self.monitor.upgrade() {
                let mut monitor = monitor.lock().unwrap();

                monitor.used -= 1;
                monitor.to_return += 1;

                if monitor.to_return >= (monitor.limit / 2).max(1) {
                    let _ = monitor.return_tx.send(monitor.to_return);
                    monitor.to_return = 0;
                }
            }
        }
    }
}
