//! Web Push test double: a [`PushSender`] that records instead of talking to a push service.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use localdate_api::push::{Notifier, Outcome, Push, PushSender, SendFuture, Target};
use serde_json::{Value, json};

use super::{TestApp, Tokens};

/// A real browser key pair's public half and auth secret (only their shape is checked).
pub const P256DH: &str =
    "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
pub const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";

pub fn endpoint(name: &str) -> String {
    format!("https://fcm.googleapis.com/fcm/send/{name}")
}

#[derive(Default)]
pub struct RecordingSender {
    sent: Mutex<Vec<(String, Value)>>,
    gone: Mutex<HashSet<String>>,
    rejecting: Mutex<HashSet<String>>,
}

impl RecordingSender {
    /// `(endpoint, payload)` of every push so far, in send order.
    pub fn sent(&self) -> Vec<(String, Value)> {
        self.sent.lock().expect("lock").clone()
    }

    pub fn sent_to(&self, endpoint: &str) -> Vec<Value> {
        self.sent()
            .into_iter()
            .filter(|(e, _)| e == endpoint)
            .map(|(_, p)| p)
            .collect()
    }

    /// From now on the push service answers 410 for `endpoint`.
    pub fn expire(&self, endpoint: &str) {
        self.gone.lock().expect("lock").insert(endpoint.to_owned());
    }

    /// While `on`, the push service answers 400 for `endpoint`.
    pub fn reject(&self, endpoint: &str, on: bool) {
        let mut rejecting = self.rejecting.lock().expect("lock");
        if on {
            rejecting.insert(endpoint.to_owned());
        } else {
            rejecting.remove(endpoint);
        }
    }
}

impl PushSender for RecordingSender {
    fn send<'a>(&'a self, target: &'a Target, push: &'a Push) -> SendFuture<'a> {
        let payload = serde_json::from_slice(&push.payload).expect("json payload");
        self.sent
            .lock()
            .expect("lock")
            .push((target.endpoint.clone(), payload));
        let outcome = if self.gone.lock().expect("lock").contains(&target.endpoint) {
            Outcome::Gone
        } else if self
            .rejecting
            .lock()
            .expect("lock")
            .contains(&target.endpoint)
        {
            Outcome::Rejected("400 Bad Request".into())
        } else {
            Outcome::Delivered
        };
        Box::pin(async move { outcome })
    }
}

impl TestApp {
    /// A test app with push enabled, delivering into the returned recorder.
    pub async fn with_push() -> (Self, Arc<RecordingSender>) {
        let mut app = Self::new().await;
        let fake = Arc::new(RecordingSender::default());
        let notifier = Notifier::with_sender(
            app.db.clone(),
            app.state.hub.clone(),
            "test-public-key".into(),
            fake.clone(),
        );
        app.state.notify = Arc::new(notifier);
        app.router = localdate_api::app(app.state.clone());
        (app, fake)
    }

    pub async fn subscribe_push(&self, t: &Tokens, endpoint: &str, lang: &str) -> StatusCode {
        let body = json!({ "endpoint": endpoint, "keys": { "p256dh": P256DH, "auth": AUTH }, "lang": lang });
        self.post_as("/api/me/push/subscriptions", &t.access_token, body)
            .await
            .0
    }

    /// Waits until every push started so far has been handled.
    pub async fn pushes_settled(&self) {
        self.state.notify.settled().await;
    }
}
