# FraiseQL v2 - Multi-stage Docker build
#
# Build stage: Compile Rust codebase
# Runtime stage: Slim production image

# ============================================================================
# STAGE 1: BUILDER
# ============================================================================
FROM rust:1.81-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy workspace and crates
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release -p fraiseql-server --locked

# Copy CLI binary as well (optional)
RUN cargo build --release -p fraiseql-cli --locked

# ============================================================================
# STAGE 2: RUNTIME
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies (PostgreSQL client, etc.)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary from builder
COPY --from=builder /app/target/release/fraiseql-server /usr/local/bin/

# Copy schema compilation CLI
COPY --from=builder /app/target/release/fraiseql-cli /usr/local/bin/

# Create schema directory
RUN mkdir -p /app/schemas

# Expose GraphQL endpoint port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Default environment
ENV RUST_LOG=info
ENV DATABASE_URL=postgresql://localhost/fraiseql
ENV FRAISEQL_BIND_ADDR=0.0.0.0:8000
ENV FRAISEQL_SCHEMA_PATH=/app/schemas/schema.compiled.json

# Run server
CMD ["fraiseql-server"]
