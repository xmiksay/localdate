//! Delivery to a push service: the [`PushSender`] seam and the real Web Push (RFC 8030/8291/8292) one.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, ensure};
use axum::http::{HeaderValue, Uri, header};
use web_push_native::vapid::VapidSignature;
use web_push_native::{Auth, WebPushBuilder, p256};

use super::message::Push;
use super::vapid::{Vapid, decode_b64url};

/// Push services reject VAPID tokens valid for more than 24 h; stay well inside that.
const VAPID_VALIDITY: Duration = Duration::from_secs(12 * 60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// One browser subscription, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    /// 404/410: the subscription no longer exists and must be forgotten.
    Gone,
    /// Any other 4xx but 429, or a subscription that cannot even be encrypted for: retrying the
    /// same request will not help, so a few in a row drop the subscription.
    Rejected(String),
    /// 429, 5xx, network: worth trying again on the next event.
    Failed(String),
}

pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;

/// Hands one push to the push service; tests swap in a recording fake.
pub trait PushSender: Send + Sync {
    fn send<'a>(&'a self, target: &'a Target, push: &'a Push) -> SendFuture<'a>;
}

pub struct WebPushSender {
    client: reqwest::Client,
    vapid: Vapid,
}

impl WebPushSender {
    pub fn new(vapid: Vapid) -> Result<Self> {
        let client = reqwest::Client::builder()
            // A redirect could lead off the allowlisted push-service hosts.
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("building push HTTP client")?;
        Ok(Self { client, vapid })
    }

    async fn deliver(&self, target: &Target, push: &Push) -> Outcome {
        let request = match build_request(&self.vapid, target, push) {
            Ok(request) => request,
            Err(e) => return Outcome::Rejected(format!("{e:#}")),
        };
        match self.client.execute(request).await {
            Ok(resp) if resp.status().is_success() => Outcome::Delivered,
            Ok(resp) if matches!(resp.status().as_u16(), 404 | 410) => Outcome::Gone,
            Ok(resp) if resp.status().is_client_error() && resp.status().as_u16() != 429 => {
                Outcome::Rejected(format!("push service answered {}", resp.status()))
            }
            Ok(resp) => Outcome::Failed(format!("push service answered {}", resp.status())),
            Err(e) => Outcome::Failed(format!("push request failed: {e}")),
        }
    }
}

impl PushSender for WebPushSender {
    fn send<'a>(&'a self, target: &'a Target, push: &'a Push) -> SendFuture<'a> {
        Box::pin(self.deliver(target, push))
    }
}

/// The endpoint's origin in canonical form (lowercase host): the VAPID `aud` claim, which push
/// services compare literally. The endpoint itself is kept as the browser sent it.
fn audience(endpoint: &str) -> Result<Uri> {
    let url = url::Url::parse(endpoint).context("endpoint is not a URL")?;
    let host = url.host_str().context("endpoint has no host")?;
    format!("{}://{host}/", url.scheme())
        .parse()
        .context("endpoint origin is not a URI")
}

/// The encrypted (aes128gcm) request with VAPID, TTL, Topic and Urgency headers.
pub fn build_request(vapid: &Vapid, target: &Target, push: &Push) -> Result<reqwest::Request> {
    let uri: Uri = target.endpoint.parse().context("endpoint is not a URI")?;
    let p256dh = decode_b64url(&target.p256dh).context("p256dh is not base64url")?;
    let ua_public =
        p256::PublicKey::from_sec1_bytes(&p256dh).map_err(|e| anyhow!("bad p256dh: {e}"))?;
    let auth = decode_b64url(&target.auth).context("auth is not base64url")?;
    ensure!(auth.len() == 16, "auth must be 16 bytes");
    // Signed separately: `with_vapid` would tie the token's expiry to the TTL (24 h for messages).
    let mut request = WebPushBuilder::new(uri.clone(), ua_public, Auth::clone_from_slice(&auth))
        .with_valid_duration(push.ttl)
        .build(push.payload.clone())
        .map_err(|e| anyhow!("encrypting push: {e}"))?;
    let signature = VapidSignature::sign(
        &audience(&target.endpoint)?,
        VAPID_VALIDITY,
        &vapid.subject,
        &vapid.key_pair,
    )
    .map_err(|e| anyhow!("signing VAPID token: {e}"))?;
    let headers = request.headers_mut();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::try_from(signature.to_string()).context("VAPID header")?,
    );
    headers.insert("Urgency", HeaderValue::from_static(push.urgency.as_str()));
    if let Some(topic) = &push.topic {
        headers.insert(
            "Topic",
            HeaderValue::try_from(topic.as_str()).context("Topic header")?,
        );
    }
    reqwest::Request::try_from(request).context("converting push request")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VapidConfig;
    use crate::push::message::{Kind, Lang, TopicKey};
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use uuid::Uuid;

    #[test]
    fn request_is_encrypted_for_the_subscription_and_signed() {
        let (public_key, private_key) = super::super::vapid::generate();
        let vapid = Vapid::from_config(&VapidConfig {
            public_key: public_key.clone(),
            private_key,
            subject: "mailto:ops@example.com".into(),
        })
        .expect("vapid");
        // The browser side of the subscription.
        let ua_secret = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
        let ua_public = ua_secret.public_key();
        let auth = [7u8; 16];
        let target = Target {
            // Uppercase host as a browser could hand it out: stored and requested as sent.
            endpoint: "https://FCM.GoogleAPIs.com/fcm/send/abc".into(),
            p256dh: URL_SAFE_NO_PAD.encode(
                p256::elliptic_curve::sec1::ToEncodedPoint::to_encoded_point(&ua_public, false)
                    .as_bytes(),
            ),
            auth: URL_SAFE_NO_PAD.encode(auth),
        };
        let push = Kind::Message(Uuid::new_v4()).build(Lang::Cs, &TopicKey::random());

        let request = build_request(&vapid, &target, &push).expect("request");
        let headers = request.headers();
        assert_eq!(headers["TTL"], "86400");
        assert_eq!(headers["Content-Encoding"], "aes128gcm");
        assert_eq!(headers["Urgency"], "high");
        assert_eq!(
            headers["Topic"].to_str().ok(),
            push.topic.as_deref(),
            "topic"
        );
        let authz = headers["Authorization"].to_str().expect("ascii");
        assert!(authz.starts_with("vapid t="), "{authz}");
        assert!(authz.ends_with(&format!(", k={public_key}")), "{authz}");
        let token = authz
            .strip_prefix("vapid t=")
            .and_then(|rest| rest.split(',').next())
            .expect("token");
        let claims = token.split('.').nth(1).expect("claims part");
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).expect("base64url"))
                .expect("claims json");
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["sub"], "mailto:ops@example.com");
        assert_eq!(request.url().host_str(), Some("fcm.googleapis.com"));

        let body = request.body().and_then(|b| b.as_bytes()).expect("body");
        let plain =
            web_push_native::decrypt(body.to_vec(), &ua_secret, &Auth::clone_from_slice(&auth))
                .expect("browser can decrypt");
        assert_eq!(plain, push.payload);
    }

    #[test]
    fn malformed_subscription_keys_fail_cleanly() {
        let (public_key, private_key) = super::super::vapid::generate();
        let vapid = Vapid::from_config(&VapidConfig {
            public_key,
            private_key,
            subject: "mailto:ops@example.com".into(),
        })
        .expect("vapid");
        let target = Target {
            endpoint: "https://fcm.googleapis.com/fcm/send/abc".into(),
            p256dh: "AAAA".into(),
            auth: "AAAA".into(),
        };
        let push = Kind::Wave.build(Lang::Cs, &TopicKey::random());
        assert!(build_request(&vapid, &target, &push).is_err());
    }
}
