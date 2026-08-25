# Orders subgraph.
#
# Runs the whole v2 pipeline, in the order the architecture prescribes:
#
#   authoring (Python)  ->  schema.json
#   compilation (CLI)   ->  schema.compiled.json
#   runtime (server)    ->  serves the compiled schema, no authoring tools present
#
# Build context is the REPOSITORY ROOT, so the workspace and the in-repo authoring SDK
# are both available. docker-compose.yml sets that for you; by hand it is:
#
#   docker build -f examples/federation/basic/orders-service/Dockerfile .
#
# The previous version of this file copied a `fraiseql-cli` directory into a context
# holding only `Dockerfile` and `schema.py`, and could not have built (#1050). Copying
# that one directory would not have helped either: fraiseql-cli is a workspace member
# whose manifest inherits its dependencies with `workspace = true`, so it cannot be
# compiled standalone.

# Builder and runtime pin the same Debian release. `rust:1-slim` floats — it is Debian
# 13 today — and a binary linked against a newer glibc than the runtime base fails when
# the container starts, not when it builds.
FROM rust:1-slim-bookworm AS builder
WORKDIR /repo
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*
COPY . .

# `federation` is not a default feature. Without it the server boots, answers ordinary
# queries and reports healthy, and returns `Federation is not enabled in this build` for
# `query { _service { sdl } }` — the one query that makes it a subgraph.
RUN cargo build --release -p fraiseql-cli -p fraiseql-server \
    --features fraiseql-server/federation

# The SDK is installed from this repository rather than PyPI so the example always
# builds against the version it ships with, and installed rather than put on PYTHONPATH
# so its declared dependencies come with it.
RUN pip install --break-system-packages --no-cache-dir ./sdks/official/fraiseql-python
WORKDIR /repo/examples/federation/basic/orders-service
RUN python3 schema.py
RUN /repo/target/release/fraiseql-cli compile schema.json -o schema.compiled.json

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# Runtime carries the server and the compiled schema — no CLI, no Python, no authoring
# sources. That separation is the point of the architecture.
COPY --from=builder /repo/target/release/fraiseql-server /usr/local/bin/fraiseql-server
COPY --from=builder /repo/examples/federation/basic/orders-service/schema.compiled.json ./
COPY examples/federation/basic/server.toml ./
RUN groupadd -r app && useradd -r -g app app
USER app
ENV FRAISEQL_CONFIG=/app/server.toml
ENV FRAISEQL_SCHEMA_PATH=/app/schema.compiled.json
ENV FRAISEQL_BIND_ADDR=0.0.0.0:4002
EXPOSE 4002
HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
    CMD curl -f http://localhost:4002/health || exit 1
CMD ["fraiseql-server"]
