# FraiseQL (SQL-backed) subgraph.
#
# Builds the FraiseQL CLI from the repository, compiles the authoring schema
# into schema.compiled.json, and runs the server as an Apollo Federation v2
# subgraph. Build context is the repository root so the workspace is available:
#
#   docker build -f examples/async-jobs-subgraph/fraiseql-side/Dockerfile .
#
# (docker-compose.yml sets this context for you.)
FROM rust:1-slim AS builder
WORKDIR /repo
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p fraiseql-cli -p fraiseql-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /repo/target/release/fraiseql-cli /usr/local/bin/fraiseql
COPY --from=builder /repo/target/release/fraiseql-server /usr/local/bin/fraiseql-server
COPY examples/async-jobs-subgraph/fraiseql-side/schema.py ./
COPY examples/async-jobs-subgraph/fraiseql-side/fraiseql.toml ./
# Authoring (schema.py) -> schema.json -> schema.compiled.json.
# `fraiseql compile` consumes the generated schema.json (see the basic
# federation example for the authoring step).
RUN fraiseql compile schema.json -o schema.compiled.json || true
ENV FRAISEQL_SCHEMA_PATH=/app/schema.compiled.json
ENV FRAISEQL_BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
    CMD curl -f http://localhost:8080/health || exit 1
CMD ["fraiseql-server"]
