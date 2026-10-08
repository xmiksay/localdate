SHELL := /bin/bash
export CARGO_BUILD_JOBS := 4

# Load .env for DATABASE_URL & co. when present.
ifneq (,$(wildcard .env))
include .env
export
endif

BE := cd backend &&
FE := cd frontend &&

.PHONY: help install build lint check-pins fmt test test-unit test-integration migrate run-api run-web admin-grant admin-revoke image deploy clean

help: ## List targets
	@grep -hE '^[a-z-]+:.*##' Makefile | awk -F':.*## ' '{printf "  %-18s %s\n", $$1, $$2}'

install: ## Install frontend dependencies
	$(FE) npm ci

build: ## Build frontend, then the release API binary that embeds it
	$(FE) npm run build
	$(BE) cargo build --release

lint: check-pins ## pin check + fmt check + clippy + eslint + vue-tsc
	$(BE) cargo fmt --all -- --check
	$(BE) cargo clippy --all-targets -- -D warnings
	$(FE) npm run lint
	$(FE) npm run typecheck

check-pins: ## Dockerfile base images match rust-toolchain.toml and .nvmrc
	@rust=$$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml); \
	node=$$(tr -d '[:space:]' < .nvmrc); \
	grep -q "^FROM rust:$$rust-" Dockerfile || { echo "Dockerfile: expected FROM rust:$$rust-… (rust-toolchain.toml)"; exit 1; }; \
	grep -q "^FROM node:$$node-" Dockerfile || { echo "Dockerfile: expected FROM node:$$node-… (.nvmrc)"; exit 1; }

fmt: ## Format backend and frontend
	$(BE) cargo fmt --all
	$(FE) npm run format

test: test-unit test-integration ## All tests

test-unit: ## Rust unit tests + Vitest
	$(BE) cargo test --workspace --lib --bins
	$(FE) npm run test

test-integration: ## Rust integration tests (needs TEST_DATABASE_URL)
	$(BE) cargo test --workspace --test '*'

migrate: ## Apply migrations to DATABASE_URL
	$(BE) cargo run -p migration -- up

run-api: ## Run API server (applies migrations on start)
	$(BE) cargo run -p localdate-api

run-web: ## Run Vite dev server
	$(FE) npm run dev

admin-grant: ## Make ADMIN=<username> an admin (DATABASE_URL)
	@test -n "$(ADMIN)" || { echo "usage: make admin-grant ADMIN=<username>"; exit 1; }
	$(BE) cargo run -p localdate-api -- admin grant $(ADMIN)

admin-revoke: ## Take the admin role from ADMIN=<username>
	@test -n "$(ADMIN)" || { echo "usage: make admin-revoke ADMIN=<username>"; exit 1; }
	$(BE) cargo run -p localdate-api -- admin revoke $(ADMIN)

image: ## Build the container image localdate:dev
	docker build -t localdate:dev .

deploy: ## Apply deploy/k8s.yml to the current kubectl context (Secret must exist, see docs/deployment.md)
	kubectl apply -f deploy/k8s.yml

clean: ## Remove build artefacts
	$(BE) cargo clean
	rm -rf frontend/dist
