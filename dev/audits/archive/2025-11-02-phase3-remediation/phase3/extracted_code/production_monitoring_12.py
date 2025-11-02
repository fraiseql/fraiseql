# Extracted from: docs/production/monitoring.md
# Block number: 12
from fastapi import FastAPI, Response
from prometheus_client import Counter, Gauge, Histogram, generate_latest

app = FastAPI()

# Metrics
graphql_requests_total = Counter(
    "graphql_requests_total", "Total GraphQL requests", ["operation", "status"]
)

graphql_request_duration = Histogram(
    "graphql_request_duration_seconds",
    "GraphQL request duration",
    ["operation"],
    buckets=[0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
)

graphql_query_complexity = Histogram(
    "graphql_query_complexity",
    "GraphQL query complexity score",
    buckets=[10, 25, 50, 100, 250, 500, 1000],
)

db_pool_connections = Gauge(
    "db_pool_connections",
    "Database pool connections",
    ["state"],  # active, idle
)

cache_hits = Counter("cache_hits_total", "Cache hits")
cache_misses = Counter("cache_misses_total", "Cache misses")


@app.get("/metrics")
async def metrics():
    """Prometheus metrics endpoint."""
    return Response(content=generate_latest(), media_type="text/plain")


# Middleware to track metrics
@app.middleware("http")
async def metrics_middleware(request, call_next):
    import time

    start_time = time.time()

    response = await call_next(request)

    duration = time.time() - start_time

    # Track request duration
    if request.url.path == "/graphql":
        operation = request.headers.get("X-Operation-Name", "unknown")
        status = "success" if response.status_code < 400 else "error"

        graphql_requests_total.labels(operation=operation, status=status).inc()
        graphql_request_duration.labels(operation=operation).observe(duration)

    return response
