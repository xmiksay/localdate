//! The built PWA (`frontend/dist`) served from the binary as the router fallback.
//! `/api` and `/media` are matched first in `lib.rs`, so they never reach this module.

use axum::body::Body;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rust_embed::{EmbeddedFile, RustEmbed};

/// Release builds embed the bundle, debug builds read it from disk. `allow_missing` lets the crate
/// build without a frontend build (CI lint/test); every lookup then misses and answers 404.
#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
#[allow_missing = true]
pub struct Dist;

const INDEX: &str = "index.html";
/// Vite content-hashes everything under `assets/`, so a name never changes meaning.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Unhashed entry points (index.html, sw.js, workbox, manifest, icons) must revalidate, or a
/// deploy would keep clients on stale bundles; the ETag makes that revalidation a cheap 304.
const NO_CACHE: &str = "no-cache";

/// Fallback handler: an embedded file, else the SPA shell for route-like paths, else 404.
pub async fn serve<E: RustEmbed>(method: Method, uri: Uri, headers: HeaderMap) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
        )
            .into_response();
    }
    let path = uri.path().trim_start_matches('/');
    let found = (!path.is_empty()).then(|| E::get(path)).flatten();
    match found {
        Some(file) => respond(path, &file, &headers),
        None if looks_like_file(path) => StatusCode::NOT_FOUND.into_response(),
        None => match E::get(INDEX) {
            Some(file) => respond(INDEX, &file, &headers),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

fn respond(name: &str, file: &EmbeddedFile, headers: &HeaderMap) -> Response {
    let etag = format!(
        "\"{}\"",
        URL_SAFE_NO_PAD.encode(file.metadata.sha256_hash())
    );
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| etag_matches(v, &etag));
    let common = [
        (header::ETAG, etag),
        (header::CACHE_CONTROL, cache_control(name).to_owned()),
    ];
    if fresh {
        return (StatusCode::NOT_MODIFIED, common).into_response();
    }
    let content_type = [(header::CONTENT_TYPE, file.metadata.mimetype().to_owned())];
    (common, content_type, Body::from(file.data.clone())).into_response()
}

fn cache_control(name: &str) -> &'static str {
    if name.starts_with("assets/") {
        IMMUTABLE
    } else {
        NO_CACHE
    }
}

/// A missing `/foo.js` is a broken reference, not a client-side route; answering it with
/// index.html would hand the browser HTML under a script MIME expectation.
fn looks_like_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
}

fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match
        .split(',')
        .map(str::trim)
        .any(|tag| tag == "*" || tag.trim_start_matches("W/") == etag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_assets_are_immutable_everything_else_revalidates() {
        assert_eq!(cache_control("assets/index-AbC123.js"), IMMUTABLE);
        assert_eq!(cache_control("assets/sub/font-x.woff2"), IMMUTABLE);
        for name in [
            "index.html",
            "sw.js",
            "workbox-9c191d2f.js",
            "registerSW.js",
            "manifest.webmanifest",
            "icon.svg",
        ] {
            assert_eq!(cache_control(name), NO_CACHE, "{name}");
        }
    }

    #[test]
    fn file_like_paths_have_an_extension_in_the_last_segment() {
        assert!(looks_like_file("favicon.ico"));
        assert!(looks_like_file("assets/missing-123.js"));
        assert!(!looks_like_file(""));
        assert!(!looks_like_file("nearby"));
        assert!(!looks_like_file("matches/5f0c3a"));
        assert!(!looks_like_file("v1.2/people"));
    }

    #[test]
    fn if_none_match_handles_lists_weak_tags_and_wildcard() {
        let etag = "\"abc\"";
        assert!(etag_matches("\"abc\"", etag));
        assert!(etag_matches("\"x\", W/\"abc\"", etag));
        assert!(etag_matches("*", etag));
        assert!(!etag_matches("\"abd\"", etag));
        assert!(!etag_matches("abc", etag));
    }
}
