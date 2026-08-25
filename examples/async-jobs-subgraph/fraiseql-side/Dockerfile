# FraiseQL (SQL-backed) subgraph.
#
# Runs the whole v2 pipeline, in the order the architecture prescribes:
#
#   authoring (Python)  ->  types.json
#   compilation (CLI)   ->  schema.compiled.json
#   runtime (server)    ->  serves the compiled schema, no authoring tools present
#
# Build context is the repository root, so the workspace and the in-repo authoring
# SDK are both available:
#
#   docker build -f examples/async-jobs-subgraph/fraiseql-side/Dockerfile .
#
# (docker-compose.yml sets this context for you.)
#
# Every step must fail the build if it fails. This file previously ended its compile
# step with `|| true` over a `schema.json` that no step generated, so the build was
# green and shipped an image whose server could not boot (#1071).

# The builder pins the same Debian release as the runtime stage below. `rust:1-slim`
# floats: it is Debian 13 (trixie) today, and a binary linked against trixie's glibc is
# not guaranteed to resolve on the bookworm runtime — the failure appears at container
# start, long after a green build.
FROM rust:1-slim-bookworm AS builder
WORKDIR /repo
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*
COPY . .
# `federation` is not a default feature. Without it the server boots, answers
# ordinary queries and reports healthy — and returns
# `Validation error: Federation is not enabled in this build` for the one query
# that makes it a subgraph, `query { _service { sdl } }`. rover asks exactly that,
# so composition fails against a service that looks entirely fine.
RUN cargo build --release -p fraiseql-cli -p fraiseql-server \
    --features fraiseql-server/federation

# Authoring: schema.py declares the types; running it writes types.json.
#
# The SDK is installed from THIS REPOSITORY rather than PyPI so the example always builds
# against the version it ships with — and installed rather than put on PYTHONPATH so that
# its declared dependencies come with it. A bare PYTHONPATH import fails outright:
# `fraiseql/__init__.py` imports the async client, which imports httpx.
RUN pip install --break-system-packages --no-cache-dir ./sdks/official/fraiseql-python
WORKDIR /repo/examples/async-jobs-subgraph/fraiseql-side
RUN python3 schema.py

# Compilation: types.json (shape) + fraiseql.toml (queries, federation, database)
# -> schema.compiled.json. This is the only artifact the runtime image needs.
RUN /repo/target/release/fraiseql-cli compile fraiseql.toml \
    --types types.json \
    -o schema.compiled.json

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# Runtime carries the server and the compiled schema — no CLI, no Python, no
# authoring sources. That separation is the point of the architecture.
COPY --from=builder /repo/target/release/fraiseql-server /usr/local/bin/fraiseql-server
COPY --from=builder /repo/examples/async-jobs-subgraph/fraiseql-side/schema.compiled.json ./
COPY examples/async-jobs-subgraph/fraiseql-side/server.toml ./
# Drop privileges: run as a non-root user (schema files stay world-readable)
RUN groupadd -r app && useradd -r -g app app
USER app
ENV FRAISEQL_CONFIG=/app/server.toml
ENV FRAISEQL_SCHEMA_PATH=/app/schema.compiled.json
ENV FRAISEQL_BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
    CMD curl -f http://localhost:8080/health || exit 1
CMD ["fraiseql-server"]
