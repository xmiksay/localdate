//! Shared integration-test harness: a fresh Postgres database per test plus the real router.
#![allow(dead_code)]

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use localdate_api::config::Config;
use localdate_api::state::AppState;
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

pub mod areas;

pub const PASSWORD: &str = "correct horse battery";

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    pub db: DatabaseConnection,
    admin_url: String,
    db_name: String,
    _photo_dir: tempfile::TempDir,
}

pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: Uuid,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::try_new(localdate_api::app)
            .await
            .expect("test app setup")
    }

    /// Like `new`, but serving the embedded fixture bundle `F` instead of `frontend/dist`.
    pub async fn with_frontend<F: rust_embed::RustEmbed + 'static>() -> Self {
        Self::try_new(localdate_api::app_with_frontend::<F>)
            .await
            .expect("test app setup")
    }

    async fn try_new(build: fn(AppState) -> Router) -> Result<Self> {
        // Walks up from the crate dir to the workspace-root .env; real env vars win.
        dotenvy::dotenv().ok();
        let admin_url = std::env::var("TEST_DATABASE_URL")
            .context("TEST_DATABASE_URL must point at the Postgres server")?;
        let db_name = format!("localdate_test_{}", Uuid::new_v4().simple());

        let admin = Database::connect(&admin_url)
            .await
            .context("admin connect")?;
        admin
            .execute_unprepared(&format!("CREATE DATABASE {db_name}"))
            .await
            .context("create test database")?;
        let _ = admin.close().await;

        let db = Database::connect(database_url(&admin_url, &db_name))
            .await
            .context("connect test database")?;
        Migrator::up(&db, None).await.context("migrate")?;

        let photo_dir = tempfile::tempdir()?;
        let config = Config {
            database_url: String::new(),
            jwt_secret: "test-secret-test-secret-test-secret-1".into(),
            photo_dir: photo_dir.path().to_path_buf(),
            bind_addr: "127.0.0.1:0".parse()?,
            rate_limit: false,
            cleanup_interval: std::time::Duration::from_secs(300),
        };
        let state = AppState::new(db.clone(), config);
        let router = build(state.clone());
        Ok(Self {
            router,
            state,
            db,
            admin_url,
            db_name,
            _photo_dir: photo_dir,
        })
    }

    /// Sends a JSON request; returns status and parsed body (`Null` when empty).
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(t) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => builder.body(Body::empty()),
        }
        .expect("build request");
        let resp = self.router.clone().oneshot(req).await.expect("infallible");
        let status = resp.status();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::POST, path, None, Some(body)).await
    }

    pub async fn get(&self, path: &str, token: Option<&str>) -> (StatusCode, Value) {
        self.request(Method::GET, path, token, None).await
    }

    pub async fn put(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::PUT, path, Some(token), Some(body))
            .await
    }

    pub async fn delete(&self, path: &str, token: &str) -> (StatusCode, Value) {
        self.request(Method::DELETE, path, Some(token), None).await
    }

    pub async fn patch(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::PATCH, path, Some(token), Some(body))
            .await
    }

    /// Runs raw SQL against the test database (to seed or backdate rows).
    pub async fn sql(&self, query: &str) {
        self.db
            .execute_unprepared(query)
            .await
            .unwrap_or_else(|e| panic!("sql failed: {e}\n{query}"));
    }

    /// Opens a 60 min window at the given spot; returns the window JSON.
    pub async fn open_window(&self, t: &Tokens, lat: f64, lon: f64) -> Value {
        let (status, body) = self
            .post_as(
                "/api/me/window",
                &t.access_token,
                json!({ "minutes": 60, "lat": lat, "lon": lon }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "window start failed: {body}");
        body
    }

    /// Registers, onboards (default filter, one photo) and opens a window.
    pub async fn visible_user(&self, name: &str, lat: f64, lon: f64) -> Tokens {
        let t = self.register(name).await;
        self.onboard(&t, "female", "1995-05-05").await;
        self.open_window(&t, lat, lon).await;
        t
    }

    /// Authenticated JSON POST.
    pub async fn post_as(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::POST, path, Some(token), Some(body))
            .await
    }

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

    /// Gives the user a profile, the default filter and one photo, i.e. makes them visible-capable.
    pub async fn onboard(&self, t: &Tokens, gender: &str, birth_date: &str) {
        let (status, body) = self
            .put(
                "/api/me/profile",
                &t.access_token,
                json!({
                    "display_name": "Tester",
                    "birth_date": birth_date,
                    "gender": gender,
                    "bio": "",
                    "interest_ids": [],
                }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "profile failed: {body}");
        let (status, body) = self
            .put(
                "/api/me/filter",
                &t.access_token,
                json!({
                    "max_distance_m": 2000, "genders": [], "age_min": 18, "age_max": 99,
                    "reasons": ["date", "meet"], "default_window_minutes": 60,
                }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "filter failed: {body}");
        self.upload_photo(t, 8, 8).await;
    }

    pub fn photo_path(&self, file_name: &str) -> std::path::PathBuf {
        self.state.config.photo_dir.join(file_name)
    }

    /// Two currently visible users wave at each other; returns the match id.
    pub async fn match_up(&self, a: &Tokens, b: &Tokens) -> String {
        self.post_as(
            "/api/waves",
            &a.access_token,
            json!({ "to_user_id": b.user_id }),
        )
        .await;
        let (_, res) = self
            .post_as(
                "/api/waves",
                &b.access_token,
                json!({ "to_user_id": a.user_id }),
            )
            .await;
        res["match_id"].as_str().expect("match id").to_owned()
    }

    /// Registers `username` and grants the admin role the way the CLI does.
    pub async fn admin(&self, username: &str) -> Tokens {
        let t = self.register(username).await;
        localdate_api::admin::set_admin(&self.db, username, true)
            .await
            .expect("grant admin");
        t
    }

    pub async fn register(&self, username: &str) -> Tokens {
        let (status, body) = self
            .post(
                "/api/auth/register",
                json!({ "username": username, "password": PASSWORD }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
        tokens_from(&body)
    }
}

/// Birth year that makes someone exactly `age` today (Jan 1 birthdays have always passed).
pub fn now_year_birth(age: i32) -> i32 {
    use chrono::Datelike;
    chrono::Utc::now().year() - age
}

/// First column of the first row of a `SELECT count(*)`.
pub async fn count(app: &TestApp, query: &str) -> i64 {
    use sea_orm::{ConnectionTrait, Statement};
    let row = app
        .db
        .query_one(Statement::from_string(
            sea_orm::DbBackend::Postgres,
            query.to_owned(),
        ))
        .await
        .expect("count query")
        .expect("one row");
    row.try_get_by_index(0).expect("count column")
}

pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(width, height)
        .write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode png");
    buf.into_inner()
}

pub fn tokens_from(body: &Value) -> Tokens {
    Tokens {
        access_token: body["access_token"].as_str().expect("access_token").into(),
        refresh_token: body["refresh_token"]
            .as_str()
            .expect("refresh_token")
            .into(),
        user_id: body["user"]["id"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .expect("user.id"),
    }
}

fn database_url(admin_url: &str, db_name: &str) -> String {
    let (base, query) = admin_url.split_once('?').unwrap_or((admin_url, ""));
    let root = base.rsplit_once('/').map_or(base, |(root, _)| root);
    if query.is_empty() {
        format!("{root}/{db_name}")
    } else {
        format!("{root}/{db_name}?{query}")
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        // Best effort: a separate thread + runtime because Drop is sync and may run inside one.
        let (admin_url, db_name) = (self.admin_url.clone(), self.db_name.clone());
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async {
                if let Ok(admin) = Database::connect(&admin_url).await {
                    let _ = admin
                        .execute_unprepared(&format!(
                            "DROP DATABASE IF EXISTS {db_name} WITH (FORCE)"
                        ))
                        .await;
                }
            });
        })
        .join();
    }
}
