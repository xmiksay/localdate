//! rust-embed does not tell cargo about the bundle folder, so without this a binary built before
//! `frontend/dist` existed (or changed) is reused as-is and silently ships a stale or empty PWA.
//!
//! Also bakes the build identity into the binary (`GET /api/health`): `LOCALDATE_COMMIT` from the
//! `GIT_SHA` env (the image's build arg), else the checkout's HEAD, else "dev"; `LOCALDATE_BUILT_AT`
//! from `BUILD_TIME`, only when set.

use std::path::Path;
use std::process::Command;

const DIST: &str = "../../frontend/dist";

fn main() {
    println!("cargo::rerun-if-changed={DIST}");
    // Debug builds (clippy, tests, CI) must work without a frontend build; a release binary is the
    // deployable, so shipping one without the PWA is always a mistake.
    let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
    if release && !Path::new(DIST).join("index.html").exists() {
        panic!(
            "frontend/dist/index.html is missing: run `make build`, which builds the frontend \
             before the release binary that embeds it"
        );
    }

    println!("cargo::rustc-env=LOCALDATE_COMMIT={}", commit());
    println!("cargo::rerun-if-env-changed=BUILD_TIME");
    if let Some(t) = std::env::var("BUILD_TIME")
        .ok()
        .filter(|t| !t.trim().is_empty())
    {
        println!("cargo::rustc-env=LOCALDATE_BUILT_AT={}", t.trim());
    }
}

fn commit() -> String {
    println!("cargo::rerun-if-env-changed=GIT_SHA");
    if let Some(sha) = std::env::var("GIT_SHA").ok().and_then(|s| short_sha(&s)) {
        return sha;
    }
    git_head().unwrap_or_else(|| "dev".into())
}

/// The first 7 hex chars; anything else (empty, the Dockerfile's "unknown") is no sha.
fn short_sha(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() >= 7 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s[..7].to_ascii_lowercase())
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    out.status.success().then(|| text.trim().to_owned())
}

fn git_head() -> Option<String> {
    let sha = short_sha(&git(&["rev-parse", "HEAD"])?)?;
    // A checkout moves HEAD, a commit moves the branch ref: rerun on those only, not on every build.
    // A path that doesn't exist (packed ref) would make cargo rerun every time, so skip it.
    let mut watched = vec!["HEAD".to_owned()];
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for r in watched {
        if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", &r])
            && Path::new(&p).exists()
        {
            println!("cargo::rerun-if-changed={p}");
        }
    }
    Some(sha)
}
