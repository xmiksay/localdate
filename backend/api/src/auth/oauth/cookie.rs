//! The signed flow cookie (`ld_oauth`), PKCE values and the in-app redirect check.

use anyhow::{Context, Result};
use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::http::header::COOKIE;
use axum::http::{HeaderMap, HeaderValue};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::Provider;

pub const COOKIE_NAME: &str = "ld_oauth";
/// Covers start/link (setting), callback and exchange (reading); nothing else ever sees it.
pub const COOKIE_PATH: &str = "/api/auth/oauth";
pub const COOKIE_TTL_SECS: i64 = 10 * 60;
const MAX_REDIRECT_LEN: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Login,
    Link,
}

/// Everything the callback needs to finish the flow the cookie's browser started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    pub provider: Provider,
    pub mode: Mode,
    pub state: String,
    pub verifier: String,
    pub nonce: String,
    pub redirect: Option<String>,
    /// Link mode: the account that asked; signed, so the browser cannot swap it.
    pub linker: Option<Linker>,
    /// Unix seconds.
    pub exp: i64,
    /// Import the provider's profile picture in the callback (only set for providers offering it).
    #[serde(default)]
    pub import_photo: bool,
}

/// Who started a link flow, and with which access token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Linker {
    pub user_id: Uuid,
    /// `iat` of the access token that started the flow: a password reset/change after it must
    /// end the flow like it ends that token, or a session meant to be cut off could still link.
    pub token_issued_at: i64,
    /// Language of the "new sign-in method" notice mail (`MailLang`, raw).
    pub lang: Option<String>,
}

impl Flow {
    pub fn new(
        provider: Provider,
        mode: Mode,
        redirect: Option<String>,
        linker: Option<Linker>,
    ) -> Self {
        Self {
            provider,
            mode,
            state: random(),
            verifier: random(),
            nonce: random(),
            redirect,
            linker,
            exp: chrono::Utc::now().timestamp() + COOKIE_TTL_SECS,
            import_photo: false,
        }
    }
}

/// 32 random bytes, base64url (43 chars — also a valid RFC 7636 code verifier).
pub fn random() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// RFC 7636 S256 code challenge.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Constant-time equality (the length is not secret here: all compared values are fixed-size).
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// `raw` if it is an in-app path (single leading `/`, printable ASCII, no `\`, bounded), else `None`.
/// `//host` and `/\host` would be read as another origin by browsers.
pub fn safe_redirect(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let ok = raw.len() <= MAX_REDIRECT_LEN
        && raw.starts_with('/')
        && !raw.starts_with("//")
        && raw.bytes().all(|b| (0x21..0x7f).contains(&b) && b != b'\\');
    ok.then(|| raw.to_owned())
}

/// HMAC-SHA256 signer for the cookie; its key is derived from `JWT_SECRET` so no new secret is needed,
/// and the derivation label keeps it distinct from the JWT key.
#[derive(Clone)]
pub struct Signer {
    key: [u8; 32],
}

type HmacSha256 = Hmac<Sha256>;

impl Signer {
    pub fn new(jwt_secret: &str) -> Result<Self> {
        let mut mac =
            HmacSha256::new_from_slice(jwt_secret.as_bytes()).context("cookie key derivation")?;
        mac.update(b"localdate oauth flow cookie v1");
        Ok(Self {
            key: mac.finalize().into_bytes().into(),
        })
    }

    fn mac(&self) -> Result<HmacSha256> {
        HmacSha256::new_from_slice(&self.key).context("cookie mac")
    }

    pub fn sign(&self, flow: &Flow) -> Result<String> {
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(flow).context("cookie json")?);
        let mut mac = self.mac()?;
        mac.update(payload.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{payload}.{sig}"))
    }

    /// The flow in `value` if the signature holds and it has not expired at `now` (unix seconds).
    pub fn verify(&self, value: &str, now: i64) -> Option<Flow> {
        let (payload, sig) = value.split_once('.')?;
        let sig = URL_SAFE_NO_PAD.decode(sig).ok()?;
        let mut mac = self.mac().ok()?;
        mac.update(payload.as_bytes());
        mac.verify_slice(&sig).ok()?;
        let flow: Flow = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
        (flow.exp > now).then_some(flow)
    }

    /// The verified flow from the request's `Cookie` header(s).
    pub fn read(&self, headers: &HeaderMap) -> Option<Flow> {
        let now = chrono::Utc::now().timestamp();
        headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|pair| pair.trim().strip_prefix(COOKIE_NAME)?.strip_prefix('='))
            .find_map(|value| self.verify(value, now))
    }
}

/// `Set-Cookie` for `value`; `None` clears it.
pub fn set_cookie(value: Option<&str>, secure: bool) -> Result<HeaderValue> {
    let (value, max_age) = match value {
        Some(v) => (v, COOKIE_TTL_SECS),
        None => ("", 0),
    };
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={value}; Path={COOKIE_PATH}; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}"
    ))
    .context("cookie header")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn flow() -> Flow {
        Flow::new(
            Provider::Google,
            Mode::Link,
            Some("/settings".into()),
            Some(Linker {
                user_id: Uuid::new_v4(),
                token_issued_at: 1_700_000_000,
                lang: Some("en".into()),
            }),
        )
    }

    #[test]
    fn pkce_matches_rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = random();
        assert_eq!(v.len(), 43);
        assert_ne!(v, random());
    }

    #[test]
    fn signed_cookie_roundtrips_and_rejects_tampering() {
        let signer = Signer::new(SECRET).expect("signer");
        let f = flow();
        let value = signer.sign(&f).expect("sign");
        let now = chrono::Utc::now().timestamp();
        assert_eq!(signer.verify(&value, now), Some(f.clone()));

        let (payload, sig) = value.split_once('.').expect("two parts");
        let mut other = f.clone();
        other.linker = other.linker.map(|l| Linker {
            user_id: Uuid::new_v4(),
            ..l
        });
        let forged_payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&other).expect("json"));
        assert_eq!(signer.verify(&format!("{forged_payload}.{sig}"), now), None);
        assert_eq!(signer.verify(payload, now), None);
        assert_eq!(signer.verify(&format!("{payload}.AAAA"), now), None);
        let foreign = Signer::new("another-secret-another-secret-xx").expect("signer");
        assert_eq!(foreign.verify(&value, now), None);
        assert_eq!(signer.verify(&value, f.exp), None, "expired");
    }

    #[test]
    fn cookie_is_read_from_the_header_among_others() {
        let signer = Signer::new(SECRET).expect("signer");
        let f = flow();
        let value = signer.sign(&f).expect("sign");
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!("a=b; {COOKIE_NAME}=junk; {COOKIE_NAME}={value}"))
                .expect("header"),
        );
        assert_eq!(signer.read(&headers), Some(f));
        assert_eq!(signer.read(&HeaderMap::new()), None);
    }

    #[test]
    fn set_cookie_attributes() {
        let set = set_cookie(Some("v"), true).expect("header");
        let set = set.to_str().expect("ascii");
        for attr in [
            "ld_oauth=v",
            "Path=/api/auth/oauth",
            "Max-Age=600",
            "HttpOnly",
            "SameSite=Lax",
            "Secure",
        ] {
            assert!(set.contains(attr), "{set} lacks {attr}");
        }
        let clear = set_cookie(None, false).expect("header");
        let clear = clear.to_str().expect("ascii");
        assert!(clear.contains("Max-Age=0") && !clear.contains("Secure"));
    }

    #[test]
    fn redirect_must_be_an_in_app_path() {
        for ok in ["/", "/settings", "/chat/abc?x=1#y", "/a%2F%2Fb"] {
            assert_eq!(safe_redirect(Some(ok)).as_deref(), Some(ok), "{ok}");
        }
        let long = format!("/{}", "a".repeat(512));
        for bad in [
            "",
            "settings",
            "//evil.com",
            "/\\evil.com",
            "https://evil.com",
            "/a b",
            "/a\nb",
            "/é",
            "\\/evil",
            &long,
        ] {
            assert_eq!(safe_redirect(Some(bad)), None, "{bad:?}");
        }
        assert_eq!(safe_redirect(None), None);
    }

    #[test]
    fn ct_eq_compares_contents() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "ab"));
    }
}
