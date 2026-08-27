# Docker

Two Compose files ship in this repository, and they answer different questions.

| File | What it is | Verified by |
|---|---|---|
| [`../docker-compose.yml`](../docker-compose.yml) | **The canonical stack.** FraiseQL + PostgreSQL, on a published, version-pinned image. | `tools/compose-stack-test.sh`, a step on the `Dagger — image` CI leg |
| [`docker-compose.test.yml`](docker-compose.test.yml) | The **test rig**: PostgreSQL (+ a streaming standby, a failover standby, a TLS server), Redis, NATS, Vault. Not a deployment. | `make db-up` and every integration leg |

Every other compose file in the tree carries a `Not CI-verified` line in its own
header. That is a rule, not a convention: `tools/compose-stack-test.sh` discovers every
compose file in the repository and fails if one is neither the canonical stack, nor a
rig it names, nor marked unverified.

---

## Running the canonical stack

It requires three inputs and starts without none of them. Copy
[`../.env.example`](../.env.example) to `.env` beside `docker-compose.yml`:

```bash
DB_PASSWORD=<a real password>
FRAISEQL_SCHEMA_FILE=/abs/path/to/schema.compiled.json
FRAISEQL_CONFIG_FILE=/abs/path/to/fraiseql.toml
```

- **The schema** comes from `fraiseql compile schema.json`. The image bakes none, and
  the server exits at startup when `FRAISEQL_SCHEMA_PATH` names no file.
- **The config** must exist because production mode is the *default* — anything but
  `FRAISEQL_ENV=development` — and it refuses to start with CORS enabled and no
  origins. `cors_origins` has **no environment variable**; a `fraiseql.toml`
  containing one line is enough:

  ```toml
  cors_origins = ["https://app.example.com"]
  ```

Then:

```bash
docker compose up -d
curl -fsS http://localhost:8000/health
```

Each input is declared `${VAR:?…}`, so an unset one aborts `docker compose up` with an
instruction naming it — rather than starting a container that exits.

⚠ `FRAISEQL_SCHEMA_FILE` and `FRAISEQL_CONFIG_FILE` must name files that **exist**.
Docker creates a *directory* for a missing bind-mount source and mounts that, which
turns a clear "file not found" into a parse error somewhere else.

## What the stack deliberately does not set

No `healthcheck:` and no `FRAISEQL_BIND_ADDR`. The image owns both — it sets
`FRAISEQL_BIND_ADDR=0.0.0.0:8000` and carries a `HEALTHCHECK` on 8000. Restating them
in the compose file would mean the CI gate agrees with the image by construction and
could never disagree with it, which is exactly how the published image spent six
months permanently `unhealthy`, its `EXPOSE` and `HEALTHCHECK` naming 8815 while the
process listened on 8000 (#1216).

## Ports

| Service | Published on | Why |
|---|---|---|
| `fraiseql` | `8000` (all interfaces) | The application's own port. |
| `postgres` | `127.0.0.1:5432` | Loopback only. Docker's port publishing bypasses host firewalls, so an unqualified `5432:5432` exposes the database to the internet regardless of `ufw` rules. |

Gated by `tools/check-deploy-security.sh`.

## The test rig

```bash
make db-up        # start, and wait for every service to report healthy
make db-status
make db-reset     # fresh volumes
make db-down
```

PostgreSQL listens on `127.0.0.1:5433` (not 5432 — the canonical stack uses that one).
`make db-up` also generates the TLS fixtures the TLS suites need; bring the rig up with
that target rather than by hand.

## The image

Built from [`../Dockerfile`](../Dockerfile) — a multi-stage build whose runtime stage is
`debian:bookworm-slim` running as uid 65532.

```bash
dagger call images --source=.          # build every variant this repo publishes
make image-boot                        # boot each one and query it
make image-properties                  # assert what the built image IS
make compose-stack                     # bring up docker-compose.yml on it and query it
make chart-deploy                      # deploy the Helm chart on it and query it
```

Publishing is `\.github/workflows/docker-build.yml`, on `v*` tags only. It pushes
`ghcr.io/fraiseql/{server,server-full,tutorial}` and the `fraiseql/*` Docker Hub
mirrors. Nothing else in CI pushes an image.

## Elsewhere

- Kubernetes and Helm: [`../deploy/kubernetes/`](../deploy/kubernetes/)
- Deployment runbook: [`../docs/runbooks/01-deployment.md`](../docs/runbooks/01-deployment.md)
- Getting started: [`../docs/guides/getting-started.md`](../docs/guides/getting-started.md)
- Examples: [`../examples/README.md`](../examples/README.md)
