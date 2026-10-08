mod common;

use axum::http::StatusCode;
use common::{TestApp, Tokens};
use serde_json::{Value, json};

const LAT: f64 = 50.0870;
const LON: f64 = 14.4210;

/// Two visible users who waved at each other; returns the match id.
async fn matched(app: &TestApp, a_name: &str, b_name: &str) -> (Tokens, Tokens, String) {
    let a = app.visible_user(a_name, LAT, LON).await;
    let b = app.visible_user(b_name, LAT + 0.0027, LON).await;
    let id = app.match_up(&a, &b).await;
    (a, b, id)
}

async fn say(app: &TestApp, t: &Tokens, id: &str, body: &str) -> (StatusCode, Value) {
    app.post_as(
        &format!("/api/matches/{id}/messages"),
        &t.access_token,
        json!({ "body": body }),
    )
    .await
}

async fn history(app: &TestApp, t: &Tokens, id: &str, query: &str) -> (StatusCode, Value) {
    app.get(
        &format!("/api/matches/{id}/messages{query}"),
        Some(&t.access_token),
    )
    .await
}

fn bodies(list: &Value) -> Vec<&str> {
    list.as_array()
        .expect("array")
        .iter()
        .map(|m| m["body"].as_str().expect("body"))
        .collect()
}

#[tokio::test]
async fn send_list_and_paginate_newest_first() {
    let app = TestApp::new().await;
    let (a, b, id) = matched(&app, "anna", "bob").await;

    let (status, sent) = say(&app, &a, &id, "  hello  ").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(sent["body"], "hello");
    assert_eq!(sent["match_id"], json!(id));
    assert_eq!(sent["sender_id"], json!(a.user_id));
    for text in ["two", "three", "four", "five"] {
        say(&app, &b, &id, text).await;
    }

    let (status, all) = history(&app, &a, &id, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bodies(&all), ["five", "four", "three", "two", "hello"]);

    let (_, page1) = history(&app, &b, &id, "?limit=2").await;
    assert_eq!(bodies(&page1), ["five", "four"]);
    let cursor = page1[1]["id"].as_str().expect("id");
    let (_, page2) = history(&app, &b, &id, &format!("?limit=2&before={cursor}")).await;
    assert_eq!(bodies(&page2), ["three", "two"]);
    let cursor = page2[1]["id"].as_str().expect("id");
    let (_, page3) = history(&app, &b, &id, &format!("?before={cursor}")).await;
    assert_eq!(bodies(&page3), ["hello"]);
}

#[tokio::test]
async fn validates_body_limit_and_cursor() {
    let app = TestApp::new().await;
    let (a, _b, id) = matched(&app, "anna", "bob").await;
    for body in ["", "   ", &"x".repeat(2001)] {
        let (status, err) = say(&app, &a, &id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err["error"]["code"], "validation");
    }
    assert_eq!(
        say(&app, &a, &id, &"x".repeat(2000)).await.0,
        StatusCode::CREATED
    );

    for q in ["?limit=0", "?limit=101", "?limit=abc", "?before=nope"] {
        let (status, _) = history(&app, &a, &id, q).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{q}");
    }
    assert_eq!(history(&app, &a, &id, "?limit=100").await.0, StatusCode::OK);
    let unknown = format!("?before={}", uuid::Uuid::new_v4());
    assert_eq!(
        history(&app, &a, &id, &unknown).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn strangers_and_unknown_matches_get_404() {
    let app = TestApp::new().await;
    let (_a, _b, id) = matched(&app, "anna", "bob").await;
    let eve = app.visible_user("eve", LAT, LON).await;

    assert_eq!(history(&app, &eve, &id, "").await.0, StatusCode::NOT_FOUND);
    assert_eq!(say(&app, &eve, &id, "hi").await.0, StatusCode::NOT_FOUND);
    let random = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        history(&app, &eve, &random, "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        history(&app, &eve, "not-a-uuid", "").await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = app.get(&format!("/api/matches/{id}/messages"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn block_forbids_messages_and_hides_the_match() {
    let app = TestApp::new().await;
    let (a, b, id) = matched(&app, "anna", "bob").await;
    say(&app, &a, &id, "before the block").await;
    let (_, list) = app.get("/api/matches", Some(&b.access_token)).await;
    assert_eq!(list.as_array().map(Vec::len), Some(1));

    app.post_as(
        "/api/blocks",
        &a.access_token,
        json!({ "user_id": b.user_id }),
    )
    .await;
    for t in [&a, &b] {
        let (status, err) = history(&app, t, &id, "").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(err["error"]["code"], "forbidden");
        assert_eq!(say(&app, t, &id, "hi").await.0, StatusCode::FORBIDDEN);
        let (_, list) = app.get("/api/matches", Some(&t.access_token)).await;
        assert_eq!(list, json!([]));
    }

    app.delete(&format!("/api/blocks/{}", b.user_id), &a.access_token)
        .await;
    assert_eq!(history(&app, &b, &id, "").await.0, StatusCode::OK);
}

#[tokio::test]
async fn matches_are_ordered_by_latest_activity_with_last_message() {
    let app = TestApp::new().await;
    let (a, b, with_b) = matched(&app, "anna", "bob").await;
    let c = app.visible_user("carl", LAT, LON + 0.0027).await;
    let with_c = app.match_up(&a, &c).await;

    // c's match is newer and has no messages yet.
    let (_, list) = app.get("/api/matches", Some(&a.access_token)).await;
    assert_eq!(list[0]["match_id"], json!(with_c));
    assert!(list[0]["last_message"].is_null());

    say(&app, &b, &with_b, "ping").await;
    let (status, list) = app.get("/api/matches", Some(&a.access_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["match_id"], json!(with_b));
    assert_eq!(list[0]["last_message"]["body"], "ping");
    assert_eq!(list[0]["other"]["user_id"], json!(b.user_id));
    assert_eq!(list[0]["other"]["display_name"], "Tester");
    assert!(
        list[0]["other"]["photo_url"]
            .as_str()
            .is_some_and(|u| u.starts_with("/media/"))
    );
    assert_eq!(list[1]["match_id"], json!(with_c));
}
