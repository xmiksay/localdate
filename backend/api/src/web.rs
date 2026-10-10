//! The built PWA (`frontend/dist`) served from the binary as the router fallback.
//! `/api` and `/media` are matched first in `lib.rs`; what they leave unmatched gets a 404 here.

use axum::body::Body;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use percent_encoding::percent_decode_str;
use rust_embed::{EmbeddedFile, RustEmbed};

/// Release builds embed the bundle, debug builds read it from disk. `allow_missing` lets the crate
/// build without a frontend build (CI lint/test); every lookup then misses and answers 404.
#[derive(RustEmbed)]
#[folder = "../../frontend/dist"]
#[allow_missing = true]
pub struct Dist;

const INDEX: &str = "index.html";
/// Vite content-hashes everything under `assets/`, so a name never changes meaning.
pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// Unhashed entry points (index.html, sw.js, workbox, manifest, icons) must revalidate, or a
/// deploy would keep clients on stale bundles; the ETag makes that revalidation a cheap 304.
pub const NO_CACHE: &str = "no-cache";

/// Fallback handler: an embedded file, else the SPA shell for route-like paths, else 404.
pub async fn serve<E: RustEmbed>(method: Method, uri: Uri, headers: HeaderMap) -> Response {
    if reserved(uri.path()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
        )
            .into_response();
    }
    let Some(path) = asset_name(uri.path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match E::get(&path) {
        Some(file) => respond(&path, &file, &headers),
        None if looks_like_file(&path) => StatusCode::NOT_FOUND.into_response(),
        None => match E::get(INDEX) {
            Some(file) => respond(INDEX, &file, &headers),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

/// Paths of the API and media namespaces that their routers did not match (`/media/`, `/api`,
/// `/media/x/y`, also percent-encoded): a plain 404, never the SPA shell, so a client or test can
/// never mistake `index.html` for an API or photo answer.
fn reserved(uri_path: &str) -> bool {
    let decoded = percent_decode_str(uri_path).decode_utf8_lossy();
    let first = decoded
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("");
    matches!(first, "api" | "media")
}

/// The percent-decoded bundle-relative name, or `None` if it could leave the bundle folder.
/// Debug builds resolve names against the disk (and rust-embed follows a symlink in the last
/// component), so `..`, `.`, empty (absolute-making) segments, `\` and NUL are refused up front.
fn asset_name(uri_path: &str) -> Option<String> {
    let decoded = percent_decode_str(uri_path).decode_utf8().ok()?;
    let name = decoded.strip_prefix('/').unwrap_or(&decoded);
    let dir_like = name.strip_suffix('/').unwrap_or(name);
    let safe = dir_like.is_empty()
        || dir_like
            .split('/')
            .all(|s| !matches!(s, "" | "." | "..") && !s.contains(['\\', '\0']));
    safe.then(|| name.to_owned())
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
    fn asset_names_are_decoded_and_cannot_escape_the_bundle() {
        assert_eq!(asset_name("/").as_deref(), Some(""));
        assert_eq!(asset_name("/nearby/").as_deref(), Some("nearby/"));
        assert_eq!(
            asset_name("/assets/a-1.js").as_deref(),
            Some("assets/a-1.js")
        );
        assert_eq!(asset_name("/caf%C3%A9.png").as_deref(), Some("café.png"));
        for bad in [
            "/../Cargo.toml",
            "/assets/../sw.js",
            "/%2e%2e/Cargo.toml",
            "/assets/%2E%2E/sw.js",
            "/./sw.js",
            "//etc/passwd",
            "/%2fetc/passwd",
            "/assets//a.js",
            "/a%5c..%5cb",
            "/a%00.js",
            "/%ff",
        ] {
            assert_eq!(asset_name(bad), None, "{bad}");
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
