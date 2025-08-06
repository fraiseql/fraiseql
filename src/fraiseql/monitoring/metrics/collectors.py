"""Core metrics collectors for FraiseQL.

This module provides the main FraiseQLMetrics class that defines
and manages all Prometheus metrics for the application.
"""

from .config import (
    CollectorRegistry,
    Counter,
    Gauge,
    Histogram,
    MetricsConfig,
    generate_latest,
)


class FraiseQLMetrics:
    """Prometheus metrics collector for FraiseQL.

    This class manages all metrics collection for FraiseQL applications,
    including GraphQL operations, database queries, caching, and errors.

    Attributes:
        config: Metrics configuration.
        registry: Prometheus collector registry.
    """

    def __init__(
        self,
        config: MetricsConfig | None = None,
        registry: CollectorRegistry | None = None,
    ) -> None:
        """Initialize metrics with configuration."""
        self.config = config or MetricsConfig()
        self.registry = registry or CollectorRegistry()
        self._cache_totals: dict[str, dict[str, int]] = {}

        # GraphQL metrics
        self.query_total = Counter(
            f"{self.config.namespace}_graphql_queries_total",
            "Total number of GraphQL queries",
            ["operation_type", "operation_name"],
            registry=self.registry,
        )

        self.query_duration = Histogram(
            f"{self.config.namespace}_graphql_query_duration_seconds",
            "GraphQL query execution time in seconds",
            ["operation_type", "operation_name"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        self.query_success = Counter(
            f"{self.config.namespace}_graphql_queries_success",
            "Number of successful GraphQL queries",
            ["operation_type"],
            registry=self.registry,
        )

        self.query_errors = Counter(
            f"{self.config.namespace}_graphql_queries_errors",
            "Number of failed GraphQL queries",
            ["operation_type"],
            registry=self.registry,
        )

        # Mutation metrics
        self.mutation_total = Counter(
            f"{self.config.namespace}_graphql_mutations_total",
            "Total number of GraphQL mutations",
            ["mutation_name"],
            registry=self.registry,
        )

        self.mutation_success = Counter(
            f"{self.config.namespace}_graphql_mutations_success",
            "Number of successful mutations",
            ["mutation_name", "result_type"],
            registry=self.registry,
        )

        self.mutation_errors = Counter(
            f"{self.config.namespace}_graphql_mutations_errors",
            "Number of failed mutations",
            ["mutation_name", "error_type"],
            registry=self.registry,
        )

        self.mutation_duration = Histogram(
            f"{self.config.namespace}_graphql_mutation_duration_seconds",
            "Mutation execution time in seconds",
            ["mutation_name"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        # Database metrics
        self.db_connections_active = Gauge(
            f"{self.config.namespace}_db_connections_active",
            "Number of active database connections",
            registry=self.registry,
        )

        self.db_connections_idle = Gauge(
            f"{self.config.namespace}_db_connections_idle",
            "Number of idle database connections",
            registry=self.registry,
        )

        self.db_connections_total = Gauge(
            f"{self.config.namespace}_db_connections_total",
            "Total number of database connections",
            registry=self.registry,
        )

        self.db_queries_total = Counter(
            f"{self.config.namespace}_db_queries_total",
            "Total database queries executed",
            ["query_type", "table_name"],
            registry=self.registry,
        )

        self.db_query_duration = Histogram(
            f"{self.config.namespace}_db_query_duration_seconds",
            "Database query execution time",
            ["query_type"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        # Cache metrics
        self.cache_hits = Counter(
            f"{self.config.namespace}_cache_hits_total",
            "Number of cache hits",
            ["cache_type"],
            registry=self.registry,
        )

        self.cache_misses = Counter(
            f"{self.config.namespace}_cache_misses_total",
            "Number of cache misses",
            ["cache_type"],
            registry=self.registry,
        )

        # Error metrics
        self.errors_total = Counter(
            f"{self.config.namespace}_errors_total",
            "Total number of errors",
            ["error_type", "error_code", "operation"],
            registry=self.registry,
        )

        # HTTP metrics
        self.http_requests_total = Counter(
            f"{self.config.namespace}_http_requests_total",
            "Total HTTP requests",
            ["method", "endpoint", "status"],
            registry=self.registry,
        )

        self.http_request_duration = Histogram(
            f"{self.config.namespace}_http_request_duration_seconds",
            "HTTP request duration",
            ["method", "endpoint"],
            buckets=self.config.buckets,
            registry=self.registry,
        )

        # Performance metrics
        self.response_time_histogram = Histogram(
            f"{self.config.namespace}_response_time_seconds",
            "Overall response time distribution",
            buckets=self.config.buckets,
            registry=self.registry,
        )

    def record_query(
        self,
        operation_type: str,
        operation_name: str,
        duration_ms: float,
        success: bool,
    ) -> None:
        """Record a GraphQL query execution."""
        self.query_total.labels(operation_type=operation_type, operation_name=operation_name).inc()
        self.query_duration.labels(
            operation_type=operation_type,
            operation_name=operation_name,
        ).observe(duration_ms / 1000)

        if success:
            self.query_success.labels(operation_type=operation_type).inc()
        else:
            self.query_errors.labels(operation_type=operation_type).inc()

    def record_mutation(
        self,
        mutation_name: str,
        duration_ms: float,
        success: bool,
        result_type: str | None = None,
        error_type: str | None = None,
    ) -> None:
        """Record a GraphQL mutation execution."""
        self.mutation_total.labels(mutation_name=mutation_name).inc()
        self.mutation_duration.labels(mutation_name=mutation_name).observe(duration_ms / 1000)

        if success and result_type:
            self.mutation_success.labels(mutation_name=mutation_name, result_type=result_type).inc()
        elif not success and error_type:
            self.mutation_errors.labels(mutation_name=mutation_name, error_type=error_type).inc()

    def update_db_connections(self, active: int, idle: int, total: int) -> None:
        """Update database connection pool metrics."""
        self.db_connections_active.set(active)
        self.db_connections_idle.set(idle)
        self.db_connections_total.set(total)

    def record_db_query(
        self,
        query_type: str,
        table_name: str,
        duration_ms: float,
        rows_affected: int = 0,
    ) -> None:
        """Record a database query execution."""
        self.db_queries_total.labels(query_type=query_type, table_name=table_name).inc()
        self.db_query_duration.labels(query_type=query_type).observe(duration_ms / 1000)

    def record_cache_hit(self, cache_type: str) -> None:
        """Record a cache hit."""
        self.cache_hits.labels(cache_type=cache_type).inc()

        # Track for hit rate calculation
        if cache_type not in self._cache_totals:
            self._cache_totals[cache_type] = {"hits": 0, "misses": 0}
        self._cache_totals[cache_type]["hits"] += 1

    def record_cache_miss(self, cache_type: str) -> None:
        """Record a cache miss."""
        self.cache_misses.labels(cache_type=cache_type).inc()

        # Track for hit rate calculation
        if cache_type not in self._cache_totals:
            self._cache_totals[cache_type] = {"hits": 0, "misses": 0}
        self._cache_totals[cache_type]["misses"] += 1

    def get_cache_hit_rate(self, cache_type: str) -> float:
        """Calculate cache hit rate for a specific cache type."""
        if cache_type not in self._cache_totals:
            return 0.0

        totals = self._cache_totals[cache_type]
        total = totals["hits"] + totals["misses"]
        if total == 0:
            return 0.0

        return totals["hits"] / total

    def record_error(self, error_type: str, error_code: str, operation: str) -> None:
        """Record an error occurrence."""
        self.errors_total.labels(
            error_type=error_type,
            error_code=error_code,
            operation=operation,
        ).inc()

    def record_response_time(self, duration_ms: float) -> None:
        """Record overall response time."""
        self.response_time_histogram.observe(duration_ms / 1000)

    def generate_metrics(self) -> bytes:
        """Generate Prometheus metrics output."""
        return generate_latest(self.registry)
