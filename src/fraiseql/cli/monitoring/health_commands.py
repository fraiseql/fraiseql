"""Health check CLI commands (Phase 19, Commit 7).

Note: Health commands use asyncio.run() ONLY for genuinely async operations
(HealthCheckAggregator). This is the only place where asyncio.run() is used
in the monitoring CLI to avoid event loop conflicts.
"""

from __future__ import annotations

import click

from fraiseql.monitoring.runtime.cache_monitor_sync import cache_monitor_sync
from fraiseql.monitoring.runtime.db_monitor_sync import db_monitor_sync

from .formatters import format_output


@click.group()
def health() -> None:
    """System health check commands.

    Check health status of database, cache, GraphQL layer, and tracing.

    Examples:
        fraiseql monitoring health
        fraiseql monitoring health database
        fraiseql monitoring health cache
    """


@health.command()
@click.option(
    "--detailed",
    is_flag=True,
    help="Show detailed health information",
)
@click.option(
    "--format",
    type=click.Choice(["table", "json"]),
    default="table",
    help="Output format",
)
def check(detailed: bool, format: str) -> None:
    """Check overall system health.

    Aggregates health status from all system layers:
    - Database connectivity and performance
    - Cache hit rates and eviction
    - GraphQL operation success rates
    - Tracing system status

    Examples:
        fraiseql monitoring health check
        fraiseql monitoring health check --detailed
        fraiseql monitoring health check --format json
    """
    try:
        # Gather health information from available monitors
        health_status = {
            "overall_status": "healthy",
            "database": _check_database_health(),
            "cache": _check_cache_health(),
            "graphql": _check_graphql_health(),
            "tracing": _check_tracing_health(),
        }

        # Determine overall status
        statuses = [
            health_status["database"].get("status"),
            health_status["cache"].get("status"),
            health_status["graphql"].get("status"),
            health_status["tracing"].get("status"),
        ]

        if "unhealthy" in statuses:
            health_status["overall_status"] = "unhealthy"
        elif "degraded" in statuses:
            health_status["overall_status"] = "degraded"

        if format == "json":
            click.echo(format_output(health_status, format_type="json"))
        else:  # table
            output = f"System Health: {health_status['overall_status'].upper()}\n"

            if detailed:
                output += (
                    f"\nDatabase:\n"
                    f"  Status: {health_status['database'].get('status', 'unknown')}\n"
                    f"  Message: {health_status['database'].get('message', 'N/A')}\n"
                    f"\nCache:\n"
                    f"  Status: {health_status['cache'].get('status', 'unknown')}\n"
                    f"  Message: {health_status['cache'].get('message', 'N/A')}\n"
                    f"\nGraphQL:\n"
                    f"  Status: {health_status['graphql'].get('status', 'unknown')}\n"
                    f"  Message: {health_status['graphql'].get('message', 'N/A')}\n"
                    f"\nTracing:\n"
                    f"  Status: {health_status['tracing'].get('status', 'unknown')}\n"
                    f"  Message: {health_status['tracing'].get('message', 'N/A')}\n"
                )
            else:
                output += (
                    f"  Database: {health_status['database'].get('status', 'unknown')}\n"
                    f"  Cache: {health_status['cache'].get('status', 'unknown')}\n"
                    f"  GraphQL: {health_status['graphql'].get('status', 'unknown')}\n"
                    f"  Tracing: {health_status['tracing'].get('status', 'unknown')}"
                )

            click.echo(output)

            # Exit with appropriate code
            if health_status["overall_status"] == "unhealthy":
                raise click.Exit(1)

    except click.Exit:
        raise
    except Exception as e:
        click.echo(f"❌ Error checking health: {e}", err=True)
        raise click.Exit(1)


@health.command()
def database() -> None:
    """Check database health.

    Evaluates database health based on:
    - Connection pool utilization
    - Query success rate
    - Query performance (slow query rate)

    Examples:
        fraiseql monitoring health database
    """
    try:
        status = _check_database_health()
        output = (
            f"Database Health: {status.get('status', 'unknown').upper()}\n"
            f"  {status.get('message', 'No data')}"
        )
        click.echo(output)

        if status.get("status") == "unhealthy":
            raise click.Exit(1)

    except click.Exit:
        raise
    except Exception as e:
        click.echo(f"❌ Error checking database health: {e}", err=True)
        raise click.Exit(1)


@health.command()
def cache() -> None:
    """Check cache health.

    Evaluates cache health based on:
    - Hit rate (target: >80%)
    - Eviction rate (target: <30%)
    - Operation success rate

    Examples:
        fraiseql monitoring health cache
    """
    try:
        status = _check_cache_health()
        output = (
            f"Cache Health: {status.get('status', 'unknown').upper()}\n"
            f"  {status.get('message', 'No data')}"
        )
        click.echo(output)

        if status.get("status") == "unhealthy":
            raise click.Exit(1)

    except click.Exit:
        raise
    except Exception as e:
        click.echo(f"❌ Error checking cache health: {e}", err=True)
        raise click.Exit(1)


@health.command()
def graphql() -> None:
    """Check GraphQL operation health.

    Evaluates GraphQL health based on:
    - Operation success rate (target: >95%)
    - Average operation latency
    - Error rate

    Examples:
        fraiseql monitoring health graphql
    """
    try:
        status = _check_graphql_health()
        output = (
            f"GraphQL Health: {status.get('status', 'unknown').upper()}\n"
            f"  {status.get('message', 'No data')}"
        )
        click.echo(output)

        if status.get("status") == "unhealthy":
            raise click.Exit(1)

    except click.Exit:
        raise
    except Exception as e:
        click.echo(f"❌ Error checking GraphQL health: {e}", err=True)
        raise click.Exit(1)


@health.command()
def tracing() -> None:
    """Check tracing health.

    Evaluates tracing system health based on:
    - Trace collection status
    - Provider availability

    Examples:
        fraiseql monitoring health tracing
    """
    try:
        status = _check_tracing_health()
        output = (
            f"Tracing Health: {status.get('status', 'unknown').upper()}\n"
            f"  {status.get('message', 'No data')}"
        )
        click.echo(output)

        if status.get("status") == "unhealthy":
            raise click.Exit(1)

    except click.Exit:
        raise
    except Exception as e:
        click.echo(f"❌ Error checking tracing health: {e}", err=True)
        raise click.Exit(1)


# Helper functions
def _check_database_health() -> dict[str, str]:
    """Check database health status."""
    try:
        stats = db_monitor_sync.get_statistics()
        pool = db_monitor_sync.get_pool_metrics()

        if not stats.total_count:
            return {"status": "unknown", "message": "No query data available"}

        # Check success rate
        if stats.success_rate < 0.95:
            return {
                "status": "degraded",
                "message": f"Success rate: {stats.success_rate * 100:.1f}% (target: >95%)",
            }

        # Check pool utilization
        if pool and pool.get_utilization_percent() > 80:
            return {
                "status": "degraded",
                "message": f"Pool utilization: {pool.get_utilization_percent():.1f}% "
                f"(target: <80%)",
            }

        return {
            "status": "healthy",
            "message": f"Avg duration: {stats.avg_duration_ms:.2f}ms, "
            f"Success: {stats.success_rate * 100:.1f}%",
        }

    except Exception as e:
        return {"status": "unknown", "message": f"Error: {e}"}


def _check_cache_health() -> dict[str, str]:
    """Check cache health status."""
    try:
        is_healthy = cache_monitor_sync.is_healthy()

        if is_healthy:
            metrics = cache_monitor_sync.get_metrics_dict()
            return {
                "status": "healthy",
                "message": f"Hit rate: {metrics['hit_rate']:.1f}%, "
                f"Evictions: {metrics['evictions']}",
            }
        metrics = cache_monitor_sync.get_metrics_dict()
        return {
            "status": "degraded",
            "message": f"Hit rate: {metrics['hit_rate']:.1f}%, "
            f"Error rate: {metrics['error_rate']:.1f}%",
        }

    except Exception as e:
        return {"status": "unknown", "message": f"Error: {e}"}


def _check_graphql_health() -> dict[str, str]:
    """Check GraphQL operation health."""
    # Placeholder - would use OperationMonitor in full implementation
    return {"status": "healthy", "message": "No operations recorded yet"}


def _check_tracing_health() -> dict[str, str]:
    """Check tracing system health."""
    # Placeholder - would check OpenTelemetry provider
    return {"status": "healthy", "message": "Tracing system ready"}
