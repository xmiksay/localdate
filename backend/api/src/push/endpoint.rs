//! Validation of a browser's push subscription: where the server will POST, and the keys.

use url::{Host, Url};
use web_push_native::p256;

use super::vapid::decode_b64url;
use crate::error::AppError;

pub const MAX_ENDPOINT_LEN: usize = 1024;

/// Push services browsers actually use. An allowlist rather than "public IPs only": the server
/// POSTs to whatever a client registers, and a fixed set of hosts rules out SSRF without DNS checks.
const EXACT_HOSTS: [&str; 3] = [
    "fcm.googleapis.com",                // Chrome, Edge (Chromium), Samsung, Opera
    "updates.push.services.mozilla.com", // Firefox
    "web.push.apple.com",                // Safari / iOS home-screen apps
];
const HOST_SUFFIXES: [&str; 2] = [".push.apple.com", ".notify.windows.com"];

fn allowed_host(host: &str) -> bool {
    EXACT_HOSTS.contains(&host)
        || HOST_SUFFIXES
            .iter()
            .any(|s| host.len() > s.len() && host.ends_with(s))
}

/// Accepts an https URL on an allowed push-service host with the default port. The endpoint is
/// stored exactly as sent (not normalised), because the browser identifies it by that string when
/// unsubscribing; so it must also be what the sender will parse.
pub fn validate_endpoint(raw: &str) -> Result<(), AppError> {
    let invalid = || AppError::validation("endpoint must be a push service https URL");
    if raw.len() > MAX_ENDPOINT_LEN {
        return Err(AppError::validation(format!(
            "endpoint must be at most {MAX_ENDPOINT_LEN} characters"
        )));
    }
    if raw.trim() != raw || raw.parse::<axum::http::Uri>().is_err() {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    let host_ok = matches!(url.host(), Some(Host::Domain(host)) if allowed_host(host));
    let plain = url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    if host_ok && plain {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// `p256dh` must be an uncompressed P-256 point and `auth` 16 bytes, both base64url.
pub fn validate_keys(p256dh: &str, auth: &str) -> Result<(), AppError> {
    let point = decode_b64url(p256dh)
        .ok()
        .filter(|b| b.len() == 65 && p256::PublicKey::from_sec1_bytes(b).is_ok());
    if point.is_none() {
        return Err(AppError::validation(
            "keys.p256dh must be a P-256 public key",
        ));
    }
    if decode_b64url(auth).map(|b| b.len()) != Ok(16) {
        return Err(AppError::validation("keys.auth must be 16 bytes"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_known_push_services() {
        for ok in [
            "https://fcm.googleapis.com/fcm/send/abc:APA91b",
            "https://updates.push.services.mozilla.com/wpush/v2/gAAAA",
            "https://web.push.apple.com/QGx1",
            "https://api.push.apple.com/3/device/x",
            "https://wns2-db5p.notify.windows.com/w/?token=AwYAAA",
            "https://FCM.googleapis.com/fcm/send/x",
        ] {
            assert!(validate_endpoint(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn refuses_anything_that_could_reach_elsewhere() {
        for bad in [
            "http://fcm.googleapis.com/fcm/send/x",
            "https://fcm.googleapis.com:8443/fcm/send/x",
            "https://user:pw@fcm.googleapis.com/x",
            "https://fcm.googleapis.com.evil.example/x",
            "https://evilfcm.googleapis.com/x",
            "https://push.apple.com/x",
            "https://notify.windows.com/x",
            "https://127.0.0.1/x",
            "https://[::1]/x",
            "https://169.254.169.254/latest/meta-data",
            "https://localhost/x",
            "https://localdate-db:5432/x",
            "file:///etc/passwd",
            "fcm.googleapis.com/x",
            " https://fcm.googleapis.com/fcm/send/x",
            "https://fcm.googleapis.com/fcm/send/x\n",
            "",
        ] {
            assert!(validate_endpoint(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn endpoint_length_is_limited() {
        let base = "https://fcm.googleapis.com/fcm/send/";
        let fits = format!("{base}{}", "a".repeat(MAX_ENDPOINT_LEN - base.len()));
        assert!(validate_endpoint(&fits).is_ok());
        assert!(validate_endpoint(&format!("{fits}a")).is_err());
    }

    #[test]
    fn keys_must_decode_to_the_right_shapes() {
        let p256dh = "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
        let auth = "_ordMnz7uTCmrpBTeUV4Bw";
        assert!(validate_keys(p256dh, auth).is_ok());
        assert!(validate_keys(&format!("{p256dh}="), &format!("{auth}==")).is_ok());
        assert!(validate_keys(p256dh, "c2hvcnQ").is_err());
        assert!(validate_keys("not base64!", auth).is_err());
        // Right length, but not a point on the curve.
        let off_curve = format!("BA{}", "A".repeat(85));
        assert!(validate_keys(&off_curve, auth).is_err());
    }
}
