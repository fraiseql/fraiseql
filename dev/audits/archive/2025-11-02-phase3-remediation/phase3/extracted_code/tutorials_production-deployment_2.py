# Extracted from: docs/tutorials/production-deployment.md
# Block number: 2
# src/monitoring.py
from prometheus_client import Counter, Gauge, Histogram

# Request metrics
http_requests_total = Counter(
    "http_requests_total", "Total HTTP requests", ["method", "endpoint", "status"]
)

query_duration_seconds = Histogram(
    "graphql_query_duration_seconds", "GraphQL query duration", ["operation"]
)

db_pool_connections = Gauge("db_pool_connections", "Active database connections")


# Middleware
@app.middleware("http")
async def metrics_middleware(request, call_next):
    start_time = time.time()
    response = await call_next(request)
    duration = time.time() - start_time

    query_duration_seconds.labels(operation=request.url.path).observe(duration)

    http_requests_total.labels(
        method=request.method, endpoint=request.url.path, status=response.status_code
    ).inc()

    return response
