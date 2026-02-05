# syntax=docker/dockerfile:1.4

# Build arguments for cross-compilation
ARG TARGETARCH
ARG TARGETVARIANT

# Stage 1: Builder - use rust image for target arch
FROM --platform=$BUILDPLATFORM rust:1.85-slim AS builder

ARG TARGETARCH
ARG TARGETVARIANT

# Set Rust target based on architecture
RUN case "$TARGETARCH" in \
      amd64) TARGET="x86_64-unknown-linux-gnu" ;; \
      arm64) TARGET="aarch64-unknown-linux-gnu" ;; \
      arm) TARGET="armv7-unknown-linux-gnueabihf" ;; \
      ppc64le) TARGET="powerpc64le-unknown-linux-gnu" ;; \
      *) echo "Unsupported architecture: $TARGETARCH" && exit 1 ;; \
    esac && \
    echo "$TARGET" > /tmp/rust_target.txt && \
    rustup target add "$TARGET"

RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN TARGET=$(cat /tmp/rust_target.txt) && \
    cargo build --release --target "$TARGET" -p fraiseql-server

# Stage 2: Runtime
FROM debian:bookworm-slim

LABEL org.opencontainers.image.version="2.1.0" \
      org.opencontainers.image.vendor="FraiseQL" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.description="FraiseQL GraphQL execution engine" \
      org.opencontainers.image.documentation="https://github.com/fraiseql/fraiseql" \
      security.compliance="production" \
      security.hardenings="non-root,readonly-capable,capabilities-dropped"

# Security updates
RUN apt-get update && apt-get upgrade -y && apt-get install -y --no-install-recommends \
    libpq5 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user (UID 65532 for distroless compatibility)
RUN groupadd -g 65532 fraiseql && \
    useradd -r -u 65532 -g fraiseql -s /sbin/nologin -d /app fraiseql

# Create app directory with minimal permissions
RUN mkdir -p /app && chown -R fraiseql:fraiseql /app

WORKDIR /app

# Copy binary from builder (auto-detects target arch from build stage)
COPY --from=builder --chown=fraiseql:fraiseql /build/target/*/release/fraiseql-server .

USER fraiseql
EXPOSE 8815

ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8815/health || exit 1

CMD ["./fraiseql-server"]
