# Build the async-jobs subgraph (standalone crate, no FraiseQL dependency).
FROM rust:1-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/async-jobs-subgraph /usr/local/bin/async-jobs-subgraph
# Drop privileges: run as a non-root user
RUN groupadd -r app && useradd -r -g app app
USER app
ENV PORT=4001
EXPOSE 4001
HEALTHCHECK --interval=10s --timeout=5s --retries=5 \
    CMD curl -f http://localhost:4001/health || exit 1
CMD ["async-jobs-subgraph"]
