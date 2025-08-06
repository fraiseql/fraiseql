"""CLI tool for TurboRouter query registration."""

import json
from pathlib import Path
from typing import List, Optional

import click
from graphql import parse

from fraiseql.fastapi.turbo import TurboRegistry
from fraiseql.turbo.registration import TurboRegistration


@click.group()
def turbo():
    """TurboRouter management commands."""


@turbo.command()
@click.argument("query_file", type=click.Path(exists=True))
@click.option(
    "--view-mapping",
    "-m",
    type=click.Path(exists=True),
    help="JSON file with GraphQL type to view mappings",
)
@click.option("--output", "-o", type=click.Path(), help="Output file for registration results")
@click.option("--dry-run", is_flag=True, help="Validate without registering")
def register(query_file: str, view_mapping: Optional[str], output: Optional[str], dry_run: bool):
    """Register GraphQL queries for TurboRouter optimization.

    QUERY_FILE: Path to file containing GraphQL queries
    """
    # Load queries
    queries = load_queries(query_file)

    # Load view mapping
    mapping = {}
    if view_mapping:
        with Path(view_mapping).open() as f:
            mapping = json.load(f)

    # Create registration system
    registry = TurboRegistry()
    registration = TurboRegistration(registry)

    results = []

    for query_data in queries:
        query_str = query_data.get("query", "")
        operation_name = query_data.get("operationName")

        click.echo(f"Registering query: {operation_name or 'unnamed'}...")

        if dry_run:
            # Just validate
            try:
                parse(query_str)
                click.echo("  ✓ Valid GraphQL")
            except Exception as e:
                click.echo(f"  ✗ Invalid GraphQL: {e}")
                continue
        else:
            # Register
            result = registration.register_from_graphql(query_str, mapping, operation_name)

            if result.success:
                click.echo(f"  ✓ Registered with hash: {result.query_hash}")
            else:
                click.echo(f"  ✗ Failed: {result.error}")

            results.append(
                {
                    "operationName": operation_name,
                    "success": result.success,
                    "hash": result.query_hash,
                    "error": result.error,
                }
            )

    # Save results
    if output and results:
        with Path(output).open("w") as f:
            json.dump(results, f, indent=2)
        click.echo(f"\nResults saved to: {output}")

    # Summary
    successful = sum(1 for r in results if r["success"])
    click.echo(f"\nRegistration complete: {successful}/{len(queries)} successful")


@turbo.command(name="list")
@click.option(
    "--format", "-f", type=click.Choice(["json", "sql"]), default="json", help="Output format"
)
def list_queries(format: str):
    """List registered TurboRouter queries."""
    # This would connect to the database and list queries
    click.echo("Registered queries:")
    # Implementation would query the database


@turbo.command()
@click.argument("query_hash")
def inspect(query_hash: str):
    """Inspect a registered query by hash."""
    click.echo(f"Query details for hash: {query_hash}")
    # Implementation would fetch and display query details


def load_queries(file_path: str) -> List[dict]:
    """Load queries from file."""
    path = Path(file_path)

    if path.suffix == ".graphql":
        # Single query file
        content = path.read_text()
        return [{"query": content}]

    if path.suffix == ".json":
        # JSON file with multiple queries
        with path.open() as f:
            data = json.load(f)

        if isinstance(data, list):
            return data
        if isinstance(data, dict) and "queries" in data:
            return data["queries"]
        return [data]

    raise ValueError(f"Unsupported file format: {path.suffix}")


if __name__ == "__main__":
    turbo()
