# Extracted from: docs/production/observability.md
# Block number: 10
from prometheus_client import Counter, Histogram, generate_latest

# Define metrics
graphql_requests = Counter(
    "graphql_requests_total", "Total GraphQL requests", ["operation", "status"]
)

graphql_duration = Histogram(
    "graphql_request_duration_seconds", "GraphQL request duration", ["operation"]
)


# Expose metrics endpoint
@app.get("/metrics")
async def metrics_endpoint():
    return Response(content=generate_latest(), media_type="text/plain")
