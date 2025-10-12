# Multi-stage Dockerfile for FraiseQL
# Optimized for production with security best practices

# Stage 1: Builder
FROM python:3.13-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    g++ \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /build

# Copy dependency files first for better caching
COPY pyproject.toml README.md ./
COPY src ./src

# Build wheel
RUN pip install --no-cache-dir build && \
    python -m build --wheel

# Stage 2: Runtime
FROM python:3.13-slim AS runtime

# Labels for metadata
LABEL org.opencontainers.image.authors="FraiseQL Team"
LABEL org.opencontainers.image.version="0.1.0"
LABEL org.opencontainers.image.description="Production-ready GraphQL-to-PostgreSQL framework"

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r fraiseql && useradd -r -g fraiseql fraiseql

# Set working directory
WORKDIR /app

# Copy wheel from builder
COPY --from=builder /build/dist/*.whl /tmp/

# Install FraiseQL and production dependencies
RUN pip install --no-cache-dir \
    /tmp/*.whl \
    uvicorn[standard] \
    gunicorn \
    prometheus-client \
    opentelemetry-api \
    opentelemetry-sdk \
    opentelemetry-exporter-otlp \
    opentelemetry-instrumentation-fastapi \
    opentelemetry-instrumentation-psycopg \
    && rm -rf /tmp/*.whl

# Copy entrypoint script
COPY docker/entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/entrypoint.sh

# Create app directory and set permissions
RUN mkdir -p /app && chown -R fraiseql:fraiseql /app

# Switch to non-root user
USER fraiseql

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Set environment variables
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    FRAISEQL_PRODUCTION=true

# Entrypoint
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

# Default command - can be overridden
CMD ["gunicorn", "app:app", "-w", "4", "-k", "uvicorn.workers.UvicornWorker", "--bind", "0.0.0.0:8000"]

# Security scanning recommendation
# To scan this image for vulnerabilities, run:
# docker run --rm -i hadolint/hadolint < Dockerfile
# trivy image fraiseql:latest
