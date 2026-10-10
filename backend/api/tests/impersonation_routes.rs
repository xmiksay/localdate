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

/// Every function outside the extractor itself that takes or returns `ActingUser` (handlers and
/// the WebSocket's `authenticate`), as `path/in/src.rs::name`. Paired with the route list above:
/// a new method on an allowed path, or a handler mounted some other way (`route_service`, `nest`,
/// `on(…)`), cannot accept impersonation tokens without showing up here.
const ACTING_FNS: &[&str] = &[
    "areas/mod.rs::containing",
    "discovery/location.rs::update_location",
    "discovery/nearby.rs::get_nearby",
    "discovery/window.rs::get_window",
    "discovery/window.rs::start_window",
    "discovery/window.rs::extend_window",
    "discovery/window.rs::end_window",
    "me/filter.rs::get_filter",
    "me/filter.rs::put_filter",
    "me/mod.rs::get_me",
    "me/photos.rs::upload",
    "me/photos.rs::remove",
    "me/photos.rs::reorder",
    "me/profile.rs::put_profile",
    "social/matches.rs::list",
    "social/messages.rs::list",
    "social/messages.rs::send",
    "social/waves.rs::post_wave",
    "social/waves.rs::incoming",
    "ws/mod.rs::authenticate",
];

/// Where `ActingUser` is defined and checked; everything else must be listed in `ACTING_FNS`.
const EXTRACTOR: &str = "auth/extractor.rs";

/// (`file::fn` taking/returning `ActingUser`, stray uses outside any fn signature) under `src/`.
fn acting_uses(root: &Path, dir: &Path, fns: &mut BTreeSet<String>, stray: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            acting_uses(root, &path, fns, stray);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == EXTRACTOR {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        // Signature spans: from `fn ` to the body's `{` (or a `;` for a declaration).
        let mut spans = Vec::new();
        for (at, _) in text.match_indices("fn ") {
            let name: String = text[at + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let end = at + text[at..].find(['{', ';']).unwrap_or(text.len() - at);
            if text[at..end].contains("ActingUser") {
                fns.insert(format!("{rel}::{name}"));
            }
            spans.push(at..end);
        }
        for (at, _) in text.match_indices("ActingUser") {
            let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let line = text[line_start..].lines().next().unwrap_or("").trim_start();
            let declared = line.starts_with("use ") || line.starts_with("pub use ");
            let comment = line.starts_with("//");
            if !declared && !comment && !spans.iter().any(|s| s.contains(&at)) {
                stray.push(format!("{rel}: {line}"));
            }
        }
    }
}

#[test]
fn only_the_allow_listed_functions_accept_impersonation_tokens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let (mut fns, mut stray) = (BTreeSet::new(), Vec::new());
    acting_uses(&root, &root, &mut fns, &mut stray);
    let allowed: BTreeSet<String> = ACTING_FNS.iter().map(|s| (*s).to_owned()).collect();
    let unexpected: Vec<_> = fns.difference(&allowed).collect();
    let missing: Vec<_> = allowed.difference(&fns).collect();
    assert!(
        unexpected.is_empty(),
        "these take ActingUser but are not allow-listed in ACTING_FNS (is impersonation safe there?): {unexpected:?}"
    );
    assert!(
        missing.is_empty(),
        "allow-listed but no longer take ActingUser: {missing:?}"
    );
    assert!(
        stray.is_empty(),
        "ActingUser used outside a fn signature (closure handler?): {stray:?}"
    );
    assert_eq!(
        fns.len(),
        ACTING.len(),
        "one function per ACTING route (+ WS)"
    );
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
