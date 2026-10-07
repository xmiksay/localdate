SHELL := /bin/bash
export CARGO_BUILD_JOBS := 4

# Load .env for DATABASE_URL & co. when present.
ifneq (,$(wildcard .env))
include .env
export
endif

BE := cd backend &&
FE := cd frontend &&

.PHONY: help install build lint fmt test test-unit test-integration migrate run-api run-web clean

help: ## List targets
	@grep -hE '^[a-z-]+:.*##' Makefile | awk -F':.*## ' '{printf "  %-18s %s\n", $$1, $$2}'

install: ## Install frontend dependencies
	$(FE) npm ci

build: ## Build frontend, then the release API binary that embeds it
	$(FE) npm run build
	$(BE) cargo build --release

lint: ## fmt check + clippy + eslint + vue-tsc
	$(BE) cargo fmt --all -- --check
	$(BE) cargo clippy --all-targets -- -D warnings
	$(FE) npm run lint
	$(FE) npm run typecheck

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

clean: ## Remove build artefacts
	$(BE) cargo clean
	rm -rf frontend/dist
