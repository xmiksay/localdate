mod common;

use axum::http::StatusCode;
use common::{TestApp, png_bytes};
use serde_json::{Value, json};

fn file_name(photo: &Value) -> String {
    photo["url"]
        .as_str()
        .and_then(|u| u.strip_prefix("/media/"))
        .expect("media url")
        .to_owned()
}

#[tokio::test]
async fn upload_converts_to_downscaled_webp_on_disk() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let photo = app.upload_photo(&t, 2000, 1000).await;
    assert_eq!(photo["position"], 0);

    let name = file_name(&photo);
    assert!(name.ends_with(".webp"));
    let bytes = std::fs::read(app.photo_path(&name)).expect("file on disk");
    assert_eq!(
        image::guess_format(&bytes).expect("format"),
        image::ImageFormat::WebP
    );
    let img = image::load_from_memory(&bytes).expect("decodes");
    assert_eq!((img.width(), img.height()), (1280, 640));
    assert!(!app.photo_path(&format!("{name}.tmp")).exists());

    // Served through /media too.
    let resp = app.get(&format!("/media/{name}"), None).await;
    assert_eq!(resp.0, StatusCode::OK);
}

#[tokio::test]
async fn small_images_are_not_upscaled() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let photo = app.upload_photo(&t, 40, 30).await;
    let bytes = std::fs::read(app.photo_path(&file_name(&photo))).expect("file");
    let img = image::load_from_memory(&bytes).expect("decodes");
    assert_eq!((img.width(), img.height()), (40, 30));
}

#[tokio::test]
async fn garbage_and_missing_field_are_rejected() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let (status, body) = app
        .post_multipart("/api/me/photos", &t.access_token, "file", b"not an image")
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "unsupported_image");

    let (status, body) = app
        .post_multipart("/api/me/photos", &t.access_token, "other", &png_bytes(4, 4))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");
    assert_eq!(
        std::fs::read_dir(&app.state.config.photo_dir)
            .expect("dir")
            .count(),
        0
    );
}

#[tokio::test]
async fn oversized_upload_is_unsupported_image() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let big = vec![0u8; 10 * 1024 * 1024 + 1];
    let (status, body) = app
        .post_multipart("/api/me/photos", &t.access_token, "file", &big)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "unsupported_image");
}

#[tokio::test]
async fn oversize_dimensions_are_unsupported_image() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    // Tiny file, but one edge over image_proc::MAX_INPUT_EDGE (10 000).
    let (status, body) = app
        .post_multipart(
            "/api/me/photos",
            &t.access_token,
            "file",
            &png_bytes(10_001, 1),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "unsupported_image");
    assert_eq!(
        std::fs::read_dir(&app.state.config.photo_dir)
            .expect("dir")
            .count(),
        0
    );
}

#[tokio::test]
async fn seventh_photo_is_photo_limit() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    for _ in 0..6 {
        app.upload_photo(&t, 8, 8).await;
    }
    let (status, body) = app
        .post_multipart("/api/me/photos", &t.access_token, "file", &png_bytes(8, 8))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "photo_limit");
    assert_eq!(
        std::fs::read_dir(&app.state.config.photo_dir)
            .expect("dir")
            .count(),
        6
    );
}

#[tokio::test]
async fn delete_compacts_positions_and_removes_file() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let a = app.upload_photo(&t, 8, 8).await;
    let b = app.upload_photo(&t, 8, 8).await;
    let c = app.upload_photo(&t, 8, 8).await;

    let (status, _) = app
        .delete(
            &format!("/api/me/photos/{}", a["id"].as_str().expect("id")),
            &t.access_token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!app.photo_path(&file_name(&a)).exists());
    assert!(app.photo_path(&file_name(&b)).exists());

    // Order endpoint returns the compacted list.
    let (status, list) = app
        .put(
            "/api/me/photos/order",
            &t.access_token,
            json!({ "photo_ids": [b["id"], c["id"]] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["id"], b["id"]);
    assert_eq!(list[0]["position"], 0);
    assert_eq!(list[1]["id"], c["id"]);
    assert_eq!(list[1]["position"], 1);
}

#[tokio::test]
async fn delete_of_foreign_or_unknown_photo_is_404() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let bob = app.register("bob").await;
    let photo = app.upload_photo(&eva, 8, 8).await;
    let id = photo["id"].as_str().expect("id");

    let (status, body) = app
        .delete(&format!("/api/me/photos/{id}"), &bob.access_token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert!(app.photo_path(&file_name(&photo)).exists());

    for path in [
        "/api/me/photos/not-a-uuid",
        &format!("/api/me/photos/{}", uuid::Uuid::new_v4()),
    ] {
        let (status, _) = app.delete(path, &eva.access_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn reorder_rewrites_positions_and_validates_set() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    let a = app.upload_photo(&t, 8, 8).await;
    let b = app.upload_photo(&t, 8, 8).await;
    let c = app.upload_photo(&t, 8, 8).await;

    let (status, list) = app
        .put(
            "/api/me/photos/order",
            &t.access_token,
            json!({ "photo_ids": [c["id"], a["id"], b["id"]] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&Value> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|p| &p["id"])
        .collect();
    assert_eq!(ids, [&c["id"], &a["id"], &b["id"]]);

    let stranger = json!(uuid::Uuid::new_v4());
    for ids in [
        json!([a["id"], b["id"]]),
        json!([a["id"], a["id"], b["id"]]),
        json!([a["id"], b["id"], stranger]),
        json!([a["id"], b["id"], c["id"], c["id"]]),
    ] {
        let (status, body) = app
            .put(
                "/api/me/photos/order",
                &t.access_token,
                json!({ "photo_ids": ids }),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{ids}");
        assert_eq!(body["error"]["code"], "validation");
    }
}

#[tokio::test]
async fn profile_lists_photos_in_position_order() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1990-01-01").await;
    let extra = app.upload_photo(&t, 8, 8).await;
    let (_, me) = app.get("/api/me", Some(&t.access_token)).await;
    let photos = me["profile"]["photos"].as_array().expect("photos");
    assert_eq!(photos.len(), 2);
    assert_eq!(photos[1]["id"], extra["id"]);
    assert_eq!(photos[1]["position"], 1);
}
