//! `/admin/areas` CRUD and `GET /areas` containment.

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, count};
use serde_json::{Value, json};

const LAT: f64 = 50.0830;
const LON: f64 = 14.4350;
/// 0.0027 degrees of latitude is about 300 m.
const D300: f64 = 0.0027;

fn area_body(name: &str, lat: f64, radius_m: i32) -> Value {
    json!({ "name": name, "kind": "train_station", "lat": lat, "lon": LON, "radius_m": radius_m })
}

async fn create(app: &TestApp, token: &str, body: Value) -> Value {
    let (status, created) = app.post_as("/api/admin/areas", token, body).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    created
}

async fn names_at(app: &TestApp, token: &str, lat: f64) -> Vec<String> {
    let (status, body) = app
        .get(&format!("/api/areas?lat={lat}&lon={LON}"), Some(token))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array()
        .expect("array")
        .iter()
        .map(|a| a["name"].as_str().expect("name").to_owned())
        .collect()
}

#[tokio::test]
async fn admin_area_routes_need_an_admin() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let id = app.area("Nádraží", LAT, LON, 300).await;
    let paths = [
        (Method::GET, "/api/admin/areas".to_owned()),
        (Method::POST, "/api/admin/areas".to_owned()),
        (Method::PUT, format!("/api/admin/areas/{id}")),
        (Method::DELETE, format!("/api/admin/areas/{id}")),
    ];
    for (method, path) in paths {
        let body = Some(area_body("x", LAT, 300));
        let (status, err) = app
            .request(method.clone(), &path, Some(&eva.access_token), body.clone())
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
        assert_eq!(err["error"]["code"], "forbidden");
        let (status, _) = app.request(method, &path, None, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(count(&app, "SELECT count(*) FROM area").await, 1);
}

#[tokio::test]
async fn admin_creates_lists_updates_and_deletes() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let tok = &admin.access_token;

    let created = create(&app, tok, area_body("  Hlavní nádraží ", 50.083_456, 300)).await;
    assert_eq!(created["name"], "Hlavní nádraží");
    assert_eq!(created["kind"], "train_station");
    assert_eq!(created["lat"], 50.083_456, "centres are not rounded");
    assert_eq!(created["radius_m"], 300);
    assert_eq!(created["active"], true);
    assert!(created["created_at"].is_string());
    let id = created["id"].as_str().expect("id").to_owned();

    let mut hidden = area_body("Arena", LAT, 500);
    hidden["active"] = json!(false);
    hidden["kind"] = json!("venue");
    create(&app, tok, hidden).await;
    create(&app, tok, area_body("Centrum", LAT, 1000)).await;

    let (status, list) = app.get("/api/admin/areas", Some(tok)).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|a| a["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["Centrum", "Hlavní nádraží", "Arena"]);

    let path = format!("/api/admin/areas/{id}");
    let mut edit = area_body("Nádraží", LAT, 400);
    edit["active"] = json!(false);
    edit["kind"] = json!("city_centre");
    let (status, updated) = app.put(&path, tok, edit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["name"], "Nádraží");
    assert_eq!(updated["kind"], "city_centre");
    assert_eq!(updated["radius_m"], 400);
    assert_eq!(updated["active"], false);
    assert_eq!(updated["created_at"], created["created_at"]);

    let (status, _) = app.delete(&path, tok).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.delete(&path, tok).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.put(&path, tok, edit).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.delete("/api/admin/areas/not-a-uuid", tok).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_input_is_validated() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let tok = &admin.access_token;
    let id = app.area("Nádraží", LAT, LON, 300).await;
    let bad = [
        area_body("   ", LAT, 300),
        area_body(&"x".repeat(81), LAT, 300),
        area_body("a", LAT, 49),
        area_body("a", LAT, 5001),
        area_body("a", 91.0, 300),
        json!({ "name": "a", "kind": "castle", "lat": LAT, "lon": LON, "radius_m": 300 }),
        json!({ "name": "a", "kind": "venue", "lat": LAT, "radius_m": 300 }),
    ];
    for body in bad {
        let (status, err) = app.post_as("/api/admin/areas", tok, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(err["error"]["code"], "validation");
    }
    // PUT replaces the whole area, so `active` is required there.
    let (status, _) = app
        .put(
            &format!("/api/admin/areas/{id}"),
            tok,
            area_body("a", LAT, 300),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(count(&app, "SELECT count(*) FROM area").await, 1);
}

#[tokio::test]
async fn areas_lists_active_areas_containing_the_point_nearest_first() {
    let app = TestApp::new().await;
    let eva = app.register("eva").await;
    let tok = &eva.access_token;
    app.area("Centrum", LAT, LON, 2000).await;
    app.area("Nádraží", LAT + D300, LON, 300).await;
    let closed = app.area("Arena", LAT + D300, LON, 500).await;
    app.sql(&format!(
        "UPDATE area SET active = false WHERE id = '{closed}'"
    ))
    .await;

    // On the station centre: both contain it, the station centre is nearer.
    assert_eq!(
        names_at(&app, tok, LAT + D300).await,
        ["Nádraží", "Centrum"]
    );
    assert_eq!(names_at(&app, tok, LAT).await, ["Centrum"]);
    assert!(names_at(&app, tok, LAT + 0.1).await.is_empty());

    let (_, list) = app
        .get(&format!("/api/areas?lat={LAT}&lon={LON}"), Some(tok))
        .await;
    let item = &list[0];
    for key in [
        "id",
        "name",
        "kind",
        "lat",
        "lon",
        "radius_m",
        "active",
        "created_at",
    ] {
        assert!(!item[key].is_null(), "{key}");
    }

    for query in [
        "",
        "?lat=50",
        "?lat=abc&lon=14",
        "?lat=91&lon=14",
        "?lat=NaN&lon=14",
    ] {
        let (status, err) = app.get(&format!("/api/areas{query}"), Some(tok)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(err["error"]["code"], "validation");
    }
    let (status, _) = app.get("/api/areas?lat=50&lon=14", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_area_with_windows_can_only_be_deactivated() {
    let app = TestApp::new().await;
    let admin = app.admin("mod").await;
    let area = app.area("Nádraží", LAT, LON, 300).await;
    let anna = app.area_user("anna", area, LAT, LON).await;
    let path = format!("/api/admin/areas/{area}");

    let (status, err) = app.delete(&path, &admin.access_token).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "area_in_use");

    // An ended window still references the area until the cleanup job purges the row.
    app.delete("/api/me/window", &anna.access_token).await;
    let (status, _) = app.delete(&path, &admin.access_token).await;
    assert_eq!(status, StatusCode::CONFLICT);
    app.sql("DELETE FROM visibility_window").await;
    let (status, _) = app.delete(&path, &admin.access_token).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
