use futures::{channel::mpsc, executor::block_on, sink::SinkExt};
use std::{
    sync::mpsc as sync_mpsc,
    thread,
    time::{Duration, Instant},
};

/// Sends timeout notifications over a channel.
pub struct Timeout {
    control_tx: Option<sync_mpsc::Sender<Option<Instant>>>,
    timeout_thread: Option<thread::JoinHandle<()>>,
}

impl Timeout {
    /// Creates a new timeout notifier.
    ///
    /// Returns the control interface and notification channel.
    pub fn new() -> (Self, mpsc::Receiver<()>) {
        let (timeout_tx, timeout_rx) = mpsc::channel(10);
        let (control_tx, control_rx) = sync_mpsc::channel();
        let timeout_thread = thread::spawn(move || {
            Self::timeout_thread_fn(control_rx, timeout_tx);
        });
        let timeout = Self { control_tx: Some(control_tx), timeout_thread: Some(timeout_thread) };
        (timeout, timeout_rx)
    }

    /// Thread function.
    fn timeout_thread_fn(rx: sync_mpsc::Receiver<Option<Instant>>, mut tx: mpsc::Sender<()>) {
        let mut wakeup_instant: Option<Instant> = None;
        loop {
            match &wakeup_instant {
                Some(instant) => {
                    let now = Instant::now();
                    let timeout = if *instant > now { *instant - now } else { Duration::new(0, 0) };
                    match rx.recv_timeout(timeout) {
                        Ok(next_instant) => {
                            wakeup_instant = next_instant;
                        }
                        Err(sync_mpsc::RecvTimeoutError::Timeout) => {
                            let ok = block_on(async { tx.send(()).await.is_ok() });
                            if !ok {
                                return;
                            }
                            wakeup_instant = None;
                        }
                        Err(sync_mpsc::RecvTimeoutError::Disconnected) => {
                            return;
                        }
                    }
                }
                None => match rx.recv() {
                    Ok(next_instant) => {
                        wakeup_instant = next_instant;
                    }
                    Err(sync_mpsc::RecvError) => {
                        return;
                    }
                },
            }
        }
    }

    /// Sets the duration from now when the timeout will send the next notification.
    pub fn set_duration(&mut self, duration: Duration) {
        let instant = Instant::now() + duration;
        self.set_instant(instant)
    }

    /// Sets the instant when the timeout will send the next notification.
    pub fn set_instant(&mut self, instant: Instant) {
        let _ = self.control_tx.as_mut().unwrap().send(Some(instant));
    }

    /// Cancels sending notifications.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        let _ = self.control_tx.as_mut().unwrap().send(None);
    }
}

impl Drop for Timeout {
    fn drop(&mut self) {
        self.control_tx = None;
        self.timeout_thread.take().unwrap().join().unwrap();
    }
}
