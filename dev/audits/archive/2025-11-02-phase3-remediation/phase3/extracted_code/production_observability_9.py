# Extracted from: docs/production/observability.md
# Block number: 9
from fraiseql.monitoring import MetricsRecorder

metrics = MetricsRecorder(db_pool)

# Counter
await metrics.increment(
    "graphql.requests.total", labels={"operation": "getUser", "status": "success"}
)

# Gauge
await metrics.set_gauge(
    "db.pool.connections.active",
    value=pool.get_size() - pool.get_idle_size(),
    labels={"pool": "primary"},
)

# Histogram
await metrics.record_histogram(
    "graphql.request.duration_ms", value=duration_ms, labels={"operation": "getOrders"}
)
