# Optimized Strawberry GraphQL Dockerfile
FROM python:3.11-alpine

WORKDIR /app

# Install system dependencies
RUN apk add --no-cache \
    postgresql-client \
    gcc \
    musl-dev \
    curl \
    libpq-dev \
    python3-dev

# Copy requirements and install Python dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY . .

# Environment variables
ENV DATABASE_URL=postgresql://benchmark:benchmark@postgres-bench:5432/benchmark_db
ENV REDIS_HOST=redis-bench
ENV REDIS_PORT=6379
ENV PYTHONUNBUFFERED=1

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8001/health || exit 1

# Start the Strawberry app
CMD ["python", "optimized_strawberry_app.py"]
