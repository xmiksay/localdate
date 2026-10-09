//! `GET`/`HEAD /media/{name}`: photo files, streamed from the [`PhotoStore`]. Unauthenticated like
//! the `ServeDir` it replaced — names are unguessable v4 UUIDs, so `<img src>` needs no token.

pub mod store;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;
pub use store::{Meta, PhotoStore};

/// A name is written once and never reused (a new upload gets a new UUID), so a cached copy can
/// never go stale. `private`: no shared cache (CDN, proxy) keeps a photo after it was deleted.
const CACHE_FOREVER: &str = "private, max-age=31536000, immutable";

/// Nested at `/media`; its own fallback keeps every other `/media/…` path away from the SPA shell.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{name}", get(serve))
        .fallback(|| async { AppError::NotFound })
}

/// The file name of photo `id` (its row id).
pub fn file_name(id: Uuid) -> String {
    format!("{id}.webp")
}

/// Only what [`file_name`] produces (canonical lowercase hyphenated UUID + `.webp`), so no
/// request can name a path outside the photo set, or any other object in the bucket.
pub fn is_photo_name(name: &str) -> bool {
    name.strip_suffix(".webp")
        .and_then(|stem| Uuid::try_parse(stem).ok().map(|id| (stem, id)))
        .is_some_and(|(stem, id)| id.hyphenated().to_string() == stem)
}

async fn serve(
    State(state): State<AppState>,
    method: Method,
    Path(name): Path<String>,
    request_headers: HeaderMap,
) -> Result<Response, AppError> {
    if !is_photo_name(&name) {
        return Err(AppError::NotFound);
    }
    let if_none_match: Vec<&str> = request_headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    // axum routes HEAD here too. HEAD and a conditional GET look at the metadata first, so neither
    // a HEAD nor a 304 downloads the object.
    if method == Method::HEAD || !if_none_match.is_empty() {
        let meta = state.photos.head(&name).await?.ok_or(AppError::NotFound)?;
        if none_match(&if_none_match, meta.e_tag.as_deref()) {
            return Ok((StatusCode::NOT_MODIFIED, headers(&meta, false)).into_response());
        }
        if method == Method::HEAD {
            return Ok((headers(&meta, true), Body::empty()).into_response());
        }
    }
    let (meta, stream) = state.photos.get(&name).await?.ok_or(AppError::NotFound)?;
    Ok((headers(&meta, true), Body::from_stream(stream)).into_response())
}

/// Caching headers, plus the representation headers for a 200 (`full`).
fn headers(meta: &Meta, full: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_FOREVER),
    );
    if let Some(etag) = meta
        .e_tag
        .as_deref()
        .and_then(|t| HeaderValue::from_str(&quoted(t)).ok())
    {
        headers.insert(header::ETAG, etag);
    }
    if full {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(meta.size));
        headers.insert(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
    }
    headers
}

/// RFC 9110 §13.1.2 for an existing object: `*` matches; otherwise any listed entity tag that
/// equals ours under the weak comparison (`W/` ignored on both sides). Every header line counts.
fn none_match(header_lines: &[&str], etag: Option<&str>) -> bool {
    let ours = etag.map(quoted);
    let opaque = |tag: &str| tag.strip_prefix("W/").unwrap_or(tag).to_owned();
    header_lines
        .iter()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .any(|tag| tag == "*" || ours.as_deref().is_some_and(|o| opaque(o) == opaque(tag)))
}

/// S3 ETags come quoted, the disk backend's do not; HTTP wants them quoted.
fn quoted(tag: &str) -> String {
    if tag.starts_with('"') || tag.starts_with("W/\"") {
        tag.to_owned()
    } else {
        format!("\"{tag}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_generated_names_are_photo_names() {
        assert!(is_photo_name(&file_name(Uuid::new_v4())));
        assert!(is_photo_name("0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.webp"));
        for bad in [
            "",
            ".webp",
            "0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a",
            "0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.png",
            "0B7C6A1E-9D4F-4A8E-8C3B-2F1E0D9C8B7A.webp",
            "0b7c6a1e9d4f4a8e8c3b2f1e0d9c8b7a.webp",
            "{0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a}.webp",
            "urn:uuid:0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.webp",
            "../0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.webp",
            "x/0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.webp",
            "0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.webp.tmp",
            "..",
        ] {
            assert!(!is_photo_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn if_none_match_follows_rfc_9110() {
        let tag = Some("\"abc\"");
        assert!(none_match(&["\"abc\""], tag));
        assert!(none_match(&["W/\"abc\""], tag), "weak comparison");
        assert!(none_match(&["\"x\", W/\"abc\""], tag), "list");
        assert!(
            none_match(&["\"x\"", " \"abc\" "], tag),
            "several header lines"
        );
        assert!(none_match(&["*"], tag));
        assert!(none_match(&["*"], None), "* matches any existing object");
        assert!(
            none_match(&["\"abc\""], Some("abc")),
            "unquoted backend tag"
        );
        assert!(!none_match(&["\"abcd\"", "\"ab\""], tag));
        assert!(!none_match(&[], tag));
        assert!(!none_match(&["\"abc\""], None));
        assert!(!none_match(&[""], tag));
    }

    #[test]
    fn etags_are_quoted_once() {
        assert_eq!(quoted("abc"), "\"abc\"");
        assert_eq!(quoted("\"abc\""), "\"abc\"");
        assert_eq!(quoted("W/\"abc\""), "W/\"abc\"");
    }
}
