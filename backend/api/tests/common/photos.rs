//! Photo fixtures: multipart uploads and generated PNGs.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use super::{TestApp, Tokens};

impl TestApp {
    /// POSTs `bytes` as multipart field `field` (filename `upload.bin`).
    pub async fn post_multipart(
        &self,
        path: &str,
        token: &str,
        field: &str,
        bytes: &[u8],
    ) -> (StatusCode, Value) {
        let boundary = "localdate-test-boundary";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"upload.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .expect("build request");
        let resp = self.router.clone().oneshot(req).await.expect("infallible");
        let status = resp.status();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Uploads a generated PNG; returns the created photo.
    pub async fn upload_photo(&self, t: &Tokens, width: u32, height: u32) -> Value {
        let (status, body) = self
            .post_multipart(
                "/api/me/photos",
                &t.access_token,
                "file",
                &png_bytes(width, height),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "photo upload failed: {body}");
        body
    }

    pub fn photo_path(&self, file_name: &str) -> std::path::PathBuf {
        self.state.config.photo_dir.join(file_name)
    }
}

pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(width, height)
        .write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode png");
    buf.into_inner()
}
