"""Cache monitoring CLI commands (Phase 19, Commit 7)."""

from __future__ import annotations

import click

from fraiseql.monitoring.runtime.cache_monitor_sync import cache_monitor_sync

from .formatters import format_output


@click.group()
def cache() -> None:
    """Cache monitoring commands.

    Monitor cache performance, hit rates, and eviction behavior.

    Examples:
        fraiseql monitoring cache stats
        fraiseql monitoring cache health
    """


@cache.command()
@click.option(
    "--format",
    type=click.Choice(["table", "json"]),
    default="table",
    help="Output format",
)
def stats(format: str) -> None:
    """Show cache statistics.

    Displays cache hit rate, operation counts, evictions, and performance metrics.

    Examples:
        fraiseql monitoring cache stats
        fraiseql monitoring cache stats --format json
    """
    try:
        metrics = cache_monitor_sync.get_metrics_dict()

        if format == "json":
            click.echo(format_output(metrics, format_type="json"))
        else:  # table
            output = (
                f"Cache Statistics\n"
                f"  Hit Rate: {metrics['hit_rate']:.1f}%\n"
                f"  Hits: {metrics['hits']}\n"
                f"  Misses: {metrics['misses']}\n"
                f"  Errors: {metrics['errors']}\n"
                f"  Evictions: {metrics['evictions']}\n"
                f"  Total Operations: {metrics['total_operations']}\n"
                f"  Effective Entries: {metrics['effective_entries']}\n"
                f"  Memory: {metrics['memory_bytes']} bytes\n"
                f"  Error Rate: {metrics['error_rate']:.1f}%"
            )
            click.echo(output)

    except Exception as e:
        click.echo(f"❌ Error fetching cache stats: {e}", err=True)
        raise click.Exit(1)


@cache.command()
@click.option(
    "--format",
    type=click.Choice(["table", "json"]),
    default="table",
    help="Output format",
)
def health(format: str) -> None:
    """Check cache health status.

    Evaluates cache health based on hit rate, eviction rate, and error rate.

    Examples:
        fraiseql monitoring cache health
        fraiseql monitoring cache health --format json
    """
    try:
        is_healthy = cache_monitor_sync.is_healthy()
        metrics = cache_monitor_sync.get_metrics_dict()

        if format == "json":
            data = {
                "status": "healthy" if is_healthy else "degraded",
                "hit_rate": metrics["hit_rate"],
                "evictions": metrics["evictions"],
                "error_rate": metrics["error_rate"],
                "total_operations": metrics["total_operations"],
            }
            click.echo(format_output(data, format_type="json"))
        else:  # table
            status = "HEALTHY ✓" if is_healthy else "DEGRADED ⚠"
            output = (
                f"Cache Health: {status}\n"
                f"  Hit Rate: {metrics['hit_rate']:.1f}% "
                f"{'✓' if metrics['hit_rate'] >= 80 else '⚠'}\n"
                f"  Error Rate: {metrics['error_rate']:.1f}% "
                f"{'✓' if metrics['error_rate'] == 0 else '⚠'}\n"
                f"  Total Operations: {metrics['total_operations']}"
            )
            click.echo(output)

    except Exception as e:
        click.echo(f"❌ Error checking cache health: {e}", err=True)
        raise click.Exit(1)
