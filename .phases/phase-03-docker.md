# Phase 03: Docker Image & CI

## Status
[ ] Not Started

## Objective
Fix the Docker build CI job and publish a `full` image variant that includes
optional compile-time features (`grpc-transport`, `rest-transport`).

## Dependencies
- Phase 01 (nightly fmt) — Format Check must be green before Docker CI runs usefully

---

## Cycle 1 — Verify the root Dockerfile builds

### Problem
The `docker-build.yml` CI job builds `Dockerfile` at the repo root with:
```
cargo build --release --target "$TARGET" -p fraiseql-server
```

No features are specified, so `grpc-transport`, `rest-transport`, and `arrow`
are **not** included in the official image. This is the root cause of issue #80.

### Pre-check
```bash
docker build -f Dockerfile . --target runtime --build-arg TARGETARCH=amd64
```

Fix any build errors before adding feature variants.

---

## Cycle 2 — Add a `full` image variant (issue #80)

### Problem
Users who want gRPC or REST transport must build from source. There is no way
to activate these features from the prebuilt Docker image. (#80)

### Fix
Add a build matrix entry to `docker-build.yml` for a `full` image tag that
compiles with all stable optional features:

```yaml
- name: fraiseql-server-full
  dockerfile: Dockerfile
  build-context: .
  image-suffix: "server-full"
  build-args: |
    CARGO_FEATURES=grpc-transport,rest-transport,arrow
  optional: true
```

Update `Dockerfile` to accept and forward the `CARGO_FEATURES` build arg:

```dockerfile
ARG CARGO_FEATURES=""

RUN TARGET=$(cat /tmp/rust_target.txt) && \
    if [ -n "$CARGO_FEATURES" ]; then \
      cargo build --release --target "$TARGET" -p fraiseql-server --features "$CARGO_FEATURES"; \
    else \
      cargo build --release --target "$TARGET" -p fraiseql-server; \
    fi
```

The `:latest` image remains feature-minimal (stable, small). The `:full` tag
includes `grpc-transport` + `rest-transport` and is documented in the README.

### Docker image tag strategy
| Tag | Features | Use case |
|-----|----------|----------|
| `latest` / `2.1.x` | default (postgres, rich-filters) | standard deployment |
| `full` / `2.1.x-full` | + grpc-transport, rest-transport, arrow | advanced transport + Arrow Flight |

---

## Success Criteria
- [ ] `docker build -f Dockerfile . --target runtime` succeeds locally
- [ ] `docker build -f Dockerfile . --target runtime --build-arg CARGO_FEATURES=grpc-transport,rest-transport,arrow` succeeds
- [ ] `docker-build.yml` matrix includes a `server-full` entry
- [ ] `Dockerfile` accepts `CARGO_FEATURES` ARG and forwards to cargo
- [ ] Container Security Scan passes (no unaddressed CRITICAL CVEs)

## Branch Strategy
Work on a feature branch (e.g. `feat/docker-full-image`), merge to `dev` via PR.

## Closes
- Issue #80 (grpc-transport missing from official Docker image)
