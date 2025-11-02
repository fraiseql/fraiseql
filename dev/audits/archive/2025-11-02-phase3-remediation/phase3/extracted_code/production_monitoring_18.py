# Extracted from: docs/production/monitoring.md
# Block number: 18
from fraiseql.monitoring.metrics import query_duration_histogram


@app.middleware("http")
async def query_timing_middleware(request: Request, call_next):
    if request.url.path != "/graphql":
        return await call_next(request)

    import time

    start_time = time.time()

    # Parse query
    body = await request.json()
    query = body.get("query", "")
    operation_name = body.get("operationName", "unknown")

    response = await call_next(request)

    duration = time.time() - start_time

    # Track timing
    query_duration_histogram.labels(operation=operation_name).observe(duration)

    # Log slow queries
    if duration > 1.0:  # Slower than 1 second
        logger.warning(
            "Slow query detected",
            extra={
                "operation": operation_name,
                "duration_ms": duration * 1000,
                "query": query[:500],
            },
        )

    return response
