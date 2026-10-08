//! OpenID Connect / OAuth 2.0 client for one provider: authorization URL, code exchange, and the
//! account id from a verified ID token or a userinfo-style endpoint.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use serde_json::{Map, Value};
use url::Url;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::config::{OidcConfig, SubjectSource, Userinfo};

/// An unknown `kid` refetches the set, but not more often than this (a forged `kid` must not
/// turn every callback into a JWKS download).
const JWKS_MIN_REFETCH: Duration = Duration::from_secs(60);
/// Token endpoint and JWKS answers are tiny; anything bigger is not what we asked for.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_SUBJECT_LEN: usize = 255;

struct CachedJwks {
    set: Arc<JwkSet>,
    fetched: Instant,
}

pub struct Oidc {
    config: OidcConfig,
    http: reqwest::Client,
    jwks: Mutex<Option<CachedJwks>>,
    /// Held while fetching, so a burst of callbacks after expiry downloads the set once.
    refetch: tokio::sync::Mutex<()>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    access_token: Option<String>,
}

/// The provider account behind a callback.
pub struct Verified {
    pub subject: String,
    /// Only for userinfo-style providers, which may import a picture with it; it lives as long as
    /// the callback and is never stored or logged.
    pub access_token: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    nonce: Option<String>,
    azp: Option<String>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl Oidc {
    pub fn new(config: OidcConfig, http: reqwest::Client) -> Self {
        Self {
            config,
            http,
            jwks: Mutex::new(None),
            refetch: tokio::sync::Mutex::new(()),
        }
    }

    pub fn authorize_url(&self, state: &str, nonce: &str, challenge: &str) -> Result<String> {
        let mut url = Url::parse(&self.config.auth_endpoint).context("auth endpoint URL")?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("scope", &self.config.scope)
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("prompt", "select_account");
        Ok(url.into())
    }

    pub fn config(&self) -> &OidcConfig {
        &self.config
    }

    /// No redirects, short timeout (see `OAuthService::new`).
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// The provider account id behind an authorization `code`, after the ID token passed every
    /// check, or as the userinfo endpoint reports it for the access token.
    pub async fn subject(&self, code: &str, verifier: &str, nonce: &str) -> Result<Verified> {
        let token = self.exchange(code, verifier).await?;
        match &self.config.subject {
            SubjectSource::IdTokenClaim(name) => {
                let id_token = token.id_token.context("token response without id_token")?;
                let claims = self.verify(&id_token, nonce).await?;
                Ok(Verified {
                    subject: claim_subject(&claims, name)?,
                    access_token: None,
                })
            }
            SubjectSource::Userinfo(info) => {
                let access_token = token
                    .access_token
                    .context("token response without access_token")?;
                let fields = self
                    .api_get(
                        &info.endpoint,
                        info,
                        &access_token,
                        &[("fields", &info.field)],
                    )
                    .await?;
                Ok(Verified {
                    subject: claim_subject(&fields, &info.field)?,
                    access_token: Some(access_token),
                })
            }
        }
    }

    /// `GET endpoint` as the account behind `access_token` (bearer, plus `appsecret_proof` when the
    /// provider wants it); the JSON object it answers.
    pub async fn api_get(
        &self,
        endpoint: &str,
        info: &Userinfo,
        access_token: &str,
        params: &[(&str, &str)],
    ) -> Result<Map<String, Value>> {
        let mut url = Url::parse(endpoint).context("userinfo endpoint URL")?;
        url.query_pairs_mut().extend_pairs(params);
        if info.appsecret_proof {
            let proof = appsecret_proof(&self.config.client_secret, access_token)?;
            url.query_pairs_mut().append_pair("appsecret_proof", &proof);
        }
        let resp = self
            .http
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            // The URL carries the appsecret_proof; keep it out of the logged error.
            .map_err(reqwest::Error::without_url)
            .context("userinfo request")?;
        let status = resp.status();
        let body = read_limited(resp).await?;
        if !status.is_success() {
            bail!(
                "userinfo endpoint answered {status}: {}",
                String::from_utf8_lossy(&body)
            );
        }
        serde_json::from_slice(&body).context("userinfo response")
    }

    async fn exchange(&self, code: &str, verifier: &str) -> Result<TokenResponse> {
        let resp = self
            .http
            .post(&self.config.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &self.config.redirect_uri),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .context("token endpoint request")?;
        let status = resp.status();
        let body = read_limited(resp).await?;
        if !status.is_success() {
            // The error body is the provider's (e.g. `invalid_grant`), no secret of ours.
            bail!(
                "token endpoint answered {status}: {}",
                String::from_utf8_lossy(&body)
            );
        }
        serde_json::from_slice(&body).context("token endpoint response")
    }

    async fn verify(&self, id_token: &str, nonce: &str) -> Result<Map<String, Value>> {
        let header = decode_header(id_token).context("ID token header")?;
        if header.alg != Algorithm::RS256 {
            bail!("ID token alg {:?} is not RS256", header.alg);
        }
        let kid = header.kid.context("ID token without kid")?;
        let key = self.key(&kid).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.config.client_id]);
        validation.set_issuer(&self.config.issuers);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let claims = decode::<Claims>(id_token, &key, &validation)
            .context("ID token rejected")?
            .claims;
        if !self.config.nonce.accepts(claims.nonce.as_deref(), nonce) {
            bail!("ID token nonce mismatch");
        }
        // OIDC Core 3.1.3.7: an `azp` names the party the token was issued to; it must be us.
        if claims.azp.is_some_and(|azp| azp != self.config.client_id) {
            bail!("ID token azp is another client");
        }
        Ok(claims.rest)
    }

    fn cached(&self) -> Result<Option<(Arc<JwkSet>, Duration)>> {
        Ok(self
            .jwks
            .lock()
            .map_err(|_| anyhow!("JWKS lock poisoned"))?
            .as_ref()
            .map(|c| (c.set.clone(), c.fetched.elapsed())))
    }

    async fn key(&self, kid: &str) -> Result<DecodingKey> {
        let fresh = |age: Duration| age < self.config.jwks_ttl;
        let set = match self.cached()? {
            Some((set, age)) if fresh(age) && set.find(kid).is_some() => set,
            _ => {
                let _single = self.refetch.lock().await;
                // Whoever held the lock may have refreshed the set meanwhile; an unknown `kid` in a
                // fresh set refetches only once it is a minute old.
                match self.cached()? {
                    Some((set, age))
                        if fresh(age) && (set.find(kid).is_some() || age < JWKS_MIN_REFETCH) =>
                    {
                        set
                    }
                    stale => match self.fetch_jwks().await {
                        Ok(set) => set,
                        Err(e) => {
                            let Some((set, _)) = stale else { return Err(e) };
                            tracing::warn!(
                                error = format!("{e:#}"),
                                "JWKS refetch failed, using the cached keys"
                            );
                            set
                        }
                    },
                }
            }
        };
        let jwk = set
            .find(kid)
            .with_context(|| format!("no JWK for kid {kid}"))?;
        DecodingKey::from_jwk(jwk).context("JWK")
    }

    async fn fetch_jwks(&self) -> Result<Arc<JwkSet>> {
        let resp = self
            .http
            .get(&self.config.jwks_uri)
            .send()
            .await
            .context("JWKS request")?;
        if !resp.status().is_success() {
            bail!("JWKS endpoint answered {}", resp.status());
        }
        let body = read_limited(resp).await?;
        let set: Arc<JwkSet> = Arc::new(serde_json::from_slice(&body).context("JWKS body")?);
        *self
            .jwks
            .lock()
            .map_err(|_| anyhow!("JWKS lock poisoned"))? = Some(CachedJwks {
            set: set.clone(),
            fetched: Instant::now(),
        });
        Ok(set)
    }
}

/// The claim (or userinfo field) `name` as an identity subject: a non-empty string, or an integer
/// rendered in decimal.
fn claim_subject(claims: &Map<String, Value>, name: &str) -> Result<String> {
    let subject = match claims.get(name) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) if n.is_u64() || n.is_i64() => n.to_string(),
        _ => bail!("no usable {name} claim"),
    };
    if subject.is_empty() || subject.len() > MAX_SUBJECT_LEN {
        bail!("{name} claim is empty or too long");
    }
    Ok(subject)
}

/// Graph API `appsecret_proof`: lowercase hex HMAC-SHA256 of the access token, keyed with the
/// app secret.
pub fn appsecret_proof(app_secret: &str, access_token: &str) -> Result<String> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(app_secret.as_bytes()).context("appsecret_proof key")?;
    mac.update(access_token.as_bytes());
    Ok(mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

async fn read_limited(mut resp: reqwest::Response) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.context("reading provider response")? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            bail!("provider response too large");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::config;
    use super::*;

    #[test]
    fn authorize_url_carries_scope_pkce_state_and_nonce() {
        let creds = config::Credentials {
            google_id: Some("cid".into()),
            google_secret: Some("s".into()),
            ..config::Credentials::default()
        };
        let cfg = config::from_env(Some("https://a.cz"), creds)
            .expect("valid")
            .remove(0);
        let oidc = Oidc::new(cfg, reqwest::Client::new());
        let url = Url::parse(&oidc.authorize_url("st", "no", "ch").expect("url")).expect("parse");
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["scope"], "openid");
        assert_eq!(q["state"], "st");
        assert_eq!(q["nonce"], "no");
        assert_eq!(q["code_challenge"], "ch");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(
            q["redirect_uri"],
            "https://a.cz/api/auth/oauth/google/callback"
        );
    }

    #[tokio::test]
    async fn userinfo_errors_never_carry_the_appsecret_proof() {
        let mut cfg = config::facebook(Some("https://a.cz"), Some("app".into()), Some("s".into()))
            .expect("valid")
            .expect("on");
        let SubjectSource::Userinfo(info) = &mut cfg.subject else {
            panic!("userinfo");
        };
        info.endpoint = "http://127.0.0.1:1/me".into();
        let info = info.clone();
        let oidc = Oidc::new(cfg, reqwest::Client::new());
        let err = oidc
            .api_get(&info.endpoint, &info, "TOKEN", &[("fields", "id")])
            .await
            .expect_err("nothing listens on port 1");
        let shown = format!("{err:#} {err:?}");
        let proof = appsecret_proof("s", "TOKEN").expect("proof");
        assert!(
            !shown.contains(&proof) && !shown.contains("TOKEN"),
            "{shown}"
        );
    }

    #[test]
    fn appsecret_proof_matches_a_known_hmac_sha256() {
        // RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
        assert_eq!(
            appsecret_proof("Jefe", "what do ya want for nothing?").expect("proof"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn subject_claim_is_a_string_or_a_decimal_integer() {
        let claims = |v: Value| v.as_object().cloned().expect("object");
        let c =
            claims(json!({ "sub": "1043", "id": 123456789012_u64, "neg": -5, "f": 1.5, "e": "" }));
        assert_eq!(claim_subject(&c, "sub").ok().as_deref(), Some("1043"));
        assert_eq!(
            claim_subject(&c, "id").ok().as_deref(),
            Some("123456789012")
        );
        assert_eq!(claim_subject(&c, "neg").ok().as_deref(), Some("-5"));
        for bad in ["f", "e", "missing"] {
            assert!(claim_subject(&c, bad).is_err(), "{bad}");
        }
        let long = claims(json!({ "sub": "x".repeat(256) }));
        assert!(claim_subject(&long, "sub").is_err());
    }
}
