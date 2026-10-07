mod common;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use common::TestApp;
use http_body_util::BodyExt;
use rust_embed::RustEmbed;
use tower::ServiceExt;

#[derive(RustEmbed)]
#[folder = "tests/fixtures/dist"]
struct Fixture;

/// Embeds a folder that does not exist: what CI builds when the frontend was never built.
#[derive(RustEmbed)]
#[folder = "tests/fixtures/no-such-dist"]
#[allow_missing = true]
struct Missing;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

async fn send(
    app: &TestApp,
    method: Method,
    path: &str,
    if_none_match: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(tag) = if_none_match {
        req = req.header(header::IF_NONE_MATCH, tag);
    }
    let req = req.body(Body::empty()).expect("build request");
    let resp = app.router.clone().oneshot(req).await.expect("infallible");
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.expect("body").to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (parts.status, parts.headers, text)
}

fn h(headers: &HeaderMap, name: header::HeaderName) -> &str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[tokio::test]
async fn serves_files_with_mime_and_cache_policy() {
    let app = TestApp::with_frontend::<Fixture>().await;

    let (status, headers, body) = send(&app, Method::GET, "/assets/index-AbC123.js", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(h(&headers, header::CONTENT_TYPE).contains("javascript"));
    assert_eq!(h(&headers, header::CACHE_CONTROL), IMMUTABLE);
    assert!(body.contains("console.log"));

    let (status, headers, _) = send(&app, Method::GET, "/sw.js", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h(&headers, header::CACHE_CONTROL), "no-cache");

    let (status, headers, _) = send(&app, Method::GET, "/manifest.webmanifest", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h(&headers, header::CACHE_CONTROL), "no-cache");
    assert_eq!(
        h(&headers, header::CONTENT_TYPE),
        "application/manifest+json"
    );
}

#[tokio::test]
async fn spa_routes_fall_back_to_index_but_missing_files_404() {
    let app = TestApp::with_frontend::<Fixture>().await;

    for path in ["/", "/nearby", "/matches/5f0c3a", "/index.html"] {
        let (status, headers, body) = send(&app, Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(h(&headers, header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(h(&headers, header::CACHE_CONTROL), "no-cache", "{path}");
        assert!(body.contains("fixture"), "{path}");
    }

    let (status, _, body) = send(&app, Method::HEAD, "/nearby", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());

    // The last one would resolve to this test file if debug-mode disk reads allowed traversal.
    for path in ["/assets/gone-123.js", "/favicon.ico", "/../../web.rs"] {
        let (status, _, _) = send(&app, Method::GET, path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }

    let (status, headers, _) = send(&app, Method::POST, "/nearby", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(h(&headers, header::ALLOW), "GET, HEAD");
}

#[tokio::test]
async fn backend_prefixes_never_get_the_spa_shell() {
    let app = TestApp::with_frontend::<Fixture>().await;

    for path in ["/api/nope", "/api/me/nope/deeper"] {
        let (status, headers, body) = send(&app, Method::GET, path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(h(&headers, header::CONTENT_TYPE).starts_with("application/json"));
        assert!(body.contains("\"not_found\""), "{path}: {body}");
    }

    let (status, _, body) = send(&app, Method::GET, "/media/nope.webp", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("fixture"));

    let (status, _, _) = send(&app, Method::GET, "/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn matching_etag_revalidates_with_304() {
    let app = TestApp::with_frontend::<Fixture>().await;
    let (_, headers, _) = send(&app, Method::GET, "/nearby", None).await;
    let etag = h(&headers, header::ETAG).to_owned();
    assert!(etag.starts_with('"') && etag.len() > 2);

    let (status, headers, body) = send(&app, Method::GET, "/", Some(&etag)).await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
    assert_eq!(h(&headers, header::CACHE_CONTROL), "no-cache");

    let (status, _, _) = send(&app, Method::GET, "/sw.js", Some(&etag)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn without_a_bundle_everything_non_api_is_404() {
    let app = TestApp::with_frontend::<Missing>().await;
    for path in ["/", "/nearby", "/sw.js"] {
        let (status, _, _) = send(&app, Method::GET, path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _, _) = send(&app, Method::GET, "/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
}
