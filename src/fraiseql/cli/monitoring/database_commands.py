"""Database monitoring CLI commands (Phase 19, Commit 7).

Provides command-line interface for database monitoring including:
- Recent query analysis
- Slow query detection
- Connection pool status
- Query statistics
"""

from __future__ import annotations

import click

from fraiseql.monitoring.runtime.db_monitor_sync import (
    DatabaseMonitorSync,
    get_database_monitor,
)

from .formatters import format_output


def _get_db_monitor() -> DatabaseMonitorSync:
    """Get or create the database monitor sync instance."""
    return DatabaseMonitorSync(monitor=get_database_monitor())


@click.group()
def database() -> None:
    """Database monitoring commands.

    Analyze database query performance, connection pool status,
    and transaction behavior.

    Examples:
        fraiseql monitoring database recent
        fraiseql monitoring database slow --threshold 100
        fraiseql monitoring database stats
        fraiseql monitoring database pool
    """


@database.command()
@click.option(
    "--limit",
    type=int,
    default=20,
    help="Maximum number of queries to show",
)
@click.option(
    "--format",
    type=click.Choice(["table", "json", "csv"]),
    default="table",
    help="Output format",
)
@click.option(
    "--type",
    "query_type",
    default=None,
    help="Filter by query type (SELECT, INSERT, UPDATE, DELETE)",
)
def recent(limit: int, format: str, query_type: str | None) -> None:
    """Show recent database queries.

    Displays the most recent database queries with timing information.

    Examples:
        fraiseql monitoring database recent
        fraiseql monitoring database recent --limit 50
        fraiseql monitoring database recent --format json
        fraiseql monitoring database recent --type SELECT
    """
    try:
        db_monitor_sync = _get_db_monitor()
        queries = db_monitor_sync.get_recent_queries(limit=limit)

        if not queries:
            click.echo("No queries recorded yet")
            return

        # Filter by type if specified
        if query_type:
            queries = [q for q in queries if q.query_type == query_type.upper()]

        if not queries:
            click.echo(f"No {query_type} queries found")
            return

        # Format output
        if format == "json":
            data = [
                {
                    "timestamp": q.timestamp.isoformat(),
                    "type": q.query_type,
                    "duration_ms": q.duration_ms,
                    "rows_affected": q.rows_affected,
                    "status": "success" if q.is_success() else "failed",
                }
                for q in queries
            ]
            click.echo(format_output(data, format_type="json"))
        elif format == "csv":
            headers = ["Timestamp", "Type", "Duration (ms)", "Rows", "Status"]
            rows = [
                [
                    q.timestamp.isoformat(),
                    q.query_type,
                    f"{q.duration_ms:.2f}",
                    str(q.rows_affected),
                    "✓" if q.is_success() else "✗",
                ]
                for q in queries
            ]
            click.echo(format_output({}, format_type="csv", headers=headers, rows=rows))
        else:  # table
            headers = ["Timestamp", "Type", "Duration (ms)", "Rows", "Status"]
            rows = [
                [
                    q.timestamp.isoformat(),
                    q.query_type,
                    f"{q.duration_ms:.2f}",
                    str(q.rows_affected),
                    "✓" if q.is_success() else "✗",
                ]
                for q in queries
            ]
            click.echo(format_output({}, format_type="table", headers=headers, rows=rows))

    except Exception as e:
        click.echo(f"❌ Error fetching queries: {e}", err=True)
        raise click.Exit(1)


@database.command()
@click.option(
    "--limit",
    type=int,
    default=20,
    help="Maximum number of queries to show",
)
@click.option(
    "--threshold",
    type=float,
    default=100,
    help="Slow query threshold in milliseconds",
)
@click.option(
    "--format",
    type=click.Choice(["table", "json", "csv"]),
    default="table",
    help="Output format",
)
def slow(limit: int, threshold: float, format: str) -> None:
    """Show slow database queries.

    Displays queries that exceeded the slow query threshold
    with detailed performance information.

    Examples:
        fraiseql monitoring database slow
        fraiseql monitoring database slow --threshold 50
        fraiseql monitoring database slow --limit 50 --format json
    """
    try:
        db_monitor_sync = _get_db_monitor()
        queries = db_monitor_sync.get_slow_queries(limit=limit)

        # Filter by threshold
        queries = [q for q in queries if q.duration_ms >= threshold]

        if not queries:
            click.echo(f"No queries slower than {threshold}ms")
            return

        # Format output
        if format == "json":
            data = [
                {
                    "timestamp": q.timestamp.isoformat(),
                    "type": q.query_type,
                    "duration_ms": q.duration_ms,
                    "rows_affected": q.rows_affected,
                    "error": q.error,
                }
                for q in queries
            ]
            click.echo(format_output(data, format_type="json"))
        elif format == "csv":
            headers = ["Timestamp", "Type", "Duration (ms)", "Rows", "Error"]
            rows = [
                [
                    q.timestamp.isoformat(),
                    q.query_type,
                    f"{q.duration_ms:.2f}",
                    str(q.rows_affected),
                    q.error or "-",
                ]
                for q in queries
            ]
            click.echo(format_output({}, format_type="csv", headers=headers, rows=rows))
        else:  # table
            headers = ["Timestamp", "Type", "Duration (ms)", "Rows", "Error"]
            rows = [
                [
                    q.timestamp.isoformat(),
                    q.query_type,
                    f"{q.duration_ms:.2f}",
                    str(q.rows_affected),
                    q.error or "-",
                ]
                for q in queries
            ]
            click.echo(format_output({}, format_type="table", headers=headers, rows=rows))

    except Exception as e:
        click.echo(f"❌ Error fetching slow queries: {e}", err=True)
        raise click.Exit(1)


@database.command()
@click.option(
    "--format",
    type=click.Choice(["table", "json"]),
    default="table",
    help="Output format",
)
def pool(format: str) -> None:
    """Show database connection pool status.

    Displays current connection pool utilization, active/idle connections,
    and wait time statistics.

    Examples:
        fraiseql monitoring database pool
        fraiseql monitoring database pool --format json
    """
    try:
        db_monitor_sync = _get_db_monitor()
        pool_metrics = db_monitor_sync.get_pool_metrics()

        if not pool_metrics:
            click.echo("No pool metrics available")
            return

        if format == "json":
            data = {
                "total_connections": pool_metrics.total_connections,
                "active_connections": pool_metrics.active_connections,
                "idle_connections": pool_metrics.idle_connections,
                "waiting_requests": pool_metrics.waiting_requests,
                "utilization_percent": pool_metrics.get_utilization_percent(),
                "avg_wait_time_ms": pool_metrics.avg_wait_time_ms,
                "max_wait_time_ms": pool_metrics.max_wait_time_ms,
            }
            click.echo(format_output(data, format_type="json"))
        else:  # table
            utilization = pool_metrics.get_utilization_percent()
            output = (
                f"Connection Pool Status\n"
                f"  Total Connections: {pool_metrics.total_connections}\n"
                f"  Active: {pool_metrics.active_connections} "
                f"({utilization:.1f}%)\n"
                f"  Idle: {pool_metrics.idle_connections}\n"
                f"  Waiting: {pool_metrics.waiting_requests}\n"
                f"  Avg Wait Time: {pool_metrics.avg_wait_time_ms:.2f}ms\n"
                f"  Max Wait Time: {pool_metrics.max_wait_time_ms:.2f}ms"
            )
            click.echo(output)

    except Exception as e:
        click.echo(f"❌ Error fetching pool status: {e}", err=True)
        raise click.Exit(1)


@database.command()
@click.option(
    "--format",
    type=click.Choice(["table", "json"]),
    default="table",
    help="Output format",
)
def stats(format: str) -> None:
    """Show database query statistics.

    Displays aggregate statistics for all queries including success rates,
    duration percentiles, and slow query analysis.

    Examples:
        fraiseql monitoring database stats
        fraiseql monitoring database stats --format json
    """
    try:
        db_monitor_sync = _get_db_monitor()
        statistics = db_monitor_sync.get_statistics()

        if format == "json":
            data = {
                "total_count": statistics.total_count,
                "success_count": statistics.success_count,
                "error_count": statistics.error_count,
                "success_rate": round(statistics.success_rate, 4),
                "avg_duration_ms": round(statistics.avg_duration_ms, 2),
                "min_duration_ms": round(statistics.min_duration_ms, 2),
                "max_duration_ms": round(statistics.max_duration_ms, 2),
                "p50_duration_ms": round(statistics.p50_duration_ms, 2),
                "p95_duration_ms": round(statistics.p95_duration_ms, 2),
                "p99_duration_ms": round(statistics.p99_duration_ms, 2),
                "slow_count": statistics.slow_count,
                "slow_rate": round(statistics.slow_rate, 4),
            }
            click.echo(format_output(data, format_type="json"))
        else:  # table
            output = (
                f"Database Query Statistics\n"
                f"  Total Queries: {statistics.total_count}\n"
                f"  Successful: {statistics.success_count} "
                f"({statistics.success_rate * 100:.1f}%)\n"
                f"  Failed: {statistics.error_count}\n"
                f"  Avg Duration: {statistics.avg_duration_ms:.2f}ms\n"
                f"  Min Duration: {statistics.min_duration_ms:.2f}ms\n"
                f"  Max Duration: {statistics.max_duration_ms:.2f}ms\n"
                f"  P50 (Median): {statistics.p50_duration_ms:.2f}ms\n"
                f"  P95 Duration: {statistics.p95_duration_ms:.2f}ms\n"
                f"  P99 Duration: {statistics.p99_duration_ms:.2f}ms\n"
                f"  Slow Queries: {statistics.slow_count} "
                f"({statistics.slow_rate * 100:.1f}%)"
            )
            click.echo(output)

    except Exception as e:
        click.echo(f"❌ Error fetching statistics: {e}", err=True)
        raise click.Exit(1)
