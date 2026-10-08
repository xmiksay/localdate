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

// https://oauth.telegram.org/.well-known/openid-configuration
const TELEGRAM_AUTH: &str = "https://oauth.telegram.org/auth";
const TELEGRAM_TOKEN: &str = "https://oauth.telegram.org/token";
const TELEGRAM_JWKS: &str = "https://oauth.telegram.org/.well-known/jwks.json";
const TELEGRAM_ISSUER: &str = "https://oauth.telegram.org";
/// `profile` is what carries `id`, the numeric user id the bot messages (`sub` is a different,
/// opaque value); `telegram:bot_access` lets the bot send the password-reset link. The name,
/// username and photo `profile` also returns are never stored.
const TELEGRAM_SCOPE: &str = "openid profile telegram:bot_access";
const TELEGRAM_JWKS_TTL: Duration = Duration::from_secs(60 * 60);

/// Where the provider's stable account id (the identity subject) comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubjectSource {
    /// A claim of the verified ID token: a string as is, an integer in decimal (Telegram's `id`).
    IdTokenClaim(String),
    /// A field of a userinfo-style endpoint called with the access token (Facebook's Graph `/me`),
    /// for providers whose web login hands out no ID token.
    Userinfo(Userinfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Userinfo {
    /// Called as `GET {endpoint}?fields={field}` with `Authorization: Bearer <access token>`.
    pub endpoint: String,
    pub field: String,
    /// Adds Graph's `appsecret_proof` (HMAC-SHA256 of the access token keyed with the client
    /// secret), so a leaked access token alone cannot call the API as this app.
    pub appsecret_proof: bool,
    /// Where the profile picture can be imported from; `None` = the provider offers no import.
    pub picture: Option<PictureSource>,
}

/// The provider's profile picture: a JSON endpoint naming an image URL on the provider's CDN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureSource {
    /// Graph `/me/picture`, called with `redirect=false` and the same token (+ proof) as userinfo.
    pub endpoint: String,
    /// The image may only come from these hosts or their subdomains (`fbcdn.net`).
    pub hosts: Vec<String>,
    /// Off only in tests, whose fake CDN is plain http on 127.0.0.1.
    pub https_only: bool,
}

/// How the ID token's `nonce` is checked against the flow cookie's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceCheck {
    /// Must be present and equal.
    Required,
    /// Must equal when present; absent is accepted (Telegram's server-side flow may not echo it).
    /// `state`, PKCE and the cookie binding of the code still stop code injection.
    IfPresent,
}

impl NonceCheck {
    /// Whether the token's `claim` passes for the flow's `expected` nonce.
    pub fn accepts(self, claim: Option<&str>, expected: &str) -> bool {
        match (self, claim) {
            (_, Some(n)) => super::cookie::ct_eq(n, expected),
            (Self::Required, None) => false,
            (Self::IfPresent, None) => true,
        }
    }
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
    pub nonce: NonceCheck,
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

/// Raw `*_CLIENT_ID` / `*_CLIENT_SECRET` values.
#[derive(Default)]
pub struct Credentials {
    pub google_id: Option<String>,
    pub google_secret: Option<String>,
    pub telegram_id: Option<String>,
    pub telegram_secret: Option<String>,
}

/// Every configured provider; `base_url` is the validated `APP_BASE_URL`, required once any is.
pub fn from_env(base_url: Option<&str>, creds: Credentials) -> Result<Vec<OidcConfig>> {
    let mut providers = Vec::new();
    if let Some((client_id, client_secret)) = credentials(
        "GOOGLE_CLIENT_ID",
        "GOOGLE_CLIENT_SECRET",
        creds.google_id,
        creds.google_secret,
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
            nonce: NonceCheck::Required,
            jwks_ttl: GOOGLE_JWKS_TTL,
        });
    }
    if let Some((client_id, client_secret)) = credentials(
        "TELEGRAM_CLIENT_ID",
        "TELEGRAM_CLIENT_SECRET",
        creds.telegram_id,
        creds.telegram_secret,
    )? {
        let Some(base) = base_url else {
            bail!("APP_BASE_URL is required for Telegram login");
        };
        providers.push(OidcConfig {
            provider: Provider::Telegram,
            // Telegram puts the bot id into `aud`; BotFather's Client ID is that id.
            client_id,
            client_secret,
            redirect_uri: callback_uri(base, Provider::Telegram),
            issuers: vec![TELEGRAM_ISSUER.into()],
            auth_endpoint: TELEGRAM_AUTH.into(),
            token_endpoint: TELEGRAM_TOKEN.into(),
            jwks_uri: TELEGRAM_JWKS.into(),
            scope: TELEGRAM_SCOPE.into(),
            subject: SubjectSource::IdTokenClaim("id".into()),
            // Unconfirmed whether Telegram echoes it (docs/deployment.md, first-login checks).
            nonce: NonceCheck::IfPresent,
            jwks_ttl: TELEGRAM_JWKS_TTL,
        });
    }
    Ok(providers)
}

// --- Facebook (#17) ---------------------------------------------------------------------------

const FACEBOOK_GRAPH: &str = "https://graph.facebook.com/v25.0";
const FACEBOOK_AUTH: &str = "https://www.facebook.com/v25.0/dialog/oauth";

/// Facebook Login (OAuth 2.0 code flow, not OIDC): the account id is the app-scoped `id` from Graph
/// `/me`; scope `public_profile` only, which needs no App Review. `FACEBOOK_APP_ID` +
/// `FACEBOOK_APP_SECRET`, both or neither.
pub fn facebook(
    base_url: Option<&str>,
    app_id: Option<String>,
    app_secret: Option<String>,
) -> Result<Option<OidcConfig>> {
    let Some((client_id, client_secret)) =
        credentials("FACEBOOK_APP_ID", "FACEBOOK_APP_SECRET", app_id, app_secret)?
    else {
        return Ok(None);
    };
    let Some(base) = base_url else {
        bail!("APP_BASE_URL is required for Facebook login");
    };
    Ok(Some(OidcConfig {
        provider: Provider::Facebook,
        client_id,
        client_secret,
        redirect_uri: callback_uri(base, Provider::Facebook),
        // No ID token, so no issuer or signing keys.
        issuers: Vec::new(),
        auth_endpoint: FACEBOOK_AUTH.into(),
        token_endpoint: format!("{FACEBOOK_GRAPH}/oauth/access_token"),
        jwks_uri: String::new(),
        scope: "public_profile".into(),
        subject: SubjectSource::Userinfo(Userinfo {
            endpoint: format!("{FACEBOOK_GRAPH}/me"),
            field: "id".into(),
            appsecret_proof: true,
            picture: Some(PictureSource {
                endpoint: format!("{FACEBOOK_GRAPH}/me/picture"),
                hosts: vec!["fbcdn.net".into()],
                https_only: true,
            }),
        }),
        // Never consulted: the nonce is an ID token check and Userinfo has no ID token.
        nonce: NonceCheck::Required,
        jwks_ttl: Duration::ZERO,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(s: &str) -> Option<String> {
        Some(s.to_owned())
    }

    fn google(id: Option<String>, secret: Option<String>) -> Credentials {
        Credentials {
            google_id: id,
            google_secret: secret,
            ..Credentials::default()
        }
    }

    fn telegram(id: Option<String>, secret: Option<String>) -> Credentials {
        Credentials {
            telegram_id: id,
            telegram_secret: secret,
            ..Credentials::default()
        }
    }

    #[test]
    fn telegram_preset_asks_for_the_numeric_id_and_bot_access() {
        let on = from_env(Some("https://a.cz"), telegram(some("123456"), some("s"))).expect("ok");
        assert_eq!(on.len(), 1);
        let t = &on[0];
        assert_eq!(t.provider, Provider::Telegram);
        assert_eq!(
            t.redirect_uri,
            "https://a.cz/api/auth/oauth/telegram/callback"
        );
        assert_eq!(t.issuers, vec!["https://oauth.telegram.org".to_owned()]);
        assert_eq!(t.subject, SubjectSource::IdTokenClaim("id".into()));
        assert_eq!(t.nonce, NonceCheck::IfPresent);
        let scopes: Vec<&str> = t.scope.split(' ').collect();
        assert_eq!(scopes, ["openid", "profile", "telegram:bot_access"]);
        assert!(from_env(Some("https://a.cz"), telegram(some("1"), None)).is_err());
        assert!(from_env(None, telegram(some("1"), some("s"))).is_err());
        let both = Credentials {
            google_id: some("g"),
            google_secret: some("gs"),
            ..telegram(some("1"), some("s"))
        };
        assert_eq!(from_env(Some("https://a.cz"), both).expect("ok").len(), 2);
    }

    #[test]
    fn google_needs_both_credentials_and_a_base_url() {
        assert!(
            from_env(Some("https://a.cz"), google(None, some(" ")))
                .expect("off")
                .is_empty()
        );
        let on =
            from_env(Some("https://a.cz/"), google(some("id"), some("secret"))).expect("valid");
        assert_eq!(on.len(), 1);
        assert_eq!(on[0].provider, Provider::Google);
        assert_eq!(
            on[0].redirect_uri,
            "https://a.cz/api/auth/oauth/google/callback"
        );
        assert_eq!(on[0].subject, SubjectSource::IdTokenClaim("sub".into()));
        assert!(from_env(Some("https://a.cz"), google(some("id"), None)).is_err());
        assert!(from_env(None, google(some("id"), some("s"))).is_err());
    }

    #[test]
    fn nonce_check_required_or_if_present() {
        use NonceCheck::*;
        assert!(Required.accepts(Some("n"), "n"));
        assert!(!Required.accepts(Some("x"), "n"));
        assert!(!Required.accepts(None, "n"));
        assert!(IfPresent.accepts(Some("n"), "n"));
        assert!(
            !IfPresent.accepts(Some("x"), "n"),
            "a wrong nonce is never accepted"
        );
        assert!(IfPresent.accepts(None, "n"));
        let google = from_env(Some("https://a.cz"), google(some("id"), some("s"))).expect("ok");
        assert_eq!(google[0].nonce, Required);
    }

    #[test]
    fn facebook_needs_both_credentials_and_a_base_url() {
        assert!(
            facebook(Some("https://a.cz"), None, None)
                .expect("off")
                .is_none()
        );
        let on = facebook(Some("https://a.cz/"), some("app"), some("fb-app-secret"))
            .expect("valid")
            .expect("on");
        assert_eq!(on.provider, Provider::Facebook);
        assert_eq!(
            on.redirect_uri,
            "https://a.cz/api/auth/oauth/facebook/callback"
        );
        assert_eq!(on.scope, "public_profile");
        let SubjectSource::Userinfo(info) = &on.subject else {
            panic!("userinfo subject");
        };
        assert_eq!(info.field, "id");
        assert!(info.appsecret_proof);
        assert!(info.picture.as_ref().is_some_and(|p| p.https_only));
        assert!(facebook(Some("https://a.cz"), None, some("secret")).is_err());
        assert!(facebook(None, some("app"), some("secret")).is_err());
        assert!(!format!("{on:?}").contains("fb-app-secret"));
    }

    #[test]
    fn debug_never_shows_the_client_secret() {
        let cfg =
            from_env(Some("http://x"), google(some("id"), some("very-secret"))).expect("valid");
        assert!(!format!("{cfg:?}").contains("very-secret"));
    }
}
