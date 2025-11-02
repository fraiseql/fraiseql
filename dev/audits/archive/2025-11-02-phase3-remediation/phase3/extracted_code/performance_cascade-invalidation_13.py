# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 13
# Track CASCADE metrics
@app.middleware("http")
async def track_cascade_metrics(request, call_next):
    start = time.time()

    response = await call_next(request)

    cascade_time = time.time() - start
    if cascade_time > 0.01:  # >10ms
        logger.warning(f"Slow CASCADE: {cascade_time:.2f}ms")

    return response
