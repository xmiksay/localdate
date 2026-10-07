mod common;

use std::collections::BTreeSet;

use axum::http::StatusCode;
use common::{TestApp, Tokens, now_year_birth};
use entity::{Gender, Reason};
use localdate_api::discovery::geo::haversine_m;
use localdate_api::discovery::rules::{Side, mutually_visible};
use serde_json::{Value, json};

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;
/// 0.0027 degrees of latitude is about 300 m.
const D300: f64 = 0.0027;

async fn nearby(app: &TestApp, t: &Tokens) -> Vec<Value> {
    let (status, body) = app.get("/api/nearby", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array().expect("array").clone()
}

fn ids(list: &[Value]) -> BTreeSet<String> {
    list.iter()
        .map(|p| p["user_id"].as_str().expect("id").to_owned())
        .collect()
}

/// A pair 300 m apart with default filters.
async fn pair(app: &TestApp) -> (Tokens, Tokens) {
    let a = app.visible_user("anna", LAT, LON).await;
    let b = app.visible_user("bob", LAT + D300, LON).await;
    (a, b)
}

fn set_filter(user: &Tokens, assignments: &str) -> String {
    format!(
        "UPDATE filter SET {assignments} WHERE user_id = '{}'",
        user.user_id
    )
}

#[tokio::test]
async fn nearby_without_own_window_is_409() {
    let app = TestApp::new().await;
    let t = app.register("eva").await;
    app.onboard(&t, "female", "1995-05-05").await;
    let (status, err) = app.get("/api/nearby", Some(&t.access_token)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "no_active_window");
    let (status, _) = app.get("/api/nearby", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn two_users_see_each_other_with_band_and_no_private_fields() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    app.sql("INSERT INTO user_interest (user_id, interest_id) SELECT user_id, 3 FROM profile")
        .await;

    let seen_by_a = nearby(&app, &a).await;
    assert_eq!(ids(&seen_by_a), BTreeSet::from([b.user_id.to_string()]));
    let p = &seen_by_a[0];
    assert_eq!(p["distance_band"], "lt_500m");
    assert_eq!(p["wave_state"], "none");
    assert!(p["match_id"].is_null());
    assert_eq!(p["gender"], "female");
    assert_eq!(p["interests"][0]["key"], "cycling");
    assert_eq!(p["photos"].as_array().map(Vec::len), Some(1));
    assert!(
        p["photos"][0]["url"]
            .as_str()
            .is_some_and(|u| u.starts_with("/media/"))
    );

    let keys: BTreeSet<&str> = p
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    let expected = BTreeSet::from([
        "user_id",
        "display_name",
        "age",
        "gender",
        "bio",
        "interests",
        "photos",
        "reasons",
        "distance_band",
        "wave_state",
        "match_id",
    ]);
    assert_eq!(keys, expected);

    let seen_by_b = nearby(&app, &b).await;
    assert_eq!(ids(&seen_by_b), BTreeSet::from([a.user_id.to_string()]));
}

#[tokio::test]
async fn one_sided_gender_filter_hides_both() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    app.sql(&set_filter(&a, "genders = '{male}'")).await;
    assert!(nearby(&app, &a).await.is_empty());
    assert!(nearby(&app, &b).await.is_empty());
    app.sql(&set_filter(&a, "genders = '{female,other}'")).await;
    assert_eq!(nearby(&app, &b).await.len(), 1);
}

#[tokio::test]
async fn age_filter_applies_both_ways() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    // Both are ~31; a wants 18..25.
    app.sql(&set_filter(&a, "age_max = 25")).await;
    assert!(nearby(&app, &a).await.is_empty());
    assert!(nearby(&app, &b).await.is_empty());
    app.sql(&set_filter(&a, "age_max = 99")).await;
    app.sql(&set_filter(&b, "age_min = 40")).await;
    assert!(nearby(&app, &a).await.is_empty());
}

#[tokio::test]
async fn needs_one_shared_reason() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    app.sql(&set_filter(&a, "reasons = '{date}'")).await;
    app.sql(&set_filter(&b, "reasons = '{meet}'")).await;
    assert!(nearby(&app, &a).await.is_empty());
    app.sql(&set_filter(&b, "reasons = '{meet,date}'")).await;
    let seen = nearby(&app, &a).await;
    assert_eq!(seen[0]["reasons"], json!(["meet", "date"]));
}

#[tokio::test]
async fn smaller_max_distance_wins() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    app.sql(&set_filter(&a, "max_distance_m = 200")).await;
    assert!(nearby(&app, &a).await.is_empty());
    assert!(nearby(&app, &b).await.is_empty());
    app.sql(&set_filter(&a, "max_distance_m = 400")).await;
    assert_eq!(nearby(&app, &b).await.len(), 1);
}

#[tokio::test]
async fn far_away_users_are_hidden_even_in_the_bounding_box_of_others() {
    let app = TestApp::new().await;
    let a = app.visible_user("anna", LAT, LON).await;
    // ~3 km north: outside the default 2000 m.
    app.visible_user("far", LAT + 0.027, LON).await;
    // Same latitude but 90 degrees of longitude away.
    app.visible_user("other", LAT, LON + 90.0).await;
    assert!(nearby(&app, &a).await.is_empty());
}

#[tokio::test]
async fn blocked_expired_and_photoless_users_are_hidden() {
    let app = TestApp::new().await;
    let (a, b) = pair(&app).await;
    let c = app.visible_user("carl", LAT, LON + D300).await;
    assert_eq!(nearby(&app, &a).await.len(), 2);

    // Block in either direction hides, in both lists.
    app.post_as(
        "/api/blocks",
        &b.access_token,
        json!({ "user_id": a.user_id }),
    )
    .await;
    assert_eq!(
        ids(&nearby(&app, &a).await),
        BTreeSet::from([c.user_id.to_string()])
    );
    assert_eq!(
        ids(&nearby(&app, &b).await),
        BTreeSet::from([c.user_id.to_string()])
    );

    app.sql(&format!(
        "UPDATE visibility_window SET ends_at = now() - interval '1 second' WHERE user_id = '{}'",
        c.user_id
    ))
    .await;
    assert!(nearby(&app, &a).await.is_empty());

    app.sql(&format!(
        "DELETE FROM block WHERE blocker_id = '{}'",
        b.user_id
    ))
    .await;
    assert_eq!(nearby(&app, &a).await.len(), 1);
    app.sql(&format!(
        "DELETE FROM photo WHERE user_id = '{}'",
        b.user_id
    ))
    .await;
    assert!(nearby(&app, &a).await.is_empty());
}

#[tokio::test]
async fn sorted_by_band_then_newest_window() {
    let app = TestApp::new().await;
    let a = app.visible_user("anna", LAT, LON).await;
    let near = app.visible_user("near", LAT + 0.0009, LON).await; // ~100 m
    let mid_old = app.visible_user("midold", LAT + D300, LON).await; // ~300 m
    let mid_new = app.visible_user("midnew", LAT, LON).await; // moved below
    app.sql(&format!(
        "UPDATE visibility_window SET starts_at = now() - interval '10 minutes' WHERE user_id = '{}'",
        mid_old.user_id
    ))
    .await;
    // ~300 m south: same band as midold, newer window.
    app.sql(&format!(
        "UPDATE visibility_window SET lat = {}, lon = {LON} WHERE user_id = '{}'",
        LAT - D300,
        mid_new.user_id
    ))
    .await;

    let list = nearby(&app, &a).await;
    let order: Vec<&str> = list
        .iter()
        .map(|p| p["user_id"].as_str().expect("id"))
        .collect();
    assert_eq!(
        order,
        [
            near.user_id.to_string(),
            mid_new.user_id.to_string(),
            mid_old.user_id.to_string()
        ]
    );
}

/// The SQL rule and `rules::mutually_visible` must agree on every pair.
#[tokio::test]
async fn sql_agrees_with_the_rust_rule() {
    struct Spec {
        gender: &'static str,
        age: i32,
        max: i32,
        genders: &'static str,
        age_min: i32,
        age_max: i32,
        reasons: &'static str,
        lat: f64,
    }
    let specs = [
        Spec {
            gender: "female",
            age: 25,
            max: 2000,
            genders: "{}",
            age_min: 18,
            age_max: 99,
            reasons: "{date,meet}",
            lat: LAT,
        },
        Spec {
            gender: "male",
            age: 30,
            max: 500,
            genders: "{female}",
            age_min: 20,
            age_max: 30,
            reasons: "{date}",
            lat: LAT + 0.002,
        },
        Spec {
            gender: "other",
            age: 41,
            max: 5000,
            genders: "{male,other}",
            age_min: 18,
            age_max: 45,
            reasons: "{meet}",
            lat: LAT + 0.004,
        },
        Spec {
            gender: "male",
            age: 19,
            max: 10000,
            genders: "{}",
            age_min: 25,
            age_max: 50,
            reasons: "{date,meet}",
            lat: LAT + 0.012,
        },
        Spec {
            gender: "female",
            age: 50,
            max: 1000,
            genders: "{male}",
            age_min: 18,
            age_max: 99,
            reasons: "{meet}",
            lat: LAT + 0.006,
        },
        Spec {
            gender: "female",
            age: 33,
            max: 200,
            genders: "{}",
            age_min: 30,
            age_max: 35,
            reasons: "{date}",
            lat: LAT + 0.0015,
        },
    ];
    let app = TestApp::new().await;
    let mut users = vec![];
    for (i, s) in specs.iter().enumerate() {
        let t = app.visible_user(&format!("user{i}"), s.lat, LON).await;
        app.sql(&format!(
            "UPDATE profile SET gender = '{}', birth_date = '{}-01-01' WHERE user_id = '{}'",
            s.gender,
            now_year_birth(s.age),
            t.user_id
        ))
        .await;
        app.sql(&set_filter(
            &t,
            &format!(
                "max_distance_m = {}, genders = '{}', age_min = {}, age_max = {}, reasons = '{}'",
                s.max, s.genders, s.age_min, s.age_max, s.reasons
            ),
        ))
        .await;
        users.push(t);
    }

    let side = |s: &Spec| Side {
        gender: parse_gender(s.gender),
        age: s.age,
        max_distance_m: s.max,
        genders: parse_list(s.genders, parse_gender),
        age_min: s.age_min,
        age_max: s.age_max,
        reasons: parse_list(s.reasons, parse_reason),
    };
    let mut visible_pairs = 0;
    for (i, a) in specs.iter().enumerate() {
        let mut expected = BTreeSet::new();
        for (j, b) in specs.iter().enumerate() {
            let d = haversine_m(a.lat, LON, b.lat, LON);
            if i != j && mutually_visible(&side(a), &side(b), d) {
                expected.insert(users[j].user_id.to_string());
            }
        }
        visible_pairs += expected.len();
        let actual = ids(&nearby(&app, &users[i]).await);
        assert_eq!(actual, expected, "viewer {i}");
    }
    assert!(visible_pairs > 0, "matrix should contain visible pairs");
}

fn parse_gender(s: &str) -> Gender {
    match s {
        "male" => Gender::Male,
        "female" => Gender::Female,
        _ => Gender::Other,
    }
}

fn parse_reason(s: &str) -> Reason {
    if s == "date" {
        Reason::Date
    } else {
        Reason::Meet
    }
}

fn parse_list<T>(pg: &str, f: fn(&str) -> T) -> Vec<T> {
    pg.trim_matches(['{', '}'])
        .split(',')
        .filter(|s| !s.is_empty())
        .map(f)
        .collect()
}
