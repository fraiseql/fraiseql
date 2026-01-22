"""Prometheus metrics for Fraisier deployments.

Tracks deployment metrics including success rates, durations,
errors, and provider performance.

Note: prometheus_client is optional. If not installed, metrics
are silently ignored.
"""

from typing import Any

# Try to import prometheus_client, fail gracefully if not available
try:
    from prometheus_client import Counter, Gauge, Histogram

    PROMETHEUS_AVAILABLE = True
except ImportError:
    PROMETHEUS_AVAILABLE = False

    # Create dummy classes to prevent errors
    class Counter:  # type: ignore
        def __init__(self, *args, **kwargs):
            pass

        def labels(self, *args, **kwargs):
            return self

        def inc(self, *args, **kwargs):
            pass

    class Gauge:  # type: ignore
        def __init__(self, *args, **kwargs):
            pass

        def labels(self, *args, **kwargs):
            return self

        def set(self, *args, **kwargs):
            pass

        def inc(self, *args, **kwargs):
            pass

        def dec(self, *args, **kwargs):
            pass

    class Histogram:  # type: ignore
        def __init__(self, *args, **kwargs):
            pass

        def labels(self, *args, **kwargs):
            return self

        def observe(self, *args, **kwargs):
            pass


class DeploymentMetrics:
    """Prometheus metrics for Fraisier deployments.

    Tracks:
    - Total deployments by provider, status, and fraise type
    - Deployment errors by type
    - Deployment duration distribution
    - Rollback events
    - Health check performance
    - Active deployments
    """

    # Counters - monotonically increasing values
    deployments_total = Counter(
        "fraisier_deployments_total",
        "Total deployments attempted",
        ["provider", "status", "fraise_type"],
    )

    deployment_errors_total = Counter(
        "fraisier_deployment_errors_total",
        "Total deployment errors",
        ["provider", "error_type"],
    )

    rollbacks_total = Counter(
        "fraisier_rollbacks_total",
        "Total rollbacks performed",
        ["provider", "reason"],
    )

    health_checks_total = Counter(
        "fraisier_health_checks_total",
        "Total health checks performed",
        ["provider", "check_type", "status"],
    )

    # Histograms - distribution of values in buckets
    deployment_duration_seconds = Histogram(
        "fraisier_deployment_duration_seconds",
        "Deployment duration in seconds",
        ["provider", "status"],
        buckets=[5, 10, 30, 60, 120, 300, 600],
    )

    health_check_duration_seconds = Histogram(
        "fraisier_health_check_duration_seconds",
        "Health check duration in seconds",
        ["provider", "check_type"],
        buckets=[0.1, 0.5, 1, 2, 5, 10],
    )

    rollback_duration_seconds = Histogram(
        "fraisier_rollback_duration_seconds",
        "Rollback duration in seconds",
        ["provider"],
        buckets=[5, 10, 30, 60, 120],
    )

    # Gauges - point-in-time values
    active_deployments = Gauge(
        "fraisier_active_deployments",
        "Currently active deployments",
        ["provider"],
    )

    deployment_lock_wait_seconds = Gauge(
        "fraisier_deployment_lock_wait_seconds",
        "Time waiting for deployment lock",
        ["service", "provider"],
    )

    provider_availability = Gauge(
        "fraisier_provider_availability",
        "Provider availability (1=available, 0=unavailable)",
        ["provider"],
    )


class MetricsRecorder:
    """Record metrics for Fraisier operations.

    Usage:
        recorder = MetricsRecorder()
        recorder.record_deployment_start("bare_metal", "api")
        # ... do work ...
        recorder.record_deployment_success("bare_metal", "api", duration=45.2)
    """

    def __init__(self):
        """Initialize metrics recorder."""
        self.metrics = DeploymentMetrics()

    def record_deployment_start(self, provider: str, fraise_type: str) -> None:
        """Record deployment start.

        Args:
            provider: Provider type (bare_metal, docker_compose, coolify)
            fraise_type: Fraise type (api, etl, scheduled)
        """
        # Increment active deployments
        self.metrics.active_deployments.labels(provider=provider).inc()

    def record_deployment_complete(
        self,
        provider: str,
        fraise_type: str,
        status: str,
        duration: float,
    ) -> None:
        """Record deployment completion.

        Args:
            provider: Provider type
            fraise_type: Fraise type
            status: Status (success, failed)
            duration: Duration in seconds
        """
        # Decrement active deployments
        self.metrics.active_deployments.labels(provider=provider).dec()

        # Record total
        self.metrics.deployments_total.labels(
            provider=provider,
            status=status,
            fraise_type=fraise_type,
        ).inc()

        # Record duration
        self.metrics.deployment_duration_seconds.labels(
            provider=provider,
            status=status,
        ).observe(duration)

    def record_deployment_error(
        self,
        provider: str,
        error_type: str,
    ) -> None:
        """Record deployment error.

        Args:
            provider: Provider type
            error_type: Error type (timeout, health_check, config, etc.)
        """
        # Decrement active deployments
        self.metrics.active_deployments.labels(provider=provider).dec()

        # Record error
        self.metrics.deployment_errors_total.labels(
            provider=provider,
            error_type=error_type,
        ).inc()

    def record_rollback(
        self,
        provider: str,
        reason: str,
        duration: float,
    ) -> None:
        """Record rollback event.

        Args:
            provider: Provider type
            reason: Reason for rollback (timeout, health_check, manual, etc.)
            duration: Rollback duration in seconds
        """
        # Record rollback
        self.metrics.rollbacks_total.labels(
            provider=provider,
            reason=reason,
        ).inc()

        # Record duration
        self.metrics.rollback_duration_seconds.labels(
            provider=provider,
        ).observe(duration)

    def record_health_check(
        self,
        provider: str,
        check_type: str,
        status: str,
        duration: float,
    ) -> None:
        """Record health check result.

        Args:
            provider: Provider type
            check_type: Check type (http, tcp, exec, status)
            status: Result status (pass, fail)
            duration: Check duration in seconds
        """
        # Record check
        self.metrics.health_checks_total.labels(
            provider=provider,
            check_type=check_type,
            status=status,
        ).inc()

        # Record duration
        self.metrics.health_check_duration_seconds.labels(
            provider=provider,
            check_type=check_type,
        ).observe(duration)

    def set_provider_availability(self, provider: str, available: bool) -> None:
        """Set provider availability status.

        Args:
            provider: Provider type
            available: Whether provider is available
        """
        self.metrics.provider_availability.labels(
            provider=provider,
        ).set(1 if available else 0)

    def record_lock_wait(
        self,
        service: str,
        provider: str,
        wait_time: float,
    ) -> None:
        """Record deployment lock wait time.

        Args:
            service: Service name
            provider: Provider type
            wait_time: Wait time in seconds
        """
        self.metrics.deployment_lock_wait_seconds.labels(
            service=service,
            provider=provider,
        ).set(wait_time)

    def get_metrics_summary(self) -> dict[str, Any]:
        """Get summary of current metrics.

        Returns:
            Dict with metric summaries
        """
        return {
            "prometheus_available": PROMETHEUS_AVAILABLE,
            "message": (
                "Metrics are being recorded. Export with Prometheus exporter endpoint."
                if PROMETHEUS_AVAILABLE
                else "Prometheus not installed. Install with: pip install prometheus-client"
            ),
        }


# Global metrics recorder instance
_metrics_recorder: MetricsRecorder | None = None


def get_metrics_recorder() -> MetricsRecorder:
    """Get or create global metrics recorder.

    Returns:
        MetricsRecorder instance
    """
    global _metrics_recorder
    if _metrics_recorder is None:
        _metrics_recorder = MetricsRecorder()
    return _metrics_recorder
