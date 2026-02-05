"""Dockerfile for Fraisier deployment orchestrator.

Multi-stage build for optimal image size and security.
"""

# Stage 1: Builder
FROM python:3.13-slim as builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements
COPY pyproject.toml uv.lock* ./

# Create virtual environment
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install dependencies with uv if available, else pip
RUN if [ -f uv.lock ]; then \
    pip install uv && uv pip install -e .; \
else \
    pip install --upgrade pip && pip install -e .; \
fi

# Stage 2: Runtime
FROM python:3.13-slim

WORKDIR /app

# Install runtime dependencies only
RUN apt-get update && apt-get install -y --no-install-recommends \
    postgresql-client \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv

# Copy application code
COPY fraisier ./fraisier
COPY fraises.example.yaml ./
COPY PRODUCTION_GUIDE.md ./
COPY DEPLOYMENT_EXAMPLES.md ./
COPY TROUBLESHOOTING.md ./

# Set environment
ENV PATH="/opt/venv/bin:$PATH"
ENV PYTHONUNBUFFERED=1
ENV FRAISIER_LOG_LEVEL=INFO
ENV PYTHONDONTWRITEBYTECODE=1

# Create non-root user
RUN useradd -m -u 1000 fraisier && \
    chown -R fraisier:fraisier /app

USER fraisier

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD fraisier status 2>/dev/null || exit 1

# Default command
CMD ["fraisier", "--help"]

# Metadata
LABEL org.opencontainers.image.title="Fraisier" \
      org.opencontainers.image.description="Deployment orchestrator for the FraiseQL ecosystem" \
      org.opencontainers.image.version="0.1.0" \
      org.opencontainers.image.source="https://github.com/fraiseql/fraisier"
