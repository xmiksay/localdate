//! An in-process fake S3 endpoint on 127.0.0.1 (path-style PUT/GET/HEAD object, and the bulk
//! `POST /{bucket}?delete` that `object_store` deletes with), so the real S3 client is exercised
//! without Garage or credentials.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, post};
use localdate_api::config::{PhotoStorage, S3Config};
use sha2::{Digest, Sha256};

use super::TestApp;

pub const BUCKET: &str = "localdate-test";

#[derive(Clone, Default)]
struct Shared {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    read_only: Arc<AtomicBool>,
    gets: Arc<AtomicUsize>,
}

pub struct FakeS3 {
    pub addr: SocketAddr,
    shared: Shared,
}

impl FakeS3 {
    pub async fn start() -> Self {
        let shared = Shared::default();
        let app = Router::new()
            .route("/{bucket}/{*key}", any(object))
            .route("/{bucket}", post(delete_objects))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake S3");
        let addr = listener.local_addr().expect("fake S3 addr");
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { addr, shared }
    }

    pub fn config(&self) -> PhotoStorage {
        s3_config(format!("http://{}", self.addr), BUCKET)
    }

    /// Acts like a read-only key while set: PUT and DELETE answer 403 AccessDenied.
    pub fn read_only(&self, on: bool) {
        self.shared.read_only.store(on, Ordering::SeqCst);
    }

    /// Object GET requests so far (HEAD not counted).
    pub fn gets(&self) -> usize {
        self.shared.gets.load(Ordering::SeqCst)
    }

    /// Settings for this endpoint but another bucket, which does not exist here.
    pub fn config_for_bucket(&self, bucket: &str) -> PhotoStorage {
        s3_config(format!("http://{}", self.addr), bucket)
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.objects().keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn object(&self, key: &str) -> Option<Vec<u8>> {
        self.objects().get(key).cloned()
    }

    fn objects(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<u8>>> {
        self.shared.objects.lock().expect("fake S3 lock")
    }
}

/// S3 settings for an endpoint nobody listens on.
pub fn dead_s3() -> PhotoStorage {
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("free port");
    s3_config(format!("http://{addr}"), BUCKET)
}

fn s3_config(endpoint: String, bucket: &str) -> PhotoStorage {
    PhotoStorage::S3(S3Config {
        endpoint,
        region: "garage".into(),
        bucket: bucket.into(),
        access_key_id: "GKtest".into(),
        secret_access_key: "test-secret".into(),
    })
}

impl TestApp {
    /// A test app storing photos in a fresh fake S3 bucket.
    pub async fn with_s3() -> (Self, FakeS3) {
        let fake = FakeS3::start().await;
        let storage = fake.config();
        let app = Self::with_config(|c| c.photo_storage = storage).await;
        (app, fake)
    }
}

fn s3_error(status: StatusCode, code: &str) -> Response {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Error><Code>{code}</Code><Message>{code}</Message></Error>"
    );
    (status, [(header::CONTENT_TYPE, "application/xml")], xml).into_response()
}

async fn object(
    State(shared): State<Shared>,
    method: Method,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(refused) = refuse(&bucket, &headers) {
        return refused;
    }
    let failing = shared.read_only.load(Ordering::SeqCst);
    if method == Method::GET {
        shared.gets.fetch_add(1, Ordering::SeqCst);
    }
    let mut objects = shared.objects.lock().expect("fake S3 lock");
    match method {
        Method::PUT if failing => s3_error(StatusCode::FORBIDDEN, "AccessDenied"),
        Method::PUT => {
            let etag = etag(&body);
            objects.insert(key, body.to_vec());
            (StatusCode::OK, [(header::ETAG, etag)]).into_response()
        }
        Method::GET | Method::HEAD => match objects.get(&key) {
            // Hyper drops the body of a HEAD answer but keeps its Content-Length.
            Some(data) => (
                StatusCode::OK,
                [
                    (header::ETAG, etag(data)),
                    (
                        header::LAST_MODIFIED,
                        "Thu, 01 Jan 2026 00:00:00 GMT".to_owned(),
                    ),
                    (header::CONTENT_TYPE, "binary/octet-stream".to_owned()),
                ],
                data.clone(),
            )
                .into_response(),
            None => s3_error(StatusCode::NOT_FOUND, "NoSuchKey"),
        },
        _ => s3_error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed"),
    }
}

/// Path-style addressing and a SigV4 signature are what the real client must send.
fn refuse(bucket: &str, headers: &HeaderMap) -> Option<Response> {
    if bucket != BUCKET {
        return Some(s3_error(StatusCode::NOT_FOUND, "NoSuchBucket"));
    }
    let signed = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("AWS4-HMAC-SHA256 Credential=GKtest/"));
    (!signed).then(|| s3_error(StatusCode::FORBIDDEN, "AccessDenied"))
}

/// DeleteObjects: `<Delete><Object><Key>k</Key></Object>…</Delete>`; missing keys count as deleted.
async fn delete_objects(
    State(shared): State<Shared>,
    Path(bucket): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Some(refused) = refuse(&bucket, &headers) {
        return refused;
    }
    if shared.read_only.load(Ordering::SeqCst) {
        return s3_error(StatusCode::FORBIDDEN, "AccessDenied");
    }
    let mut objects = shared.objects.lock().expect("fake S3 lock");
    let mut deleted = String::new();
    for part in body.split("<Key>").skip(1) {
        let key = part.split("</Key>").next().unwrap_or_default();
        objects.remove(key);
        deleted.push_str(&format!("<Deleted><Key>{key}</Key></Deleted>"));
    }
    let xml =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><DeleteResult>{deleted}</DeleteResult>");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    )
        .into_response()
}

fn etag(data: &[u8]) -> String {
    let hex: String = Sha256::digest(data)
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("\"{hex}\"")
}
