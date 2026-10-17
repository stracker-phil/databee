//! Requests sent elsewhere under a fresh id, each waiting for the reply that carries that id.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::oneshot;

use crate::lock::MutexExt;

#[derive(Debug, PartialEq)]
pub enum NoReply<E> {
    NotSent(E),
    Dropped,
    TimedOut,
}

pub struct PendingReplies<T> {
    waiting: Mutex<HashMap<String, oneshot::Sender<T>>>,
}

impl<T> Default for PendingReplies<T> {
    fn default() -> Self {
        Self {
            waiting: Mutex::default(),
        }
    }
}

impl<T> PendingReplies<T> {
    /// Sends the request with `send`, which gets its id, and waits at most `timeout` for the
    /// reply; a reply that comes later finds no one waiting.
    pub async fn ask<E>(
        &self,
        timeout: Duration,
        send: impl FnOnce(&str) -> Result<(), E>,
    ) -> Result<T, NoReply<E>> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.waiting.lock_unpoisoned().insert(id.clone(), tx);
        let outcome = match send(&id) {
            Err(e) => Err(NoReply::NotSent(e)),
            Ok(()) => match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(reply)) => Ok(reply),
                Ok(Err(_)) => Err(NoReply::Dropped),
                Err(_) => Err(NoReply::TimedOut),
            },
        };
        self.waiting.lock_unpoisoned().remove(&id);
        outcome
    }

    /// False when no request with `id` waits (answered, timed out or unknown).
    pub fn answer(&self, id: &str, reply: T) -> bool {
        let Some(tx) = self.waiting.lock_unpoisoned().remove(id) else {
            return false;
        };
        tx.send(reply).is_ok()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.waiting.lock_unpoisoned().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const WAIT: Duration = Duration::from_secs(1);

    #[tokio::test]
    async fn the_reply_with_the_requests_id_is_the_answer() {
        let pending = Arc::new(PendingReplies::<u32>::default());
        let answering = pending.clone();

        let reply = pending
            .ask(WAIT, |id| {
                let id = id.to_string();
                tokio::spawn(async move { answering.answer(&id, 7) });
                Ok::<_, ()>(())
            })
            .await;

        assert_eq!(reply, Ok(7));
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn a_failed_send_is_not_sent_and_leaves_nothing_waiting() {
        let pending = PendingReplies::<u32>::default();

        let reply = pending.ask(WAIT, |_| Err("closed")).await;

        assert_eq!(reply, Err(NoReply::NotSent("closed")));
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn without_a_reply_the_request_times_out_and_a_late_reply_is_refused() {
        let pending = PendingReplies::<u32>::default();
        let mut sent = String::new();

        let reply = pending
            .ask(Duration::from_millis(10), |id| {
                sent = id.to_string();
                Ok::<_, ()>(())
            })
            .await;

        assert_eq!(reply, Err(NoReply::TimedOut));
        assert!(!pending.answer(&sent, 7));
        assert!(pending.is_empty());
    }

    #[test]
    fn an_answer_to_an_unknown_id_is_refused() {
        assert!(!PendingReplies::<u32>::default().answer("unknown", 7));
    }
}
