//! Impersonation is deny-by-default: only the `ActingUser` routes below accept an impersonation
//! token. Every route in the source must be classified here, so a new endpoint fails this test
//! until someone decides whether an admin acting as a user may call it.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use axum::http::{Method, StatusCode};
use common::{TestApp, count};
use serde_json::json;

/// Open to an admin acting as the user (`ActingUser`).
const ACTING: &[(&str, &str)] = &[
    ("GET", "/me"),
    ("PUT", "/me/profile"),
    ("GET", "/me/filter"),
    ("PUT", "/me/filter"),
    ("POST", "/me/photos"),
    ("PUT", "/me/photos/order"),
    ("DELETE", "/me/photos/{id}"),
    ("GET", "/me/window"),
    ("POST", "/me/window"),
    ("PATCH", "/me/window"),
    ("DELETE", "/me/window"),
    ("POST", "/me/location"),
    ("GET", "/nearby"),
    ("GET", "/areas"),
    ("POST", "/waves"),
    ("GET", "/waves/incoming"),
    ("GET", "/matches"),
    ("GET", "/matches/{id}/messages"),
    ("POST", "/matches/{id}/messages"),
    // Covered by tests/impersonation.rs (a plain GET here is no WebSocket handshake).
    ("WS", "/ws"),
];

/// Authenticated, and refused to an impersonation token (`AuthUser` / `AdminUser`).
const DENIED: &[(&str, &str)] = &[
    ("DELETE", "/me"),
    ("PUT", "/me/password"),
    ("GET", "/me/identities"),
    ("POST", "/me/identities/email"),
    ("POST", "/me/identities/email/confirm"),
    ("DELETE", "/me/identities/{id}"),
    ("POST", "/auth/oauth/{provider}/link"),
    ("POST", "/auth/oauth/{provider}/import"),
    ("POST", "/me/push/subscriptions"),
    ("DELETE", "/me/push/subscriptions"),
    ("GET", "/me/push/prefs"),
    ("PATCH", "/me/push/prefs"),
    ("GET", "/blocks"),
    ("POST", "/blocks"),
    ("DELETE", "/blocks/{user_id}"),
    ("POST", "/reports"),
    ("GET", "/admin/reports"),
    ("POST", "/admin/reports/{id}/dismiss"),
    ("POST", "/admin/users/{id}/ban"),
    ("POST", "/admin/users/{id}/unban"),
    ("GET", "/admin/users"),
    ("POST", "/admin/users/{id}/impersonate"),
    ("GET", "/admin/settings"),
    ("GET", "/admin/audit"),
    ("GET", "/admin/test-users"),
    ("POST", "/admin/test-users"),
    ("DELETE", "/admin/test-users/{id}"),
    ("POST", "/admin/test-users/{id}/photos"),
    ("GET", "/admin/areas"),
    ("POST", "/admin/areas"),
    ("PUT", "/admin/areas/{id}"),
    ("DELETE", "/admin/areas/{id}"),
];

/// No access token involved at all.
const PUBLIC: &[&str] = &[
    "/health",
    "/interests",
    "/push/config",
    "/auth/register",
    "/auth/login",
    "/auth/refresh",
    "/auth/logout",
    "/auth/providers",
    "/auth/email/start",
    "/auth/email/preview",
    "/auth/email/verify",
    "/auth/email/signup",
    "/auth/password/forgot",
    "/auth/password/reset",
    "/auth/password/reset/preview",
    "/auth/oauth/{provider}/start",
    "/auth/oauth/{provider}/callback",
    "/auth/oauth/exchange",
    "/auth/oauth/signup",
    // The `/media` router.
    "/{name}",
];

/// The first string literal after every `.route(` in `dir`, recursively.
fn source_routes(dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            source_routes(&path, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        for (at, _) in text.match_indices(".route(") {
            let rest = &text[at..];
            let start = rest.find('"').expect("route literal") + 1;
            let len = rest[start..].find('"').expect("closing quote");
            out.insert(rest[start..start + len].to_owned());
        }
    }
}

#[test]
fn every_route_is_classified_for_impersonation() {
    let mut found = BTreeSet::new();
    source_routes(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    let classified: BTreeSet<String> = ACTING
        .iter()
        .chain(DENIED)
        .map(|(_, p)| *p)
        .chain(PUBLIC.iter().copied())
        .map(str::to_owned)
        .collect();
    let unclassified: Vec<_> = found.difference(&classified).collect();
    let stale: Vec<_> = classified.difference(&found).collect();
    assert!(
        unclassified.is_empty(),
        "classify these routes in tests/impersonation_routes.rs: {unclassified:?}"
    );
    assert!(stale.is_empty(), "no longer routed: {stale:?}");
}

fn concrete(template: &str) -> String {
    let path = template.replace("{provider}", "google");
    let id = uuid::Uuid::new_v4().to_string();
    let mut out = String::new();
    let mut rest = path.as_str();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        out.push_str(&id);
        let close = rest[open..].find('}').expect("closing brace");
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    format!("/api{out}")
}

#[tokio::test]
async fn only_acting_routes_accept_an_impersonation_token_and_all_are_audited() {
    let app = TestApp::with_config(|c| c.admin_impersonation = true).await;
    let admin = app.admin("mod").await;
    let target = app.register("tester").await;
    let (status, body) = app
        .post_as(
            &format!("/api/admin/users/{}/impersonate", target.user_id),
            &admin.access_token,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token = body["access_token"].as_str().expect("token").to_owned();

    let mut sent = 0;
    for (method, template) in DENIED {
        let method: Method = method.parse().expect("method");
        let body = (method != Method::GET && method != Method::DELETE).then(|| json!({}));
        let (status, res) = app
            .request(method.clone(), &concrete(template), Some(&token), body)
            .await;
        sent += 1;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {template}: {res}");
        assert_eq!(
            res["error"]["code"], "impersonation_forbidden",
            "{method} {template}"
        );
    }
    for (method, template) in ACTING.iter().filter(|(m, _)| *m != "WS") {
        let method: Method = method.parse().expect("method");
        let body = (method != Method::GET && method != Method::DELETE).then(|| json!({}));
        let (status, res) = app
            .request(method.clone(), &concrete(template), Some(&token), body)
            .await;
        sent += 1;
        assert_ne!(
            res["error"]["code"], "impersonation_forbidden",
            "{method} {template}: {status} {res}"
        );
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {template}");
    }
    // Every request with the token is audited, GETs and refused ones included.
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM admin_audit WHERE action = 'impersonated_request'"
        )
        .await,
        sent
    );
}
