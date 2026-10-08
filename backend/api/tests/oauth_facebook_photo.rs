//! Profile picture import during the Facebook callback (#17).

mod common;

use std::sync::atomic::Ordering;

use axum::http::{Method, StatusCode};
use common::TestApp;
use common::facebook::{FakeFacebook, Picture};
use common::oauth::FakeProvider;

/// Start with the import asked for and return as `id`; the callback's fragment.
async fn start_importing(
    app: &TestApp,
    fake: &FakeFacebook,
    id: &str,
) -> std::collections::HashMap<String, String> {
    let started = app.fb_start(true).await;
    app.fb_return(fake, &started, id).await.fragment()
}

#[tokio::test]
async fn signup_keeps_the_picture_until_the_username_step() {
    let (app, fake) = TestApp::with_facebook().await;
    let started = app.fb_start(true).await;
    let fragment = app.fb_return(&fake, &started, "50001").await.fragment();
    assert_eq!(fragment["photo"], "pending");
    assert_eq!(fake.cdn_hits.load(Ordering::SeqCst), 1);
    assert_eq!(
        common::count(
            &app,
            "SELECT count(*) FROM oauth_grant WHERE photo IS NOT NULL"
        )
        .await,
        1
    );
    let resp = app
        .oauth_exchange(Some(&started.cookie), &fragment["code"])
        .await;
    let token = resp.body["signup"]["token"].as_str().expect("token");
    assert_eq!(common::count(&app, "SELECT count(*) FROM photo").await, 0);

    let (status, body) = app
        .post(
            "/api/auth/oauth/signup",
            serde_json::json!({ "token": token, "username": "pic" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["photo"], "imported");
    let t = common::tokens_from(&body);
    assert_eq!(app.photo_count(t.user_id).await, 1);
    let file: String = {
        use sea_orm::{ConnectionTrait, Statement};
        let row = app
            .db
            .query_one(Statement::from_string(
                sea_orm::DbBackend::Postgres,
                format!(
                    "SELECT file_name FROM photo WHERE user_id = '{}'",
                    t.user_id
                ),
            ))
            .await
            .expect("query")
            .expect("row");
        row.try_get_by_index(0).expect("file_name")
    };
    let stored = std::fs::read(app.photo_path(&file)).expect("photo file");
    assert_eq!(
        image::guess_format(&stored).expect("format"),
        image::ImageFormat::WebP,
        "went through the photo pipeline"
    );
}

#[tokio::test]
async fn login_to_an_existing_account_never_imports() {
    let (app, fake) = TestApp::with_facebook().await;
    let (t, _) = app.fb_signup(&fake, "50002", "known", false).await;
    let fragment = start_importing(&app, &fake, "50002").await;
    assert!(fragment.contains_key("code"));
    assert!(!fragment.contains_key("photo"), "{fragment:?}");
    assert_eq!(fake.picture_hits.load(Ordering::SeqCst), 0);
    assert_eq!(app.photo_count(t.user_id).await, 0);
}

#[tokio::test]
async fn a_signup_without_a_picture_answers_without_photo() {
    let (app, fake) = TestApp::with_facebook().await;
    fake.set_picture(Picture::Silhouette);
    let started = app.fb_start(true).await;
    let fragment = app.fb_return(&fake, &started, "50009").await.fragment();
    assert_eq!(fragment["photo"], "none");
    let resp = app
        .oauth_exchange(Some(&started.cookie), &fragment["code"])
        .await;
    let token = resp.body["signup"]["token"].as_str().expect("token");
    let (status, body) = app
        .post(
            "/api/auth/oauth/signup",
            serde_json::json!({ "token": token, "username": "nopic" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(body.get("photo").is_none(), "{body}");
    assert!(body["access_token"].is_string());
}

#[tokio::test]
async fn link_imports_into_the_linking_account() {
    let (app, fake) = TestApp::with_facebook().await;
    let alice = app.register("alice").await;
    let started = app.fb_link(&alice.access_token, true).await;
    let fragment = app.fb_return(&fake, &started, "50003").await.fragment();
    assert_eq!(fragment["linked"], "facebook");
    assert_eq!(fragment["photo"], "imported");
    let (_, me) = app.get("/api/me", Some(&alice.access_token)).await;
    assert_eq!(me["user"]["username"], "alice");
    assert_eq!(app.photo_count(alice.user_id).await, 1);
}

#[tokio::test]
async fn a_full_account_gets_no_seventh_photo() {
    let (app, fake) = TestApp::with_facebook().await;
    let t = app.register("full").await;
    for _ in 0..6 {
        app.upload_photo(&t, 32, 32).await;
    }
    let started = app.fb_link(&t.access_token, true).await;
    let fragment = app.fb_return(&fake, &started, "50004").await.fragment();
    assert_eq!(fragment["photo"], "full");
    assert_eq!(
        fragment["linked"], "facebook",
        "the link itself still works"
    );
    assert_eq!(app.photo_count(t.user_id).await, 6);
}

#[tokio::test]
async fn refused_links_download_nothing() {
    let (app, fake) = TestApp::with_facebook().await;
    let alice = app.register("alice").await;
    let started = app.fb_link(&alice.access_token, false).await;
    app.fb_return(&fake, &started, "50010").await;
    let bob = app.register("bob").await;
    let started = app.fb_link(&bob.access_token, true).await;
    let fragment = app.fb_return(&fake, &started, "50010").await.fragment();
    assert_eq!(fragment["error"], "identity_taken");
    assert_eq!(fake.picture_hits.load(Ordering::SeqCst), 0);
    assert_eq!(fake.cdn_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_default_silhouette_is_not_imported() {
    let (app, fake) = TestApp::with_facebook().await;
    fake.set_picture(Picture::Silhouette);
    let started = app.fb_start(true).await;
    let fragment = app.fb_return(&fake, &started, "50005").await.fragment();
    assert_eq!(fragment["photo"], "none");
    assert_eq!(fake.cdn_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_host_off_the_allowlist_is_never_contacted() {
    let (app, fake) = TestApp::with_facebook().await;
    // The fake CDN itself, but under a name that is not allowlisted.
    fake.set_picture(Picture::Url(fake.localhost_url("/cdn/pic")));
    let fragment = start_importing(&app, &fake, "50006").await;
    assert_eq!(fragment["photo"], "failed");
    assert!(
        fragment.contains_key("code"),
        "a failed import never fails the sign-up"
    );
    assert_eq!(fake.cdn_hits.load(Ordering::SeqCst), 0);

    for url in [
        "http://169.254.169.254/latest/meta-data/".to_owned(),
        "file:///etc/passwd".to_owned(),
        format!("{}/cdn/pic", fake.base).replace("http://", "ftp://"),
    ] {
        fake.set_picture(Picture::Url(url.clone()));
        let fragment = start_importing(&app, &fake, "50006").await;
        assert_eq!(fragment["photo"], "failed", "{url}");
    }
    assert_eq!(fake.cdn_hits.load(Ordering::SeqCst), 0);
    assert_eq!(common::count(&app, "SELECT count(*) FROM photo").await, 0);
}

#[tokio::test]
async fn cdn_redirects_are_not_followed() {
    let (app, fake) = TestApp::with_facebook().await;
    fake.set_picture(Picture::Url(format!("{}/cdn/redirect", fake.base)));
    let fragment = start_importing(&app, &fake, "50007").await;
    assert_eq!(fragment["photo"], "failed");
    assert_eq!(fake.cdn_hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn oversized_and_undecodable_pictures_are_refused() {
    let (app, fake) = TestApp::with_facebook().await;
    // A valid PNG (decoders ignore bytes after IEND), so only the size cap can refuse it.
    let mut big = common::photos::png_bytes(16, 16);
    big.resize(10 * 1024 * 1024 + 1, 0);
    fake.set_image(big);
    let fragment = start_importing(&app, &fake, "50008").await;
    assert_eq!(fragment["photo"], "failed");

    fake.set_image(b"<html>not an image</html>".to_vec());
    let fragment = start_importing(&app, &fake, "50008").await;
    assert_eq!(fragment["photo"], "failed");
    assert_eq!(
        common::count(
            &app,
            "SELECT count(*) FROM oauth_grant WHERE photo IS NOT NULL"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn providers_without_a_picture_ignore_the_import_flag() {
    let (app, fake) = TestApp::with_google().await;
    let resp = app
        .raw(
            Method::GET,
            "/api/auth/oauth/google/start?import_photo=1",
            None,
            None,
            None,
        )
        .await;
    let started = common::oauth::started(&resp, resp.location.as_deref().expect("url"));
    let resp = app
        .oauth_return(&fake, &started, FakeProvider::claims(&started, "g-1"))
        .await;
    let fragment = resp.fragment();
    assert!(fragment.contains_key("code"));
    assert!(!fragment.contains_key("photo"));
}
