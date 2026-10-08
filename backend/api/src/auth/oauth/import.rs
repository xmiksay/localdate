//! Optional import of the provider's profile picture (Facebook, #17), asked for at `start` / `link`.
//! It runs inside the callback, the only moment the provider access token exists: the picture is
//! looked up, downloaded from the provider's CDN and normalised by the photo pipeline; the token is
//! dropped afterwards. A sign-up keeps the processed image in its grant until the username step.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use url::{Host, Url};
use uuid::Uuid;

use super::config::{PictureSource, SubjectSource, Userinfo};
use super::oidc::Oidc;
use crate::error::AppError;
use crate::me::photos;
use crate::state::AppState;

/// A processed picture waiting in a sign-up grant (DB `CHECK` in migration 000014 matches).
pub const MAX_PENDING_PHOTO_BYTES: usize = 8 * 1024 * 1024;
/// Asked size; the provider answers the largest it has up to this (our pipeline caps at 1280 px).
const PICTURE_EDGE: &str = "1280";

/// What happened to the import, for the done page's `photo=` fragment value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Sign-up: fetched and held by the sign-up grant; the result comes with the signup response.
    Pending,
    Imported,
    /// The account already has the maximum number of photos.
    Full,
    /// The provider account has only the default silhouette.
    None,
    Failed,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Imported => "imported",
            Self::Full => "full",
            Self::None => "none",
            Self::Failed => "failed",
        }
    }
}

/// The picture settings of `oidc`'s provider, if it offers an import at all.
pub fn source(oidc: &Oidc) -> Option<(&Userinfo, &PictureSource)> {
    match &oidc.config().subject {
        SubjectSource::Userinfo(info) => info.picture.as_ref().map(|p| (info, p)),
        SubjectSource::IdTokenClaim(_) => None,
    }
}

/// The provider picture as normalised WebP, or why there is none. Failures are logged, never fatal.
pub async fn fetch(state: &AppState, oidc: &Oidc, access_token: &str) -> Result<Vec<u8>, Outcome> {
    let Some((info, picture)) = source(oidc) else {
        return Err(Outcome::None);
    };
    let found = async {
        let Some(url) = picture_url(oidc, info, picture, access_token).await? else {
            return Ok(None);
        };
        download(oidc.http(), &url).await.map(Some)
    };
    let raw = match found.await {
        Ok(Some(raw)) => raw,
        Ok(None) => return Err(Outcome::None),
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "profile picture download refused");
            return Err(Outcome::Failed);
        }
    };
    match photos::to_webp(state, raw).await {
        Ok(webp) if webp.len() <= MAX_PENDING_PHOTO_BYTES => Ok(webp),
        Ok(_) | Err(AppError::UnsupportedImage) => Err(Outcome::Failed),
        Err(e) => {
            tracing::error!(error = %e, "profile picture processing failed");
            Err(Outcome::Failed)
        }
    }
}

/// Adds `webp` as the user's last photo.
pub async fn attach(state: &AppState, user_id: Uuid, webp: &[u8]) -> Outcome {
    match photos::add(state, user_id, webp).await {
        Ok(_) => Outcome::Imported,
        Err(AppError::PhotoLimit) => Outcome::Full,
        Err(e) => {
            tracing::error!(error = %e, "storing an imported photo failed");
            Outcome::Failed
        }
    }
}

/// Graph `/me/picture?redirect=false`: the CDN URL, or `None` for the default silhouette.
async fn picture_url(
    oidc: &Oidc,
    info: &Userinfo,
    picture: &PictureSource,
    access_token: &str,
) -> Result<Option<Url>> {
    let answer = oidc
        .api_get(
            &picture.endpoint,
            info,
            access_token,
            &[
                ("redirect", "false"),
                ("width", PICTURE_EDGE),
                ("height", PICTURE_EDGE),
            ],
        )
        .await?;
    let data = answer.get("data").context("picture answer without data")?;
    if data.get("is_silhouette") == Some(&Value::Bool(true)) {
        return Ok(None);
    }
    let raw = data
        .get("url")
        .and_then(Value::as_str)
        .context("picture answer without url")?;
    allowed_url(raw, picture).map(Some)
}

/// `raw` if it points at the provider's CDN: https (unless the source allows http) on the default
/// port, no credentials, and a host that is an allowed name or a subdomain of one — an IP literal
/// or any other host is refused, so the server can't be pointed at internal addresses.
pub fn allowed_url(raw: &str, picture: &PictureSource) -> Result<Url> {
    let url = Url::parse(raw).context("picture URL")?;
    let scheme_ok = match url.scheme() {
        "https" => true,
        "http" => !picture.https_only,
        _ => false,
    };
    if !scheme_ok {
        bail!("picture URL scheme {} refused", url.scheme());
    }
    if picture.https_only && url.port().is_some() {
        bail!("picture URL with an explicit port refused");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("picture URL with credentials refused");
    }
    let host = match url.host() {
        Some(Host::Domain(d)) => d.to_owned(),
        Some(Host::Ipv4(ip)) => ip.to_string(),
        Some(Host::Ipv6(ip)) => ip.to_string(),
        None => bail!("picture URL without a host"),
    };
    let allowed = picture
        .hosts
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")));
    if !allowed {
        bail!("picture host {host} is not allowed");
    }
    Ok(url)
}

/// The image bytes, at most `photos::MAX_UPLOAD_BYTES`; the client follows no redirects, so the
/// host check above is the only host ever contacted.
async fn download(http: &reqwest::Client, url: &Url) -> Result<Vec<u8>> {
    let mut resp = http
        .get(url.clone())
        .send()
        .await
        // CDN URLs are signed (`oh=`, `oe=`); keep them out of the logged error.
        .map_err(reqwest::Error::without_url)
        .context("picture request")?;
    if !resp.status().is_success() {
        bail!("picture CDN answered {}", resp.status());
    }
    let cap = photos::MAX_UPLOAD_BYTES;
    if resp.content_length().is_some_and(|len| len > cap as u64) {
        bail!("picture larger than {cap} bytes");
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(reqwest::Error::without_url)
        .context("reading picture")?
    {
        if body.len() + chunk.len() > cap {
            bail!("picture larger than {cap} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cdn(https_only: bool) -> PictureSource {
        PictureSource {
            endpoint: "https://graph.test/me/picture".into(),
            hosts: vec!["fbcdn.net".into()],
            https_only,
        }
    }

    #[test]
    fn only_https_urls_on_the_cdn_are_allowed() {
        for ok in [
            "https://scontent.xx.fbcdn.net/v/t1/p.jpg?_nc=1",
            "https://fbcdn.net/p.jpg",
            "https://SCONTENT.FBCDN.NET/p.jpg",
        ] {
            assert!(allowed_url(ok, &cdn(true)).is_ok(), "{ok}");
        }
        for bad in [
            "http://scontent.fbcdn.net/p.jpg",
            "https://scontent.fbcdn.net:8443/p.jpg",
            "https://user:pw@scontent.fbcdn.net/p.jpg",
            "https://evilfbcdn.net/p.jpg",
            "https://fbcdn.net.evil.com/p.jpg",
            "https://scontent.fbcdn.net.:443/p.jpg",
            "https://127.0.0.1/p.jpg",
            "https://[::1]/p.jpg",
            "https://169.254.169.254/latest/meta-data",
            "file:///etc/passwd",
            "ftp://scontent.fbcdn.net/p.jpg",
            "not a url",
        ] {
            assert!(allowed_url(bad, &cdn(true)).is_err(), "{bad}");
        }
    }

    #[test]
    fn http_and_ports_only_when_the_source_allows_it() {
        let test = PictureSource {
            hosts: vec!["127.0.0.1".into()],
            ..cdn(false)
        };
        assert!(allowed_url("http://127.0.0.1:4000/p.png", &test).is_ok());
        assert!(allowed_url("http://localhost:4000/p.png", &test).is_err());
        assert!(allowed_url("http://127.0.0.2:4000/p.png", &test).is_err());
    }

    #[tokio::test]
    async fn download_errors_never_carry_the_signed_url() {
        let url = Url::parse("http://127.0.0.1:1/p.jpg?oh=SIGNATURE&oe=EXPIRY").expect("url");
        let err = download(&reqwest::Client::new(), &url)
            .await
            .expect_err("nothing listens on port 1");
        let shown = format!("{err:#} {err:?}");
        assert!(!shown.contains("SIGNATURE"), "{shown}");
        assert!(!shown.contains("127.0.0.1:1"), "{shown}");
    }

    #[test]
    fn outcome_names_match_the_contract() {
        let all = [
            Outcome::Pending,
            Outcome::Imported,
            Outcome::Full,
            Outcome::None,
            Outcome::Failed,
        ];
        assert_eq!(
            all.map(Outcome::as_str),
            ["pending", "imported", "full", "none", "failed"]
        );
    }
}
