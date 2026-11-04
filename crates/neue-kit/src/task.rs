//! Background work that reports back with messages.
//!
//! Every message from another thread goes through one channel to the UI thread, which a
//! [`neue_platform::Waker`] wakes; the app then drains it and calls `update` for each.
//! Cancelling is checked on the UI thread, when a message is delivered, so after
//! [`Task::cancel`] returns no further message of that task reaches `update`.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

/// Wakes the UI thread.
pub(crate) type Wake = Arc<dyn Fn() + Send + Sync>;

/// A message on its way to the UI thread, with the cancel flag of the task that sent it.
pub(crate) struct Envelope<M> {
    pub(crate) cancelled: Option<Arc<AtomicBool>>,
    pub(crate) message: M,
}

impl<M> Envelope<M> {
    /// The message, unless its task was cancelled.
    pub(crate) fn open(self) -> Option<M> {
        match self.cancelled {
            Some(flag) if flag.load(Ordering::Acquire) => None,
            _ => Some(self.message),
        }
    }
}

/// Sends messages to the app from any thread. Clone it freely.
pub struct Sender<M> {
    tx: mpsc::Sender<Envelope<M>>,
    wake: Wake,
    cancelled: Option<Arc<AtomicBool>>,
}

impl<M> Clone for Sender<M> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            wake: self.wake.clone(),
            cancelled: self.cancelled.clone(),
        }
    }
}

impl<M> std::fmt::Debug for Sender<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("task", &self.cancelled.is_some())
            .finish_non_exhaustive()
    }
}

impl<M: Send> Sender<M> {
    /// Queues `message` for `update`. False once the app has ended, when nobody listens.
    pub fn send(&self, message: M) -> bool {
        let envelope = Envelope {
            cancelled: self.cancelled.clone(),
            message,
        };
        if self.tx.send(envelope).is_err() {
            return false;
        }
        (self.wake)();
        true
    }

    /// Whether the task this sender belongs to was cancelled. Long-running work should
    /// check it and stop early. Always false for [`crate::Cx::sender`]'s senders.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    }
}

/// A running background task. Dropping it doesn't stop the task.
#[derive(Clone, Debug)]
pub struct Task {
    cancelled: Arc<AtomicBool>,
}

impl Task {
    /// Drops the task's undelivered and future messages, and tells it through
    /// [`Sender::is_cancelled`]. Work that doesn't check keeps running to its end.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// The app's end of the channel.
pub(crate) struct Mailbox<M> {
    tx: mpsc::Sender<Envelope<M>>,
    rx: mpsc::Receiver<Envelope<M>>,
    wake: Wake,
}

impl<M> std::fmt::Debug for Mailbox<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mailbox").finish_non_exhaustive()
    }
}

impl<M: Send + 'static> Mailbox<M> {
    pub(crate) fn new(wake: Wake) -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, wake }
    }

    pub(crate) fn sender(&self) -> Sender<M> {
        Sender {
            tx: self.tx.clone(),
            wake: self.wake.clone(),
            cancelled: None,
        }
    }

    /// The next delivered message, skipping cancelled tasks' ones.
    pub(crate) fn try_recv(&self) -> Option<M> {
        loop {
            match self.rx.try_recv() {
                Ok(envelope) => {
                    if let Some(message) = envelope.open() {
                        return Some(message);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// Runs `work` on a new thread; its return value becomes a message.
    pub(crate) fn spawn(&self, work: impl FnOnce(&Sender<M>) -> M + Send + 'static) -> Task {
        let cancelled = Arc::new(AtomicBool::new(false));
        let sender = Sender {
            cancelled: Some(cancelled.clone()),
            ..self.sender()
        };
        let task = Task { cancelled };
        let spawned = std::thread::Builder::new()
            .name("neue-kit task".into())
            .spawn(move || {
                let message = work(&sender);
                sender.send(message);
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "can't start a background task");
            task.cancel();
        }
        task
    }

    /// Runs `future` to completion on a new thread; its output becomes a message.
    pub(crate) fn spawn_future(&self, future: impl Future<Output = M> + Send + 'static) -> Task {
        self.spawn(move |_| pollster::block_on(future))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    use super::*;

    fn mailbox() -> (Mailbox<u32>, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = wakes.clone();
        let mailbox = Mailbox::new(Arc::new(move || {
            w.fetch_add(1, Ordering::Relaxed);
        }));
        (mailbox, wakes)
    }

    /// Waits for the next message, as the UI thread would after a wake.
    fn recv(mailbox: &Mailbox<u32>) -> Option<u32> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(m) = mailbox.try_recv() {
                return Some(m);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        None
    }

    #[test]
    fn spawned_work_returns_a_message_and_wakes() {
        let (mailbox, wakes) = mailbox();
        mailbox.spawn(|_| 7);
        assert_eq!(recv(&mailbox), Some(7));
        // The worker sends, then wakes: the wake can trail the message by a moment.
        let deadline = Instant::now() + Duration::from_secs(5);
        while wakes.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(wakes.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn progress_then_result_arrive_in_order() {
        let (mailbox, _) = mailbox();
        mailbox.spawn(|tx| {
            tx.send(1);
            tx.send(2);
            3
        });
        assert_eq!(
            [recv(&mailbox), recv(&mailbox), recv(&mailbox)],
            [Some(1), Some(2), Some(3)]
        );
    }

    #[test]
    fn futures_run_to_completion() {
        let (mailbox, _) = mailbox();
        mailbox.spawn_future(async { 40 + 2 });
        assert_eq!(recv(&mailbox), Some(42));
    }

    #[test]
    fn cancelled_tasks_deliver_nothing_more() {
        let (mailbox, _) = mailbox();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let (seen_tx, seen_rx) = mpsc::channel::<bool>();
        let task = mailbox.spawn(move |tx| {
            tx.send(1);
            let _ = go_rx.recv();
            let _ = seen_tx.send(tx.is_cancelled());
            2
        });
        // The progress message is already queued when the task is cancelled: it's dropped.
        std::thread::sleep(Duration::from_millis(20));
        task.cancel();
        let _ = go_tx.send(());
        assert_eq!(seen_rx.recv().ok(), Some(true), "the task sees the cancel");
        // Something from another sender still arrives, so the mailbox wasn't just empty.
        mailbox.sender().send(9);
        assert_eq!(recv(&mailbox), Some(9));
        assert_eq!(mailbox.try_recv(), None);
    }

    #[test]
    fn send_fails_once_the_app_is_gone() {
        let (mailbox, _) = mailbox();
        let tx = mailbox.sender();
        drop(mailbox);
        assert!(!tx.send(1));
    }
}
