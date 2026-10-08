use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use super::event::ServerEvent;

/// Why the server closes a socket; each maps to a docs/api.md close code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    Unauthorized,
    Banned,
    /// The account check itself failed (DB error); the client should just retry.
    Internal,
    /// This replica may have missed cross-replica events; reconnecting makes the client refetch.
    Resync,
    /// This replica is shutting down; the client reconnects (to another replica).
    GoingAway,
}

impl CloseReason {
    pub fn code(self) -> u16 {
        match self {
            Self::Unauthorized => 4401,
            Self::Banned => 4403,
            Self::Internal => 1011,
            Self::Resync => 1012,
            Self::GoingAway => 1001,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Banned => "banned",
            Self::Internal => "internal error",
            Self::Resync => "resync",
            Self::GoingAway => "going away",
        }
    }
}

/// What a session receives from the hub.
#[derive(Debug, Clone)]
pub enum Outbound {
    Event(ServerEvent),
    Close(CloseReason),
}

type Sockets = HashMap<Uuid, Vec<(i64, UnboundedSender<Outbound>)>>;

/// In-process fan-out keyed by user id; one entry per open socket so several tabs work.
/// Reaches only this replica's sockets; [`super::Hub`] bridges replicas.
#[derive(Default)]
pub struct LocalHub {
    sockets: Mutex<Sockets>,
    next_id: AtomicI64,
}

impl LocalHub {
    fn lock(&self) -> std::sync::MutexGuard<'_, Sockets> {
        // The map stays consistent even if a holder panicked, so poisoning is ignored.
        self.sockets.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns a handle for [`LocalHub::unsubscribe`] and the event stream for this socket.
    pub fn subscribe(&self, user: Uuid) -> (i64, UnboundedReceiver<Outbound>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = unbounded_channel();
        self.lock().entry(user).or_default().push((id, tx));
        (id, rx)
    }

    pub fn unsubscribe(&self, user: Uuid, id: i64) {
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
            list.retain(|(_, tx)| tx.send(Outbound::Event(event.clone())).is_ok());
            if list.is_empty() {
                map.remove(&user);
            }
        }
    }

    /// Tells every socket of `user` to close with `reason` and forgets them.
    pub fn disconnect(&self, user: Uuid, reason: CloseReason) {
        for (_, tx) in self.lock().remove(&user).unwrap_or_default() {
            let _ = tx.send(Outbound::Close(reason));
        }
    }

    /// Closes every socket on this replica with `reason`.
    pub fn disconnect_all(&self, reason: CloseReason) {
        for (_, list) in self.lock().drain() {
            for (_, tx) in list {
                let _ = tx.send(Outbound::Close(reason));
            }
        }
    }

    pub fn is_connected(&self, user: Uuid) -> bool {
        self.lock().contains_key(&user)
    }

    /// `(socket id, user)` of every open socket, plus when the snapshot was taken. Taken under the
    /// lock, so a socket missing from it subscribed at or after that instant.
    pub fn snapshot(&self) -> (Vec<(i64, Uuid)>, DateTime<Utc>) {
        let map = self.lock();
        let sockets = map
            .iter()
            .flat_map(|(user, list)| list.iter().map(|(id, _)| (*id, *user)))
            .collect();
        (sockets, Utc::now())
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
        let hub = LocalHub::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let (_, mut tab1) = hub.subscribe(a);
        let (_, mut tab2) = hub.subscribe(a);
        let (_, mut other) = hub.subscribe(b);

        hub.send(a, &wave(b));
        for tab in [&mut tab1, &mut tab2] {
            assert!(matches!(
                tab.recv().await,
                Some(Outbound::Event(ServerEvent::Wave { .. }))
            ));
        }
        assert!(other.try_recv().is_err());
    }

    #[tokio::test]
    async fn unsubscribe_removes_only_that_socket() {
        let hub = LocalHub::default();
        let a = Uuid::new_v4();
        let (id1, _rx1) = hub.subscribe(a);
        let (_id2, mut rx2) = hub.subscribe(a);
        hub.unsubscribe(a, id1);
        assert_eq!(hub.sockets_of(a), 1);

        hub.send(a, &ServerEvent::Ready);
        assert!(matches!(
            rx2.recv().await,
            Some(Outbound::Event(ServerEvent::Ready))
        ));
    }

    #[tokio::test]
    async fn disconnect_closes_every_socket_of_the_user_only() {
        let hub = LocalHub::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let (_, mut tab1) = hub.subscribe(a);
        let (_, mut tab2) = hub.subscribe(a);
        let (_, mut other) = hub.subscribe(b);
        hub.disconnect(a, CloseReason::Banned);
        for tab in [&mut tab1, &mut tab2] {
            assert!(matches!(
                tab.recv().await,
                Some(Outbound::Close(CloseReason::Banned))
            ));
            assert!(tab.recv().await.is_none());
        }
        assert_eq!(hub.sockets_of(a), 0);
        hub.send(b, &ServerEvent::Ready);
        assert!(matches!(
            other.recv().await,
            Some(Outbound::Event(ServerEvent::Ready))
        ));
    }

    #[tokio::test]
    async fn disconnect_all_closes_everyone_and_snapshot_lists_open_sockets() {
        let hub = LocalHub::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let (id_a, mut rx_a) = hub.subscribe(a);
        let (id_b, mut rx_b) = hub.subscribe(b);
        let (mut sockets, _) = hub.snapshot();
        sockets.sort();
        let mut expected = vec![(id_a, a), (id_b, b)];
        expected.sort();
        assert_eq!(sockets, expected);
        assert!(hub.is_connected(a));

        hub.disconnect_all(CloseReason::Resync);
        for rx in [&mut rx_a, &mut rx_b] {
            assert!(matches!(
                rx.recv().await,
                Some(Outbound::Close(CloseReason::Resync))
            ));
        }
        assert!(!hub.is_connected(a));
        assert!(hub.snapshot().0.is_empty());
    }

    #[test]
    fn close_reasons_map_to_contract_codes() {
        assert_eq!(CloseReason::Unauthorized.code(), 4401);
        assert_eq!(CloseReason::Banned.code(), 4403);
        assert_eq!(CloseReason::Internal.code(), 1011);
        assert_eq!(CloseReason::Resync.code(), 1012);
        assert_eq!(CloseReason::GoingAway.code(), 1001);
    }

    #[tokio::test]
    async fn dropped_receivers_are_pruned_and_sending_to_nobody_is_fine() {
        let hub = LocalHub::default();
        let a = Uuid::new_v4();
        hub.send(a, &ServerEvent::Ready);
        let (_, rx) = hub.subscribe(a);
        drop(rx);
        hub.send(a, &ServerEvent::Ready);
        assert_eq!(hub.sockets_of(a), 0);
    }
}
