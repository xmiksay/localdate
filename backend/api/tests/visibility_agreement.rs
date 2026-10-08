//! The SQL rule (`discovery/nearby.rs`) and `rules::mutually_visible` must agree on every pair,
//! timed and area windows mixed.

mod common;

use std::collections::BTreeSet;

use axum::http::StatusCode;
use common::{TestApp, Tokens, now_year_birth};
use entity::{Gender, Reason};
use localdate_api::discovery::geo::haversine_m;
use localdate_api::discovery::rules::{AREA_STALE_SECS, Side, mutually_visible};

const LAT: f64 = 50.0870;
/// Specs whose location gets older than the stale threshold: a timed one (no effect) and an area one.
const STALE: [usize; 2] = [0, 10];
const LON: f64 = 14.4210;

struct Spec {
    gender: &'static str,
    age: i32,
    max: i32,
    genders: &'static str,
    age_min: i32,
    age_max: i32,
    reasons: &'static str,
    lat: f64,
    /// Index into the test's areas; `None` = timed window.
    area: Option<usize>,
}

const fn spec(
    gender: &'static str,
    age: i32,
    max: i32,
    genders: &'static str,
    (age_min, age_max): (i32, i32),
    reasons: &'static str,
    (lat, area): (f64, Option<usize>),
) -> Spec {
    Spec {
        gender,
        age,
        max,
        genders,
        age_min,
        age_max,
        reasons,
        lat,
        area,
    }
}

#[tokio::test]
async fn sql_agrees_with_the_rust_rule() {
    let all = "{date,meet}";
    let specs = [
        spec("female", 25, 2000, "{}", (18, 99), all, (LAT, None)),
        spec(
            "male",
            30,
            500,
            "{female}",
            (20, 30),
            "{date}",
            (LAT + 0.002, None),
        ),
        spec(
            "other",
            41,
            5000,
            "{male,other}",
            (18, 45),
            "{meet}",
            (LAT + 0.004, None),
        ),
        spec("male", 19, 10000, "{}", (25, 50), all, (LAT + 0.012, None)),
        spec(
            "female",
            50,
            1000,
            "{male}",
            (18, 99),
            "{meet}",
            (LAT + 0.006, None),
        ),
        spec(
            "female",
            33,
            200,
            "{}",
            (30, 35),
            "{date}",
            (LAT + 0.0015, None),
        ),
        // Area windows next to the timed ones: kinds never mix, max distance is ignored inside.
        spec(
            "male",
            28,
            200,
            "{}",
            (18, 99),
            all,
            (LAT + 0.0001, Some(0)),
        ),
        spec(
            "female",
            27,
            200,
            "{}",
            (18, 99),
            all,
            (LAT + 0.008, Some(0)),
        ),
        spec(
            "female",
            45,
            2000,
            "{}",
            (18, 40),
            all,
            (LAT + 0.003, Some(0)),
        ),
        spec(
            "male",
            30,
            2000,
            "{}",
            (18, 99),
            all,
            (LAT + 0.0021, Some(1)),
        ),
        // Would see users 6 and 7, but its location is stale.
        spec(
            "female",
            29,
            2000,
            "{}",
            (18, 99),
            all,
            (LAT + 0.0002, Some(0)),
        ),
    ];
    let app = TestApp::new().await;
    let areas = [
        app.area("Centrum", LAT, LON, 1000).await,
        app.area("Nádraží", LAT + 0.002, LON, 1000).await,
    ];
    let mut users: Vec<Tokens> = vec![];
    for (i, s) in specs.iter().enumerate() {
        let name = format!("user{i}");
        let t = match s.area {
            None => app.visible_user(&name, s.lat, LON).await,
            Some(a) => app.area_user(&name, areas[a], s.lat, LON).await,
        };
        app.sql(&format!(
            "UPDATE profile SET gender = '{}', birth_date = '{}-01-01' WHERE user_id = '{}'",
            s.gender,
            now_year_birth(s.age),
            t.user_id
        ))
        .await;
        app.sql(&format!(
            "UPDATE filter SET max_distance_m = {}, genders = '{}', age_min = {}, age_max = {}, \
             reasons = '{}' WHERE user_id = '{}'",
            s.max, s.genders, s.age_min, s.age_max, s.reasons, t.user_id
        ))
        .await;
        if STALE.contains(&i) {
            app.sql(&format!(
                "UPDATE visibility_window SET location_updated_at = now() - interval '{} seconds' \
                 WHERE user_id = '{}'",
                AREA_STALE_SECS + 60,
                t.user_id
            ))
            .await;
        }
        users.push(t);
    }

    let side = |i: usize, s: &Spec| Side {
        gender: parse_gender(s.gender),
        age: s.age,
        max_distance_m: s.max,
        genders: parse_list(s.genders, parse_gender),
        age_min: s.age_min,
        age_max: s.age_max,
        reasons: parse_list(s.reasons, parse_reason),
        area: s.area.map(|a| areas[a]),
        stale: STALE.contains(&i),
    };
    let mut visible_pairs = 0;
    let mut area_pairs = 0;
    for (i, a) in specs.iter().enumerate() {
        let mut expected = BTreeSet::new();
        for (j, b) in specs.iter().enumerate() {
            let d = haversine_m(a.lat, LON, b.lat, LON);
            if i != j && mutually_visible(&side(i, a), &side(j, b), d) {
                expected.insert(users[j].user_id.to_string());
                area_pairs += usize::from(a.area.is_some());
            }
        }
        visible_pairs += expected.len();
        let (status, body) = app.get("/api/nearby", Some(&users[i].access_token)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let actual: BTreeSet<String> = body
            .as_array()
            .expect("array")
            .iter()
            .map(|p| p["user_id"].as_str().expect("id").to_owned())
            .collect();
        assert_eq!(actual, expected, "viewer {i}");
    }
    assert!(
        visible_pairs > area_pairs,
        "matrix should contain timed pairs"
    );
    assert!(area_pairs > 0, "matrix should contain area pairs");
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
