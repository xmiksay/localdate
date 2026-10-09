# localdate

Meet people nearby right now — missed bus, waiting for a train, first day in a new city, a festival where you know no one.
Open a time-boxed visibility window and see everyone around you whose filters match yours (and yours theirs).
Wave; a mutual wave opens a chat. PWA, Rust backend.

- Architecture & data model: [docs/architecture.md](docs/architecture.md) (index) + [docs/architecture/](docs/architecture/)
- API contract: [docs/api.md](docs/api.md) (index) + [docs/api/](docs/api/)
- Deployment (Docker image, Kubernetes, secrets, backups): [docs/deployment.md](docs/deployment.md)
- Roadmap: [MVP epic #29](https://github.com/xmiksay/localdate/issues/29)

## Development

Requires Rust (pinned in `rust-toolchain.toml`), Node 22 (`.nvmrc`) and a local Postgres 18.

```sh
cp .env.example .env          # adjust DATABASE_URL / TEST_DATABASE_URL / JWT_SECRET
make install                  # frontend deps
make run-api                  # API on 127.0.0.1:3000 (migrates on start)
make run-web                  # PWA on http://localhost:5173
```

| Target | What |
|---|---|
| `make lint` | `check-pins` (Dockerfile base images vs `rust-toolchain.toml`/`.nvmrc`), rustfmt check, clippy `-D warnings`, eslint, vue-tsc |
| `make test` | `test-unit` (Rust lib/bin + Vitest) and `test-integration` (Rust, fresh DB per test) |
| `make fmt` | format everything |
| `make migrate` | apply migrations to `DATABASE_URL` |
| `make icons` | regenerate the committed PNG icons (`frontend/public/*.png`) from the SVGs (needs `rsvg-convert`) |
| `make vapid-keys` | print a fresh VAPID key pair (`VAPID_PUBLIC_KEY` / `VAPID_PRIVATE_KEY`) to turn on Web Push |
| `make admin-grant ADMIN=<username>` / `make admin-revoke ADMIN=<username>` | give / take the moderator role (`localdate-api admin grant\|revoke <username>`; exits non-zero for an unknown user or pending migrations — run `make migrate` first) |
| `make build` | frontend bundle, then the release `localdate-api` binary that embeds it (single deployable: API + PWA) |
| `make image` | container image `localdate:dev` |
| `make deploy` | `kubectl apply -f deploy/k8s.yml` to the current context (see [docs/deployment.md](docs/deployment.md)) |
| `make deploy-letsgo` / `make deploy-letsgo-diff` | apply / diff the letsgo.sc-l.eu example kustomization `deploy/letsgo` (see [its runbook](deploy/letsgo/README.md)) |

The test database role needs `CREATEDB`: integration tests create and drop `localdate_test_<uuid>` databases.
