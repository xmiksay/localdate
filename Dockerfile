# syntax=docker/dockerfile:1
# Single deployable: the release `localdate-api` binary with the PWA (`frontend/dist`) embedded.
# Versions must match .nvmrc (Node major) and rust-toolchain.toml (Rust); `make check-pins` (part of
# `make lint`, so CI) fails on drift. rust-toolchain.toml is deliberately not copied in: rustup would
# then download its clippy/rustfmt components, which a release build doesn't need.
# Runtime and builder share the Debian release (glibc match).

# -- Frontend --
FROM node:22-trixie-slim AS frontend
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# -- Backend --
FROM rust:1.97.0-slim-trixie AS backend
ENV CARGO_BUILD_JOBS=4
WORKDIR /src
COPY backend/Cargo.toml backend/Cargo.lock backend/
COPY backend/api/Cargo.toml backend/api/build.rs backend/api/
COPY backend/entity/Cargo.toml backend/entity/
COPY backend/migration/Cargo.toml backend/migration/
# Dependency layer: stub every workspace target (and a placeholder bundle so build.rs passes) so
# third-party crates are compiled once and reused until Cargo.toml/Cargo.lock change.
RUN mkdir -p backend/api/src backend/entity/src backend/migration/src frontend/dist \
    && touch frontend/dist/index.html \
    && for f in api/src/lib.rs entity/src/lib.rs migration/src/lib.rs; do : > backend/$f; done \
    && for f in api/src/main.rs migration/src/main.rs; do echo 'fn main() {}' > backend/$f; done \
    && cd backend && cargo build --release --locked -p localdate-api
COPY backend/ backend/
COPY --from=frontend /src/frontend/dist frontend/dist
# COPY keeps the source mtimes, which can be older than the stub build; touch so cargo rebuilds
# the workspace crates and re-embeds the real bundle over the placeholder.
RUN find backend/api backend/entity backend/migration frontend/dist -type f -exec touch {} + \
    && cd backend && cargo build --release --locked -p localdate-api

# -- Runtime --
FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin localdate \
    && mkdir -p /data/photos && chown 10001:10001 /data/photos
COPY --from=backend /src/backend/target/release/localdate-api /usr/local/bin/localdate-api
ENV PHOTO_DIR=/data/photos \
    BIND_ADDR=0.0.0.0:3000 \
    RUST_LOG=info,sqlx=warn
USER 10001:10001
EXPOSE 3000
CMD ["localdate-api"]
