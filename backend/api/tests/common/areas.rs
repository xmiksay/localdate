//! Area fixtures: seed areas and open area windows.

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{TestApp, Tokens};

impl TestApp {
    /// Inserts an active area directly (admin authz is tested separately).
    pub async fn area(&self, name: &str, lat: f64, lon: f64, radius_m: i32) -> Uuid {
        let id = Uuid::new_v4();
        self.sql(&format!(
            "INSERT INTO area (id, name, kind, lat, lon, radius_m) \
             VALUES ('{id}', '{name}', 'train_station', {lat}, {lon}, {radius_m})"
        ))
        .await;
        id
    }

    /// `POST /me/window` with `kind: 'area'`; returns status and body.
    pub async fn start_area_window(
        &self,
        t: &Tokens,
        area: Uuid,
        lat: f64,
        lon: f64,
    ) -> (StatusCode, Value) {
        self.post_as(
            "/api/me/window",
            &t.access_token,
            json!({ "kind": "area", "area_id": area, "minutes": 60, "lat": lat, "lon": lon }),
        )
        .await
    }

    /// Registers, onboards and opens an area window, which must succeed.
    pub async fn area_user(&self, name: &str, area: Uuid, lat: f64, lon: f64) -> Tokens {
        let t = self.register(name).await;
        self.onboard(&t, "female", "1995-05-05").await;
        let (status, body) = self.start_area_window(&t, area, lat, lon).await;
        assert_eq!(status, StatusCode::CREATED, "area window failed: {body}");
        t
    }
}
