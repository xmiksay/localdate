//! Email fixtures: read links out of the in-memory outbox, log in / sign up by email.

use axum::http::StatusCode;
use localdate_api::mail::Email;
use serde_json::json;

use super::{TestApp, Tokens, tokens_from};

pub const BASE_URL: &str = "https://app.test";

impl TestApp {
    /// Like `new`, with email enabled and sent into `outbox`.
    pub async fn with_email() -> Self {
        Self::try_new(localdate_api::app, |_| {}, true)
            .await
            .expect("test app setup")
    }

    /// Emails sent to `to`, oldest first, once every queued send has finished.
    pub async fn mails_to(&self, to: &str) -> Vec<Email> {
        self.settle_messages().await;
        self.outbox
            .sent()
            .into_iter()
            .filter(|m| m.to.to_string() == to)
            .collect()
    }

    /// Waits until detached work (reset lookups, link notices) and every mail / bot send finished.
    pub async fn settle_messages(&self) {
        let idle = || {
            self.state.detached.idle()
                && self.state.email.as_ref().is_none_or(|s| s.idle())
                && self.state.telegram.as_ref().is_none_or(|s| s.idle())
        };
        for _ in 0..400 {
            if idle() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(idle(), "message sends did not finish");
    }

    /// The token from the newest email to `to` and the link path it was sent for.
    pub async fn last_link(&self, to: &str) -> (String, String) {
        let mail = self.mails_to(to).await.pop().expect("an email was sent");
        let url = mail
            .text
            .lines()
            .find(|l| l.starts_with(BASE_URL))
            .expect("the email has a link");
        let (path, token) = url[BASE_URL.len()..]
            .split_once("#token=")
            .expect("link carries the token in the fragment");
        (path.to_owned(), token.to_owned())
    }

    pub async fn email_start(&self, email: &str) -> StatusCode {
        self.post("/api/auth/email/start", json!({ "email": email }))
            .await
            .0
    }

    /// Full sign-up through the mailed link; returns the new account's tokens.
    pub async fn email_signup(&self, email: &str, username: &str) -> Tokens {
        assert_eq!(self.email_start(email).await, StatusCode::ACCEPTED);
        let (_, token) = self.last_link(email).await;
        let (status, body) = self
            .post(
                "/api/auth/email/signup",
                json!({ "token": token, "username": username }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "signup failed: {body}");
        tokens_from(&body)
    }

    /// A password account with `email` linked through the real link flow.
    pub async fn linked(&self, username: &str, email: &str) -> Tokens {
        let t = self.register(username).await;
        let (status, _) = self
            .post_as(
                "/api/me/identities/email",
                &t.access_token,
                json!({ "email": email }),
            )
            .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let (_, token) = self.last_link(email).await;
        let (status, body) = self
            .post_as(
                "/api/me/identities/email/confirm",
                &t.access_token,
                json!({ "token": token }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        t
    }
}
