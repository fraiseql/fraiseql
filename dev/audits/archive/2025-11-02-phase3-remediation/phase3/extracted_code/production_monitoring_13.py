# Extracted from: docs/production/monitoring.md
# Block number: 13


class FraiseQLMetrics:
    """Custom metrics for FraiseQL operations."""

    def __init__(self):
        self.passthrough_queries = Counter(
            "fraiseql_passthrough_queries_total", "Queries using JSON passthrough"
        )

        self.turbo_router_hits = Counter(
            "fraiseql_turbo_router_hits_total", "TurboRouter cache hits"
        )

        self.apq_cache_hits = Counter("fraiseql_apq_cache_hits_total", "APQ cache hits")

        self.mutation_duration = Histogram(
            "fraiseql_mutation_duration_seconds", "Mutation execution time", ["mutation_name"]
        )

    def track_query_execution(self, mode: str, duration: float, complexity: int):
        """Track query execution metrics."""
        if mode == "passthrough":
            self.passthrough_queries.inc()

        graphql_request_duration.labels(operation=mode).observe(duration)
        graphql_query_complexity.observe(complexity)


metrics = FraiseQLMetrics()
