"""GraphQL operation monitoring CLI commands (Phase 19, Commit 7)."""

from __future__ import annotations

import click

from fraiseql.monitoring.runtime.operation_monitor_sync import operation_monitor_sync

from .formatters import format_output


@click.group()
def graphql() -> None:
    """GraphQL operation monitoring commands.

    Monitor GraphQL query and mutation performance.

    Examples:
        fraiseql monitoring graphql recent
        fraiseql monitoring graphql stats
        fraiseql monitoring graphql slow
    """


@graphql.command()
@click.option(
    "--limit",
    type=int,
    default=20,
    help="Maximum operations to show",
)
@click.option(
    "--type",
    "operation_type",
    type=click.Choice(["query", "mutation", "subscription"]),
    default=None,
    help="Filter by operation type",
)
@click.option(
    "--format",
    type=click.Choice(["table", "json", "csv"]),
    default="table",
    help="Output format",
)
def recent(limit: int, operation_type: str | None, format: str) -> None:
    """Show recent GraphQL operations.

    Displays the most recent GraphQL operations with performance metrics.

    Examples:
        fraiseql monitoring graphql recent
        fraiseql monitoring graphql recent --limit 50
        fraiseql monitoring graphql recent --type query
    """
    try:
        operations = operation_monitor_sync.get_recent_operations(limit=limit)

        if not operations:
            click.echo("No operations recorded yet")
            return

        # Filter by type if specified
        if operation_type:
            operations = [op for op in operations if op.get("type") == operation_type]

        if not operations:
            click.echo(f"No {operation_type} operations found")
            return

        if format == "json":
            click.echo(format_output(operations, format_type="json"))
        elif format == "csv":
            headers = ["Timestamp", "Type", "Name", "Duration (ms)", "Status"]
            rows = [
                [
                    op.get("timestamp", "-"),
                    op.get("type", "-"),
                    op.get("name", "-"),
                    f"{op.get('duration_ms', 0):.2f}",
                    "✓" if not op.get("error") else "✗",
                ]
                for op in operations
            ]
            click.echo(format_output({}, format_type="csv", headers=headers, rows=rows))
        else:  # table
            headers = ["Timestamp", "Type", "Name", "Duration (ms)", "Status"]
            rows = [
                [
                    op.get("timestamp", "-"),
                    op.get("type", "-"),
                    op.get("name", "-"),
                    f"{op.get('duration_ms', 0):.2f}",
                    "✓" if not op.get("error") else "✗",
                ]
                for op in operations
            ]
            click.echo(format_output({}, format_type="table", headers=headers, rows=rows))

    except Exception as e:
        click.echo(f"❌ Error fetching operations: {e}", err=True)
        raise click.Exit(1)


@graphql.command()
@click.option(
    "--format",
    type=click.Choice(["table", "json"]),
    default="table",
    help="Output format",
)
def stats(format: str) -> None:
    """Show GraphQL operation statistics.

    Displays aggregate statistics for GraphQL operations including
    success rates, operation type breakdown, and performance metrics.

    Examples:
        fraiseql monitoring graphql stats
        fraiseql monitoring graphql stats --format json
    """
    try:
        statistics = operation_monitor_sync.get_statistics()

        if format == "json":
            click.echo(format_output(statistics, format_type="json"))
        else:  # table
            output = (
                f"GraphQL Operation Statistics\n"
                f"  Total Operations: {statistics.get('total_operations', 0)}\n"
                f"  Queries: {statistics.get('queries', 0)}\n"
                f"  Mutations: {statistics.get('mutations', 0)}\n"
                f"  Subscriptions: {statistics.get('subscriptions', 0)}\n"
                f"  Success Rate: {statistics.get('success_rate', 0.0):.1f}%\n"
                f"  Avg Duration: {statistics.get('avg_duration_ms', 0.0):.2f}ms\n"
                f"  Error Rate: {statistics.get('error_rate', 0.0):.1f}%"
            )
            click.echo(output)

    except Exception as e:
        click.echo(f"❌ Error fetching statistics: {e}", err=True)
        raise click.Exit(1)


@graphql.command()
@click.option(
    "--limit",
    type=int,
    default=20,
    help="Maximum operations to show",
)
@click.option(
    "--threshold",
    type=float,
    default=500,
    help="Slow operation threshold in milliseconds",
)
@click.option(
    "--format",
    type=click.Choice(["table", "json", "csv"]),
    default="table",
    help="Output format",
)
def slow(limit: int, threshold: float, format: str) -> None:
    """Show slow GraphQL operations.

    Displays GraphQL operations that exceeded the slow operation threshold.

    Examples:
        fraiseql monitoring graphql slow
        fraiseql monitoring graphql slow --threshold 1000
        fraiseql monitoring graphql slow --format json
    """
    try:
        operations = operation_monitor_sync.get_slow_operations(limit=limit, threshold_ms=threshold)

        if not operations:
            click.echo(f"No operations slower than {threshold}ms")
            return

        if format == "json":
            click.echo(format_output(operations, format_type="json"))
        elif format == "csv":
            headers = ["Timestamp", "Type", "Name", "Duration (ms)", "Error"]
            rows = [
                [
                    op.get("timestamp", "-"),
                    op.get("type", "-"),
                    op.get("name", "-"),
                    f"{op.get('duration_ms', 0):.2f}",
                    op.get("error", "-"),
                ]
                for op in operations
            ]
            click.echo(format_output({}, format_type="csv", headers=headers, rows=rows))
        else:  # table
            headers = ["Timestamp", "Type", "Name", "Duration (ms)", "Error"]
            rows = [
                [
                    op.get("timestamp", "-"),
                    op.get("type", "-"),
                    op.get("name", "-"),
                    f"{op.get('duration_ms', 0):.2f}",
                    op.get("error", "-"),
                ]
                for op in operations
            ]
            click.echo(format_output({}, format_type="table", headers=headers, rows=rows))

    except Exception as e:
        click.echo(f"❌ Error fetching slow operations: {e}", err=True)
        raise click.Exit(1)
