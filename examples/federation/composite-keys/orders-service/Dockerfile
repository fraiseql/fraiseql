FROM rust:latest AS builder

WORKDIR /app

# Copy FraiseQL CLI
COPY fraiseql-cli ./fraiseql-cli

# Build FraiseQL CLI
RUN cargo build --release --manifest-path fraiseql-cli/Cargo.toml

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    curl \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy schema
COPY schema.py .

# Copy compiled CLI
COPY --from=builder /app/target/release/fraiseql-cli /usr/local/bin/fraiseql

# Compile schema
RUN fraiseql compile schema.py

# Expose port
EXPOSE 4000

# Health check
HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
    CMD curl -f http://localhost:4000/health || exit 1

# Start FraiseQL server with federation config
CMD ["fraiseql", "server", \
     "--schema", "schema.compiled.json", \
     "--database", "${DATABASE_URL}", \
     "--port", "4000", \
     "--federation", \
     "--federation-config", "federation.toml"]
