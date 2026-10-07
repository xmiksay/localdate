use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use super::event::ServerEvent;

type Sockets = HashMap<Uuid, Vec<(u64, UnboundedSender<ServerEvent>)>>;

/// In-process fan-out keyed by user id; one entry per open socket so several tabs work.
/// Single API replica only (docs/architecture.md "Realtime").
#[derive(Default)]
pub struct Hub {
    sockets: Mutex<Sockets>,
    next_id: AtomicU64,
}

impl Hub {
    fn lock(&self) -> std::sync::MutexGuard<'_, Sockets> {
        // The map stays consistent even if a holder panicked, so poisoning is ignored.
        self.sockets.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns a handle for [`Hub::unsubscribe`] and the event stream for this socket.
    pub fn subscribe(&self, user: Uuid) -> (u64, UnboundedReceiver<ServerEvent>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = unbounded_channel();
        self.lock().entry(user).or_default().push((id, tx));
        (id, rx)
    }

    pub fn unsubscribe(&self, user: Uuid, id: u64) {
        let mut map = self.lock();
        if let Some(list) = map.get_mut(&user) {
            list.retain(|(i, _)| *i != id);
            if list.is_empty() {
                map.remove(&user);
            }
        }
    }

    /// Delivers to every open socket of `user`; sockets that went away are dropped.
    pub fn send(&self, user: Uuid, event: &ServerEvent) {
        let mut map = self.lock();
        if let Some(list) = map.get_mut(&user) {
            list.retain(|(_, tx)| tx.send(event.clone()).is_ok());
            if list.is_empty() {
                map.remove(&user);
            }
        }
    }

    #[cfg(test)]
    fn sockets_of(&self, user: Uuid) -> usize {
        self.lock().get(&user).map_or(0, Vec::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(from: Uuid) -> ServerEvent {
        ServerEvent::Wave { from_user_id: from }
    }

    #[tokio::test]
    async fn delivers_to_all_sockets_of_the_user_only() {
        let hub = Hub::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let (_, mut tab1) = hub.subscribe(a);
        let (_, mut tab2) = hub.subscribe(a);
        let (_, mut other) = hub.subscribe(b);

        hub.send(a, &wave(b));
        assert!(matches!(tab1.recv().await, Some(ServerEvent::Wave { .. })));
        assert!(matches!(tab2.recv().await, Some(ServerEvent::Wave { .. })));
        assert!(other.try_recv().is_err());
    }

    #[tokio::test]
    async fn unsubscribe_removes_only_that_socket() {
        let hub = Hub::default();
        let a = Uuid::new_v4();
        let (id1, _rx1) = hub.subscribe(a);
        let (_id2, mut rx2) = hub.subscribe(a);
        hub.unsubscribe(a, id1);
        assert_eq!(hub.sockets_of(a), 1);

        hub.send(a, &ServerEvent::Ready);
        assert!(matches!(rx2.recv().await, Some(ServerEvent::Ready)));
    }

    #[tokio::test]
    async fn dropped_receivers_are_pruned_and_sending_to_nobody_is_fine() {
        let hub = Hub::default();
        let a = Uuid::new_v4();
        hub.send(a, &ServerEvent::Ready);
        let (_, rx) = hub.subscribe(a);
        drop(rx);
        hub.send(a, &ServerEvent::Ready);
        assert_eq!(hub.sockets_of(a), 0);
    }
}
