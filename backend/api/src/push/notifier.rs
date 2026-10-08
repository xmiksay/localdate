//! The one place wave / match / message events leave the server: WebSocket first, then — for a
//! recipient with no open socket anywhere — Web Push, in the background.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use entity::{push_subscription, user};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

use super::message::{Kind, Lang, Push, TopicKey};
use super::sender::{Outcome, PushSender, Target, WebPushSender};
use super::vapid::Vapid;
use crate::config::VapidConfig;
use crate::sql;
use crate::ws::{Hub, ServerEvent};

/// At most one message push per (recipient, match) in this window. Per replica, in memory: with
/// several replicas a burst can push once per replica, which is acceptable.
pub const MESSAGE_COALESCE: Duration = Duration::from_secs(60);
/// Consecutive rejections (4xx other than 404/410/429) after which a subscription is dropped.
pub const MAX_REJECTIONS: i32 = 3;
/// Concurrent requests to push services across all events.
const MAX_CONCURRENT_SENDS: usize = 16;
/// Above this many coalescing entries, expired ones are swept on the next insert.
const COALESCE_SWEEP_AT: usize = 1024;

const DELIVERED: &str =
    "UPDATE push_subscription SET last_success_at = now(), failure_count = 0 WHERE id = $1";
const REJECTED: &str =
    "UPDATE push_subscription SET failure_count = failure_count + 1 WHERE id = $1";
const DROP_REJECTED: &str = "DELETE FROM push_subscription WHERE id = $1 AND failure_count >= $2";

type Recent = HashMap<(Uuid, Uuid), Instant>;

pub struct Notifier {
    db: DatabaseConnection,
    hub: Arc<Hub>,
    /// VAPID public key; `None` = push disabled.
    public_key: Option<String>,
    sender: Option<Arc<dyn PushSender>>,
    topics: TopicKey,
    recent: Mutex<Recent>,
    permits: Semaphore,
    tracker: Arc<Tracker>,
}

impl Notifier {
    /// Push is enabled iff `vapid` is set; a bad key pair is a startup error.
    pub fn new(db: DatabaseConnection, hub: Arc<Hub>, vapid: Option<&VapidConfig>) -> Result<Self> {
        let Some(config) = vapid else {
            return Ok(Self::build(db, hub, None, None, TopicKey::random()));
        };
        let vapid = Vapid::from_config(config)?;
        let public_key = vapid.public_key.clone();
        let topics = vapid.topic_key();
        let sender: Arc<dyn PushSender> = Arc::new(WebPushSender::new(vapid)?);
        Ok(Self::build(db, hub, Some(public_key), Some(sender), topics))
    }

    /// Enabled, delivering through `sender` (tests use a recording fake).
    pub fn with_sender(
        db: DatabaseConnection,
        hub: Arc<Hub>,
        public_key: String,
        sender: Arc<dyn PushSender>,
    ) -> Self {
        Self::build(db, hub, Some(public_key), Some(sender), TopicKey::random())
    }

    fn build(
        db: DatabaseConnection,
        hub: Arc<Hub>,
        public_key: Option<String>,
        sender: Option<Arc<dyn PushSender>>,
        topics: TopicKey,
    ) -> Self {
        Self {
            db,
            hub,
            public_key,
            sender,
            topics,
            recent: Mutex::default(),
            permits: Semaphore::new(MAX_CONCURRENT_SENDS),
            tracker: Arc::default(),
        }
    }

    pub fn public_key(&self) -> Option<&str> {
        self.public_key.as_deref()
    }

    /// Delivers `event` to `user`'s sockets on every replica and, unless `user` caused it
    /// (`actor`), may push it. Never waits on the push.
    pub fn send(self: &Arc<Self>, actor: Uuid, user: Uuid, event: &ServerEvent) {
        self.hub.send(user, event);
        if user == actor || self.sender.is_none() {
            return;
        }
        let Some(kind) = Kind::of(event) else { return };
        let guard = self.tracker.enter();
        let this = self.clone();
        tokio::spawn(async move {
            let _guard = guard;
            if let Err(e) = this.push(user, kind).await {
                tracing::warn!(error = format!("{e:#}"), "push notification skipped");
            }
        });
    }

    /// Resolves once no push started by [`Notifier::send`] is still running (tests).
    pub async fn settled(&self) {
        self.tracker.settled().await;
    }

    fn recent(&self) -> std::sync::MutexGuard<'_, Recent> {
        // The map stays consistent even if a holder panicked, so poisoning is ignored.
        self.recent.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Cheapest checks first: a chat burst or a user without devices costs no further queries.
    async fn push(self: &Arc<Self>, user: Uuid, kind: Kind) -> Result<()> {
        if let Kind::Message(match_id) = kind
            && is_fresh(&self.recent(), (user, match_id), Instant::now())
        {
            return Ok(());
        }
        let subs = push_subscription::Entity::find()
            .filter(push_subscription::Column::UserId.eq(user))
            .all(&self.db)
            .await
            .context("loading push subscriptions")?;
        if subs.is_empty() {
            return Ok(());
        }
        // Banned accounts get no events at all (blocks/visibility stop them first); this backs it up.
        let account = user::Entity::find_by_id(user)
            .one(&self.db)
            .await
            .context("loading push recipient")?;
        if account.is_none_or(|u| u.banned_at.is_some()) {
            return Ok(());
        }
        if !kind.allowed(&super::load_prefs(&self.db, user).await?) {
            return Ok(());
        }
        if self.hub.is_online(user).await? {
            return Ok(());
        }
        if let Kind::Message(match_id) = kind
            && !claim(&mut self.recent(), (user, match_id), Instant::now())
        {
            return Ok(());
        }
        for sub in subs {
            let guard = self.tracker.enter();
            let this = self.clone();
            tokio::spawn(async move {
                let _guard = guard;
                this.deliver(sub, kind).await;
            });
        }
        Ok(())
    }

    async fn deliver(&self, sub: push_subscription::Model, kind: Kind) {
        let Some(sender) = &self.sender else { return };
        let Ok(_permit) = self.permits.acquire().await else {
            return;
        };
        let push: Push = kind.build(Lang::from_db(&sub.lang), &self.topics);
        let target = Target {
            endpoint: sub.endpoint,
            p256dh: sub.p256dh,
            auth: sub.auth,
        };
        let result = match sender.send(&target, &push).await {
            Outcome::Delivered => sql::exec(&self.db, DELIVERED, vec![sub.id.into()]).await,
            Outcome::Gone => push_subscription::Entity::delete_by_id(sub.id)
                .exec(&self.db)
                .await
                .map(|r| r.rows_affected),
            Outcome::Rejected(error) => {
                log_failure(&target, &error);
                self.count_rejection(sub.id).await
            }
            Outcome::Failed(error) => {
                log_failure(&target, &error);
                Ok(0)
            }
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, "recording push outcome failed");
        }
    }

    async fn count_rejection(&self, id: Uuid) -> Result<u64, sea_orm::DbErr> {
        sql::exec(&self.db, REJECTED, vec![id.into()]).await?;
        let values = vec![id.into(), MAX_REJECTIONS.into()];
        sql::exec(&self.db, DROP_REJECTED, values).await
    }
}

/// The endpoint is a bearer capability; log only its host.
fn log_failure(target: &Target, error: &str) {
    let host = url::Url::parse(&target.endpoint)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned));
    tracing::warn!(?host, error, "push delivery failed");
}

fn is_fresh(recent: &Recent, key: (Uuid, Uuid), now: Instant) -> bool {
    recent
        .get(&key)
        .is_some_and(|at| now.saturating_duration_since(*at) < MESSAGE_COALESCE)
}

/// Takes the coalescing slot for `key` unless it was taken less than [`MESSAGE_COALESCE`] ago.
fn claim(recent: &mut Recent, key: (Uuid, Uuid), now: Instant) -> bool {
    if is_fresh(recent, key, now) {
        return false;
    }
    if recent.len() >= COALESCE_SWEEP_AT {
        recent.retain(|_, at| now.saturating_duration_since(*at) < MESSAGE_COALESCE);
    }
    recent.insert(key, now);
    true
}

/// Counts running push tasks so tests can wait for them.
#[derive(Default)]
struct Tracker {
    running: AtomicUsize,
    idle: Notify,
}

struct Guard(Arc<Tracker>);

impl Tracker {
    fn enter(self: &Arc<Self>) -> Guard {
        self.running.fetch_add(1, Ordering::SeqCst);
        Guard(self.clone())
    }

    async fn settled(&self) {
        loop {
            // Created before the check: `notify_waiters` reaches it even if not yet polled.
            let idle = self.idle.notified();
            if self.running.load(Ordering::SeqCst) == 0 {
                return;
            }
            idle.await;
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.running.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_coalesces_per_recipient_and_match() {
        let mut recent = Recent::new();
        let (user, m1, m2) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let t0 = Instant::now();
        assert!(!is_fresh(&recent, (user, m1), t0));
        assert!(claim(&mut recent, (user, m1), t0));
        assert!(is_fresh(&recent, (user, m1), t0 + Duration::from_secs(59)));
        assert!(!claim(
            &mut recent,
            (user, m1),
            t0 + Duration::from_secs(59)
        ));
        assert!(claim(&mut recent, (user, m2), t0 + Duration::from_secs(1)));
        assert!(claim(&mut recent, (Uuid::new_v4(), m1), t0));
        assert!(claim(&mut recent, (user, m1), t0 + MESSAGE_COALESCE));
    }

    #[test]
    fn claim_sweeps_expired_entries_when_large() {
        let mut recent = Recent::new();
        let t0 = Instant::now();
        for _ in 0..COALESCE_SWEEP_AT {
            claim(&mut recent, (Uuid::new_v4(), Uuid::new_v4()), t0);
        }
        claim(
            &mut recent,
            (Uuid::new_v4(), Uuid::new_v4()),
            t0 + MESSAGE_COALESCE,
        );
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn tracker_settles_after_the_last_guard() {
        let tracker = Arc::new(Tracker::default());
        tracker.settled().await;
        let guard = tracker.enter();
        let waiter = tokio::spawn({
            let tracker = tracker.clone();
            async move { tracker.settled().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(guard);
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("settled")
            .expect("join");
    }
}
