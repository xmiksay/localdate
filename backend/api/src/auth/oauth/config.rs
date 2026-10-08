//! Per-provider OpenID Connect configuration and the presets read from the environment.

use std::time::Duration;

use anyhow::{Result, bail};

use super::Provider;

const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_JWKS: &str = "https://www.googleapis.com/oauth2/v3/certs";
const GOOGLE_ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];
/// Google rotates keys about weekly and publishes the next one well ahead.
const GOOGLE_JWKS_TTL: Duration = Duration::from_secs(60 * 60);

/// Where the provider's stable account id (the identity subject) comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectSource {
    /// A claim of the verified ID token: a string as is, an integer in decimal (Telegram's `id`).
    IdTokenClaim(String),
    // Providers without a usable ID token (Facebook, #17) get a userinfo-endpoint variant here.
}

/// One OIDC client. Endpoints are fields so tests can point them at a fake provider.
#[derive(Clone)]
pub struct OidcConfig {
    pub provider: Provider,
    pub client_id: String,
    pub client_secret: String,
    /// [`callback_uri`]; registered at the provider.
    pub redirect_uri: String,
    /// Accepted `iss` values.
    pub issuers: Vec<String>,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    /// Space-separated; only what is needed to identify the account.
    pub scope: String,
    pub subject: SubjectSource,
    /// How long fetched signing keys are used before a refetch (a failed one keeps the stale set).
    pub jwks_ttl: Duration,
}

// `Config` is `Debug`; the client secret must never reach a log line through it.
impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcConfig")
            .field("provider", &self.provider)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("redirect_uri", &self.redirect_uri)
            .finish_non_exhaustive()
    }
}

/// `{APP_BASE_URL}/api/auth/oauth/{provider}/callback`, the same shape for every provider.
pub fn callback_uri(base_url: &str, provider: Provider) -> String {
    format!(
        "{}/api/auth/oauth/{}/callback",
        base_url.trim_end_matches('/'),
        provider.as_str()
    )
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Client credentials of one provider: both or neither (empty = unset).
fn credentials(
    id_var: &str,
    secret_var: &str,
    id: Option<String>,
    secret: Option<String>,
) -> Result<Option<(String, String)>> {
    match (clean(id), clean(secret)) {
        (None, None) => Ok(None),
        (Some(id), Some(secret)) => Ok(Some((id, secret))),
        _ => bail!("set both {id_var} and {secret_var}, or neither"),
    }
}

/// Every configured provider; `base_url` is the validated `APP_BASE_URL`, required once any is.
pub fn from_env(
    base_url: Option<&str>,
    google_id: Option<String>,
    google_secret: Option<String>,
) -> Result<Vec<OidcConfig>> {
    let mut providers = Vec::new();
    if let Some((client_id, client_secret)) = credentials(
        "GOOGLE_CLIENT_ID",
        "GOOGLE_CLIENT_SECRET",
        google_id,
        google_secret,
    )? {
        let Some(base) = base_url else {
            bail!("APP_BASE_URL is required for Google login");
        };
        providers.push(OidcConfig {
            provider: Provider::Google,
            client_id,
            client_secret,
            redirect_uri: callback_uri(base, Provider::Google),
            issuers: GOOGLE_ISSUERS.map(str::to_owned).to_vec(),
            auth_endpoint: GOOGLE_AUTH.into(),
            token_endpoint: GOOGLE_TOKEN.into(),
            jwks_uri: GOOGLE_JWKS.into(),
            scope: "openid".into(),
            subject: SubjectSource::IdTokenClaim("sub".into()),
            jwks_ttl: GOOGLE_JWKS_TTL,
        });
    }
    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(s: &str) -> Option<String> {
        Some(s.to_owned())
    }

    #[test]
    fn google_needs_both_credentials_and_a_base_url() {
        assert!(
            from_env(Some("https://a.cz"), None, some(" "))
                .expect("off")
                .is_empty()
        );
        let on = from_env(Some("https://a.cz/"), some("id"), some("secret")).expect("valid");
        assert_eq!(on.len(), 1);
        assert_eq!(on[0].provider, Provider::Google);
        assert_eq!(
            on[0].redirect_uri,
            "https://a.cz/api/auth/oauth/google/callback"
        );
        assert_eq!(on[0].subject, SubjectSource::IdTokenClaim("sub".into()));
        assert!(from_env(Some("https://a.cz"), some("id"), None).is_err());
        assert!(from_env(None, some("id"), some("s")).is_err());
    }

    #[test]
    fn debug_never_shows_the_client_secret() {
        let cfg = from_env(Some("http://x"), some("id"), some("very-secret")).expect("valid");
        assert!(!format!("{cfg:?}").contains("very-secret"));
    }
}
