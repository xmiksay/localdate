# Deployment

Production runs on the k8s cluster behind ingress-nginx + cert-manager (`letsencrypt-prod`) at
<https://localdate.mmik.cz>. Everything lives in namespace `localdate`, defined in one file:
[`deploy/k8s.yml`](../deploy/k8s.yml).

| Object | What |
|---|---|
| `ConfigMap localdate` | non-secret env (`BIND_ADDR`, `PHOTO_DIR`, `RUST_LOG`, `CLEANUP_INTERVAL_SECS`, `TRUST_PROXY_HEADERS=true`) |
| `Secret localdate` | `POSTGRES_PASSWORD`, `JWT_SECRET` — **not in git**, created by hand (below) |
| `StatefulSet localdate-db` + headless `Service` | Postgres 18, 5 Gi PVC `data-localdate-db-0` |
| `NetworkPolicy localdate-db` | only `app=localdate-api` pods may reach 5432 |
| `Deployment localdate-api` | 1 replica, `Recreate`, 768 Mi memory limit, 5 Gi PVC `localdate-photos` at `/data/photos` |
| `NetworkPolicy localdate-api` | only namespace `ingress-nginx` may reach 3000 (adjust if the controller lives elsewhere) |
| `Service` + `Ingress localdate` | class `nginx`, TLS for `localdate.mmik.cz`, body limit 11m, 1 h proxy timeouts for `/api/ws` |

The NetworkPolicies only take effect if the cluster's CNI enforces them.

Why one replica: photos live on the `ReadWriteOnce` PVC, which only one pod can mount; more replicas
need shared photo storage (S3, #21). WebSocket push is not the blocker any more — replicas fan events
out to each other through Postgres LISTEN/NOTIFY (architecture.md "Realtime"). Why `Recreate`: the photos PVC is
`ReadWriteOnce`, so the old pod must release it before the new one starts (a few seconds of downtime
per rollout). The PVCs use the cluster's default StorageClass; add `storageClassName` if there is none.

**Client IP.** `TRUST_PROXY_HEADERS=true` makes the login/register rate limiter key on the first
`X-Forwarded-For` entry. ingress-nginx overwrites that header with the address *it* sees, so this only
works if ingress-nginx sees the real client IP. That means a hostNetwork controller, or a controller
Service with `externalTrafficPolicy: Local`, or PROXY protocol between a load balancer and the
controller. If kube-proxy SNATs traffic on the way in, every user shares one rate-limit bucket. To
check, look for real public IPs (not node or pod addresses) in the ingress-nginx access log.

Never enable `use-forwarded-headers: true` in the ingress-nginx ConfigMap without narrowing
`proxy-real-ip-cidr`. Its default is `0.0.0.0/0`, so the controller would take the client's own
`X-Forwarded-For` as the real IP. Every client could then pick its own bucket and get around the
limiter. The same applies to `compute-full-forwarded-for: true`: it appends to the header instead of
replacing it, so the first entry becomes whatever the client sent.

IPv6 clients are bucketed per /64 prefix, so rotating addresses inside one prefix doesn't help.

## Image

[`Dockerfile`](../Dockerfile): Node 22 builds `frontend/dist`. Rust then builds the release
`localdate-api`, which embeds the bundle. The runtime is `debian:trixie-slim` running as uid 10001,
with `PHOTO_DIR=/data/photos` and `BIND_ADDR=0.0.0.0:3000`. The Rust base tag must match
`rust-toolchain.toml` and the Node tag must match `.nvmrc`. `make check-pins` enforces both and runs
as part of `make lint`. Dependencies are compiled in a separate layer from stubbed sources, so a
code-only change rebuilds just the workspace crates. A new workspace crate has to be added to that
stub list.

- Local: `make image` → `localdate:dev`.
- CI ([`ci.yml`](../.github/workflows/ci.yml)) only builds the image after the `gate` job (lint +
  test) passes:
  - On pull requests, `image-build` builds it without pushing, so Dockerfile breakage shows up before
    merge.
  - On a push to `master`, `image-publish` pushes `ghcr.io/xmiksay/localdate:master` and
    `:sha-<short>`. On a `v*` tag it pushes `:<version>`. Only this job gets `packages: write`.
- The package must be public for the cluster to pull it without credentials. Otherwise add an
  `imagePullSecrets` entry with a read-only ghcr token.

Smoke-test an image against the host Postgres:

```sh
docker run --rm --network host -e DATABASE_URL=postgres://localdate:localdate@localhost/localdate \
  -e JWT_SECRET="$(openssl rand -hex 32)" -e BIND_ADDR=127.0.0.1:3900 localdate:dev
curl -s localhost:3900/api/health; curl -sI localhost:3900/
```

## First deploy

`make deploy` runs `kubectl apply -f deploy/k8s.yml` against the **current kubectl context** — check
it (`kubectl config current-context`) first.

```sh
kubectl apply -f deploy/k8s.yml                 # namespace etc.; pods wait for the Secret
kubectl -n localdate create secret generic localdate \
  --from-literal=POSTGRES_PASSWORD="$(openssl rand -hex 24)" \
  --from-literal=JWT_SECRET="$(openssl rand -hex 32)"
kubectl -n localdate rollout status statefulset/localdate-db
kubectl -n localdate rollout status deployment/localdate-api
```

`POSTGRES_PASSWORD` is expanded into `DATABASE_URL` by the kubelet, so keep it URL-safe (hex is).
Postgres only reads it when initialising an empty volume: rotating it later means `ALTER ROLE
localdate PASSWORD '…'` inside the database **and** updating the Secret, then restarting the API.
Rotating `JWT_SECRET` invalidates outstanding access tokens; clients recover through their refresh token.
[`deploy/secrets.example.yml`](../deploy/secrets.example.yml) is the same Secret as a template.

## Updates and migrations

On start the API retries the Postgres connection with backoff for 60 s, then gives up. The last
attempt can add up to 30 s if it times out. This matters on the first deploy, when the DB and the API start together. It then
runs migrations (`Migrator::up`); the startup probe gives it up to 5 minutes. With `Recreate`, the
old binary never runs against the new schema.

```sh
# New master build (the manifest tracks :master with imagePullPolicy: Always)
kubectl -n localdate rollout restart deployment/localdate-api
# After changing the ConfigMap: env is read at start only, so restart too
kubectl apply -f deploy/k8s.yml && kubectl -n localdate rollout restart deployment/localdate-api
# Rollback: pin a known-good build (tags are listed on the ghcr package page)
kubectl -n localdate set image deployment/localdate-api api=ghcr.io/xmiksay/localdate:sha-<short>
```

A pinned image stays until the next `make deploy`, which puts `:master` back. You can only roll back
to a build that knows every migration already applied to the database. An older binary refuses to
start when the database has a migration it doesn't know. Rolling back across a migration therefore
means restoring a database backup.

## Admin role

Run the CLI inside the API pod, where `DATABASE_URL` is already set. It refuses to run while
migrations are pending, so the server has to have started (and migrated) first. After a rollout,
wait for `rollout status` before running it.

```sh
kubectl -n localdate exec deploy/localdate-api -- localdate-api admin grant <username>
kubectl -n localdate exec deploy/localdate-api -- localdate-api admin revoke <username>
```

## Backups

Two things hold state: the database and the photos volume. Back up both together (a photo row
without its file renders a broken image; a file without a row is just garbage).

```sh
# Database (custom format; restore with pg_restore -d localdate)
kubectl -n localdate exec localdate-db-0 -- pg_dump -U localdate -Fc localdate > localdate-$(date +%F).dump
# Photos
kubectl -n localdate exec deploy/localdate-api -- tar -C /data -cf - photos > photos-$(date +%F).tar
```

Dumps contain personal data (profiles, birth dates, messages, photos) — store them encrypted.
