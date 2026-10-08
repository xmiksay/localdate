//! rust-embed does not tell cargo about the bundle folder, so without this a binary built before
//! `frontend/dist` existed (or changed) is reused as-is and silently ships a stale or empty PWA.

use std::path::Path;

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
}
