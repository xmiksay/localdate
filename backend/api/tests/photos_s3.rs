//! Photos stored in an S3 bucket (#21), against the in-process fake from `common::s3`.

mod common;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use common::TestApp;
use common::photos::png_bytes;
use common::s3::dead_s3;
use http_body_util::BodyExt;
use localdate_api::media::PhotoStore;
use serde_json::Value;
use tower::ServiceExt;

fn file_name(photo: &Value) -> String {
    photo["url"]
        .as_str()
        .and_then(|u| u.strip_prefix("/media/"))
        .expect("media url")
        .to_owned()
}

/// Raw `/media` request: status, headers, body bytes.
async fn media(
    app: &TestApp,
    method: Method,
    path: &str,
    if_none_match: Option<&str>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(tag) = if_none_match {
        req = req.header(header::IF_NONE_MATCH, tag);
    }
    let resp = app
        .router
        .clone()
        .oneshot(req.body(Body::empty()).expect("request"))
        .await
        .expect("infallible");
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.expect("body").to_bytes().to_vec();
    (parts.status, parts.headers, bytes)
}

fn h(headers: &HeaderMap, name: header::HeaderName) -> &str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[tokio::test]
async fn upload_serve_and_delete_go_through_the_bucket() {
    let (app, s3) = TestApp::with_s3().await;
    let t = app.register("eva").await;
    let photo = app.upload_photo(&t, 2000, 1000).await;
    let name = file_name(&photo);
    assert_eq!(
        s3.keys(),
        std::slice::from_ref(&name),
        "stored at the bucket root by name"
    );
    assert_eq!(app.photo_files(), 0, "nothing on disk");
    let stored = s3.object(&name).expect("object");
    assert_eq!(
        image::guess_format(&stored).expect("format"),
        image::ImageFormat::WebP
    );

    let path = format!("/media/{name}");
    let (status, headers, body) = media(&app, Method::GET, &path, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, stored, "streams the stored bytes");
    assert_eq!(h(&headers, header::CONTENT_TYPE), "image/webp");
    assert_eq!(
        h(&headers, header::CACHE_CONTROL),
        "private, max-age=31536000, immutable"
    );
    assert_eq!(
        h(&headers, header::CONTENT_LENGTH),
        stored.len().to_string()
    );
    let etag = h(&headers, header::ETAG).to_owned();
    assert!(etag.starts_with('"') && etag.len() > 2, "{etag}");

    let (status, headers, body) = media(&app, Method::HEAD, &path, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());
    assert_eq!(
        h(&headers, header::CONTENT_LENGTH),
        stored.len().to_string()
    );
    assert_eq!(h(&headers, header::ETAG), etag);

    let gets = s3.gets();
    let weak_in_list = format!("\"other\", W/{etag}");
    for sent in [etag.as_str(), weak_in_list.as_str(), "*"] {
        let (status, headers, body) = media(&app, Method::GET, &path, Some(sent)).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED, "{sent}");
        assert!(body.is_empty());
        assert_eq!(h(&headers, header::ETAG), etag);
    }
    assert_eq!(s3.gets(), gets, "a 304 never downloads the object");
    let (status, _, body) = media(&app, Method::GET, &path, Some("\"stale\"")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, stored);

    let id = photo["id"].as_str().expect("id");
    let (status, _) = app
        .delete(&format!("/api/me/photos/{id}"), &t.access_token)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(s3.keys().is_empty());
    let (status, _, _) = media(&app, Method::GET, &path, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = media(&app, Method::HEAD, &path, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn media_refuses_anything_but_a_photo_name() {
    let (app, _s3) = TestApp::with_s3().await;
    let t = app.register("eva").await;
    let name = file_name(&app.upload_photo(&t, 8, 8).await);
    // An object that is in the bucket but not a photo name must stay unreachable.
    let store = &app.state.photos;
    store
        .put("secret.txt", b"not public".to_vec())
        .await
        .expect("put");
    for path in [
        "/media/secret.txt".to_owned(),
        "/media/../secret.txt".to_owned(),
        "/media/%2e%2e%2fsecret.txt".to_owned(),
        format!("/media/x/{name}"),
        format!("/media/{}", name.to_uppercase()),
        format!("/media/{name}.tmp"),
        "/media/".to_owned(),
        "/media".to_owned(),
        "/media/nothing-here".to_owned(),
    ] {
        let (status, _, body) = media(&app, Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(!body.starts_with(b"not public"), "{path}");
    }
    let (status, _, _) = media(&app, Method::GET, &format!("/media/{name}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = media(&app, Method::POST, &format!("/media/{name}"), None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn failed_put_is_a_500_and_leaves_no_row() {
    let (app, s3) = TestApp::with_s3().await;
    let t = app.register("eva").await;
    s3.read_only(true);
    let (status, body) = app
        .post_multipart("/api/me/photos", &t.access_token, "file", &png_bytes(8, 8))
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["error"]["code"], "internal");
    assert!(
        !body.to_string().contains(common::s3::BUCKET),
        "no storage detail leaks: {body}"
    );
    assert_eq!(app.photo_count(t.user_id).await, 0);
    assert!(s3.keys().is_empty());
}

#[tokio::test]
async fn failed_insert_removes_the_object_again() {
    let (app, s3) = TestApp::with_s3().await;
    let t = app.register("eva").await;
    for _ in 0..6 {
        app.upload_photo(&t, 8, 8).await;
    }
    // Bypass the early count check: `add` itself must clean up when the locked check refuses.
    let err =
        localdate_api::me::photos::add(&app.state.db, &app.state.photos, t.user_id, vec![1, 2, 3])
            .await
            .expect_err("seventh photo");
    assert!(matches!(err, localdate_api::error::AppError::PhotoLimit));
    assert_eq!(s3.keys().len(), 6);
}

#[tokio::test]
async fn account_deletion_removes_the_objects_even_when_some_fail() {
    let (app, s3) = TestApp::with_s3().await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    let eva_photo = file_name(&app.upload_photo(&eva, 8, 8).await);
    app.upload_photo(&eva, 8, 8).await;
    let bob_photo = file_name(&app.upload_photo(&bob, 8, 8).await);

    let (status, _) = app.delete("/api/me", &eva.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(s3.keys(), [bob_photo]);
    assert!(s3.object(&eva_photo).is_none());

    // Storage down: the account is still deleted; the object stays behind (logged).
    s3.read_only(true);
    let (status, _) = app.delete("/api/me", &bob.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        common::count(&app, "SELECT count(*) FROM \"user\"").await,
        0
    );
    assert_eq!(s3.keys().len(), 1);
}

#[tokio::test]
async fn startup_check_needs_a_writable_bucket() {
    let (app, s3) = TestApp::with_s3().await;
    app.state
        .photos
        .check()
        .await
        .expect("fake bucket is writable");
    assert!(s3.keys().is_empty(), "probe object deleted again");

    // Refusals fail at once and are not worth waiting for.
    let started = std::time::Instant::now();
    let wrong = PhotoStore::new(&s3.config_for_bucket("no-such-bucket")).expect("builds");
    let err = wrong.check().await.expect_err("missing bucket");
    assert!(!PhotoStore::is_transient(&err), "{err}");
    s3.read_only(true);
    let err = app.state.photos.check().await.expect_err("read-only key");
    assert!(!PhotoStore::is_transient(&err), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));

    // An outage is.
    let dead = PhotoStore::new(&dead_s3()).expect("builds without touching the network");
    let err = dead.check().await.expect_err("nothing listens");
    assert!(PhotoStore::is_transient(&err), "{err}");
}

#[tokio::test]
async fn imported_profile_picture_lands_in_the_bucket() {
    let facebook = common::facebook::FakeFacebook::start().await;
    let s3 = common::s3::FakeS3::start().await;
    let (oauth, storage) = (facebook.config(), s3.config());
    let app = TestApp::with_config(|c| {
        c.oauth = vec![oauth];
        c.photo_storage = storage;
    })
    .await;
    let (t, fragment) = app.fb_signup(&facebook, "50077", "pic", true).await;
    assert_eq!(fragment["photo"], "pending");
    assert_eq!(app.photo_count(t.user_id).await, 1);
    let keys = s3.keys();
    assert_eq!(keys.len(), 1);
    let stored = s3.object(&keys[0]).expect("object");
    assert_eq!(
        image::guess_format(&stored).expect("format"),
        image::ImageFormat::WebP
    );
    assert_eq!(app.photo_files(), 0);
}
