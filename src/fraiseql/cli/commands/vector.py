"""Vector database introspection and management CLI commands.

This module provides CLI commands for discovering, validating, and managing
pgvector configurations in your FraiseQL application.

Commands do NOT include embedding generation - use LangChain/LlamaIndex for that.
These are database introspection and management utilities only.
"""

import asyncio
import os
from typing import Any

import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich import box

from fraiseql.db import create_repository

console = Console()


@click.group()
def vector() -> None:
    """Vector database introspection and management commands.

    These commands help you discover, validate, and manage pgvector
    configurations in your database. They do NOT generate embeddings -
    use LangChain or LlamaIndex integrations for that.

    Examples:

        fraiseql vector list
        fraiseql vector inspect tb_documents
        fraiseql vector validate tb_chunks embedding
        fraiseql vector create-index tb_chunks embedding
    """


@vector.command()
@click.option(
    "--database-url",
    envvar="DATABASE_URL",
    required=True,
    help="PostgreSQL database URL",
)
def list(database_url: str) -> None:
    """List all tables with vector fields.

    Scans your database and displays all tables containing vector,
    halfvec, sparsevec, or bit columns.

    Example:

        fraiseql vector list
    """

    async def _list() -> None:
        repo = create_repository(database_url)
        try:
            # Query for all vector columns
            result = await repo.execute(
                """
                SELECT
                    schemaname,
                    tablename,
                    attname as column_name,
                    atttypid::regtype::text as column_type,
                    CASE
                        WHEN atttypid::regtype::text LIKE 'vector%' THEN
                            (SELECT typlen FROM pg_type WHERE oid = atttypid)
                        ELSE NULL
                    END as dimensions
                FROM pg_attribute
                JOIN pg_class ON pg_attribute.attrelid = pg_class.oid
                JOIN pg_namespace ON pg_class.relnamespace = pg_namespace.oid
                WHERE
                    atttypid::regtype::text IN ('vector', 'halfvec', 'sparsevec', 'bit')
                    AND attnum > 0
                    AND NOT attisdropped
                    AND schemaname NOT IN ('pg_catalog', 'information_schema')
                ORDER BY schemaname, tablename, attname;
                """
            )

            if not result:
                console.print(
                    "[yellow]No vector columns found in database.[/yellow]"
                )
                console.print(
                    "\n[dim]Tip: Create vector columns with:[/dim]"
                )
                console.print(
                    "  ALTER TABLE your_table ADD COLUMN embedding vector(1536);"
                )
                return

            # Display results in table
            table = Table(
                title="Vector Columns in Database",
                box=box.ROUNDED,
                show_header=True,
                header_style="bold cyan",
            )
            table.add_column("Schema", style="dim")
            table.add_column("Table", style="cyan")
            table.add_column("Column", style="green")
            table.add_column("Type", style="yellow")
            table.add_column("Dimensions", justify="right")

            for row in result:
                table.add_row(
                    row["schemaname"],
                    row["tablename"],
                    row["column_name"],
                    row["column_type"],
                    str(row.get("dimensions", "N/A")),
                )

            console.print(table)
            console.print(f"\n[green]✓[/green] Found {len(result)} vector columns")

        finally:
            await repo.close()

    asyncio.run(_list())


@vector.command()
@click.argument("table")
@click.option(
    "--database-url",
    envvar="DATABASE_URL",
    required=True,
    help="PostgreSQL database URL",
)
@click.option(
    "--schema",
    default="public",
    help="Database schema (default: public)",
)
def inspect(database_url: str, table: str, schema: str) -> None:
    """Inspect vector configuration for a specific table.

    Shows detailed information about vector fields, indexes, and storage
    for the specified table.

    Example:

        fraiseql vector inspect tb_document_chunks
        fraiseql vector inspect tb_embeddings --schema=custom_schema
    """

    async def _inspect() -> None:
        repo = create_repository(database_url)
        try:
            # Get vector columns
            columns_result = await repo.execute(
                """
                SELECT
                    attname as column_name,
                    atttypid::regtype::text as column_type,
                    pg_get_expr(adbin, adrelid) as default_value,
                    attnotnull as not_null
                FROM pg_attribute
                JOIN pg_class ON pg_attribute.attrelid = pg_class.oid
                JOIN pg_namespace ON pg_class.relnamespace = pg_namespace.oid
                LEFT JOIN pg_attrdef ON adrelid = attrelid AND adnum = attnum
                WHERE
                    pg_namespace.nspname = $1
                    AND pg_class.relname = $2
                    AND atttypid::regtype::text IN ('vector', 'halfvec', 'sparsevec', 'bit')
                    AND attnum > 0
                    AND NOT attisdropped;
                """,
                schema,
                table,
            )

            if not columns_result:
                console.print(
                    f"[yellow]No vector columns found in {schema}.{table}[/yellow]"
                )
                return

            # Display vector columns
            console.print(
                Panel.fit(
                    f"[bold cyan]{schema}.{table}[/bold cyan]",
                    title="Table Inspection",
                    border_style="cyan",
                )
            )

            cols_table = Table(
                title="Vector Columns",
                box=box.SIMPLE,
                show_header=True,
                header_style="bold",
            )
            cols_table.add_column("Column", style="green")
            cols_table.add_column("Type", style="yellow")
            cols_table.add_column("Not Null", justify="center")
            cols_table.add_column("Default")

            for col in columns_result:
                cols_table.add_row(
                    col["column_name"],
                    col["column_type"],
                    "✓" if col["not_null"] else "✗",
                    col.get("default_value") or "[dim]none[/dim]",
                )

            console.print(cols_table)

            # Get indexes
            indexes_result = await repo.execute(
                """
                SELECT
                    i.relname as index_name,
                    a.attname as column_name,
                    am.amname as index_method,
                    pg_size_pretty(pg_relation_size(i.oid)) as index_size,
                    idx.indisvalid as is_valid
                FROM pg_index idx
                JOIN pg_class i ON i.oid = idx.indexrelid
                JOIN pg_class t ON t.oid = idx.indrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
                JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(idx.indkey)
                JOIN pg_am am ON am.oid = i.relam
                WHERE
                    n.nspname = $1
                    AND t.relname = $2
                    AND am.amname IN ('hnsw', 'ivfflat')
                ORDER BY i.relname;
                """,
                schema,
                table,
            )

            if indexes_result:
                console.print()
                idx_table = Table(
                    title="Vector Indexes",
                    box=box.SIMPLE,
                    show_header=True,
                    header_style="bold",
                )
                idx_table.add_column("Index Name", style="cyan")
                idx_table.add_column("Column", style="green")
                idx_table.add_column("Method", style="yellow")
                idx_table.add_column("Size", justify="right")
                idx_table.add_column("Valid", justify="center")

                for idx in indexes_result:
                    idx_table.add_row(
                        idx["index_name"],
                        idx["column_name"],
                        idx["index_method"].upper(),
                        idx["index_size"],
                        "✓" if idx["is_valid"] else "✗",
                    )

                console.print(idx_table)
            else:
                console.print()
                console.print(
                    "[yellow]⚠ No vector indexes found[/yellow]"
                )
                console.print(
                    "[dim]Tip: Create an index with:[/dim]"
                )
                console.print(
                    f"  fraiseql vector create-index {table} {columns_result[0]['column_name']}"
                )

            # Get row count and sample dimensions
            stats_result = await repo.execute(
                f"""
                SELECT
                    COUNT(*) as row_count,
                    COUNT({columns_result[0]['column_name']}) as non_null_vectors
                FROM {schema}.{table};
                """
            )

            if stats_result:
                console.print()
                console.print(
                    f"[bold]Statistics:[/bold] {stats_result[0]['row_count']:,} rows, "
                    f"{stats_result[0]['non_null_vectors']:,} vectors"
                )

        finally:
            await repo.close()

    asyncio.run(_inspect())


@vector.command()
@click.argument("table")
@click.argument("column")
@click.option(
    "--database-url",
    envvar="DATABASE_URL",
    required=True,
    help="PostgreSQL database URL",
)
@click.option(
    "--schema",
    default="public",
    help="Database schema (default: public)",
)
def validate(database_url: str, table: str, column: str, schema: str) -> None:
    """Validate vector column configuration and data.

    Checks for common issues:
    - Dimension consistency
    - NULL values
    - Index existence
    - Performance recommendations

    Example:

        fraiseql vector validate tb_chunks embedding
    """

    async def _validate() -> None:
        repo = create_repository(database_url)
        try:
            issues = []
            warnings = []

            # Check if column exists and is vector type
            col_check = await repo.execute(
                """
                SELECT
                    atttypid::regtype::text as column_type
                FROM pg_attribute
                JOIN pg_class ON pg_attribute.attrelid = pg_class.oid
                JOIN pg_namespace ON pg_class.relnamespace = pg_namespace.oid
                WHERE
                    pg_namespace.nspname = $1
                    AND pg_class.relname = $2
                    AND attname = $3;
                """,
                schema,
                table,
                column,
            )

            if not col_check:
                console.print(
                    f"[red]✗ Column {column} not found in {schema}.{table}[/red]"
                )
                return

            if col_check[0]["column_type"] not in [
                "vector",
                "halfvec",
                "sparsevec",
                "bit",
            ]:
                console.print(
                    f"[red]✗ Column {column} is not a vector type (found: {col_check[0]['column_type']})[/red]"
                )
                return

            console.print(
                Panel.fit(
                    f"[bold cyan]Validating {schema}.{table}.{column}[/bold cyan]",
                    title="Vector Validation",
                    border_style="cyan",
                )
            )

            # Check dimension consistency
            dim_check = await repo.execute(
                f"""
                SELECT
                    array_length({column}, 1) as dimension,
                    COUNT(*) as count
                FROM {schema}.{table}
                WHERE {column} IS NOT NULL
                GROUP BY array_length({column}, 1)
                ORDER BY count DESC;
                """
            )

            if len(dim_check) > 1:
                issues.append(
                    f"Inconsistent dimensions found: {', '.join([f'{d['dimension']}d ({d['count']} rows)' for d in dim_check])}"
                )
            elif dim_check:
                console.print(
                    f"[green]✓[/green] Dimension consistency: {dim_check[0]['dimension']}d"
                )

            # Check for NULLs
            null_check = await repo.execute(
                f"""
                SELECT
                    COUNT(*) FILTER (WHERE {column} IS NULL) as null_count,
                    COUNT(*) as total_count
                FROM {schema}.{table};
                """
            )

            if null_check[0]["null_count"] > 0:
                null_pct = (
                    null_check[0]["null_count"] / null_check[0]["total_count"]
                ) * 100
                warnings.append(
                    f"{null_check[0]['null_count']:,} NULL values ({null_pct:.1f}%)"
                )
            else:
                console.print(f"[green]✓[/green] No NULL values")

            # Check for index
            idx_check = await repo.execute(
                """
                SELECT
                    i.relname as index_name,
                    am.amname as method
                FROM pg_index idx
                JOIN pg_class i ON i.oid = idx.indexrelid
                JOIN pg_class t ON t.oid = idx.indrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
                JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(idx.indkey)
                JOIN pg_am am ON am.oid = i.relam
                WHERE
                    n.nspname = $1
                    AND t.relname = $2
                    AND a.attname = $3
                    AND am.amname IN ('hnsw', 'ivfflat');
                """,
                schema,
                table,
                column,
            )

            if not idx_check:
                issues.append(f"No vector index found on {column}")
                console.print(
                    f"[yellow]✗[/yellow] No vector index (queries will be slow)"
                )
                console.print(
                    f"[dim]  Run: fraiseql vector create-index {table} {column}[/dim]"
                )
            else:
                console.print(
                    f"[green]✓[/green] Index exists: {idx_check[0]['index_name']} ({idx_check[0]['method'].upper()})"
                )

            # Performance recommendations
            if null_check[0]["total_count"] > 100000 and not idx_check:
                warnings.append(
                    f"Large table ({null_check[0]['total_count']:,} rows) without index - consider adding HNSW index"
                )

            # Summary
            console.print()
            if issues:
                console.print("[bold red]Issues:[/bold red]")
                for issue in issues:
                    console.print(f"  [red]✗[/red] {issue}")

            if warnings:
                console.print("[bold yellow]Warnings:[/bold yellow]")
                for warning in warnings:
                    console.print(f"  [yellow]⚠[/yellow] {warning}")

            if not issues and not warnings:
                console.print("[bold green]✓ All checks passed![/bold green]")

        finally:
            await repo.close()

    asyncio.run(_validate())


@vector.command("create-index")
@click.argument("table")
@click.argument("column")
@click.option(
    "--method",
    type=click.Choice(["hnsw", "ivfflat"], case_sensitive=False),
    default="hnsw",
    help="Index method (default: hnsw)",
)
@click.option(
    "--distance",
    type=click.Choice(
        ["cosine", "l2", "inner_product"], case_sensitive=False
    ),
    default="cosine",
    help="Distance metric (default: cosine)",
)
@click.option(
    "--schema",
    default="public",
    help="Database schema (default: public)",
)
@click.option(
    "--execute",
    is_flag=True,
    help="Execute the SQL (default: just print)",
)
@click.option(
    "--database-url",
    envvar="DATABASE_URL",
    help="PostgreSQL database URL (required for --execute)",
)
def create_index(
    table: str,
    column: str,
    method: str,
    distance: str,
    schema: str,
    execute: bool,
    database_url: str | None,
) -> None:
    """Generate SQL for creating a vector index.

    By default, prints the SQL. Use --execute to run it directly.

    Examples:

        # Print SQL for HNSW index with cosine distance
        fraiseql vector create-index tb_chunks embedding

        # Create IVFFlat index with L2 distance
        fraiseql vector create-index tb_chunks embedding \\
            --method ivfflat --distance l2

        # Execute directly
        fraiseql vector create-index tb_chunks embedding --execute
    """
    # Map distance to operator class
    ops_map = {
        "cosine": "vector_cosine_ops",
        "l2": "vector_l2_ops",
        "inner_product": "vector_ip_ops",
    }

    index_name = f"idx_{table}_{column}_{method}"

    if method == "hnsw":
        sql = f"""CREATE INDEX {index_name} ON {schema}.{table}
USING hnsw ({column} {ops_map[distance]})
WITH (m = 16, ef_construction = 64);"""
    else:  # ivfflat
        sql = f"""CREATE INDEX {index_name} ON {schema}.{table}
USING ivfflat ({column} {ops_map[distance]})
WITH (lists = 100);"""

    console.print(Panel.fit(sql, title="Generated SQL", border_style="cyan"))

    if execute:
        if not database_url:
            console.print(
                "[red]Error: --database-url required with --execute[/red]"
            )
            console.print("Set DATABASE_URL environment variable or pass --database-url")
            return

        async def _execute() -> None:
            repo = create_repository(database_url)
            try:
                console.print("\n[yellow]Executing...[/yellow]")
                await repo.execute(sql)
                console.print(f"[green]✓ Index {index_name} created successfully[/green]")
            except Exception as e:
                console.print(f"[red]✗ Error: {e}[/red]")
            finally:
                await repo.close()

        asyncio.run(_execute())
    else:
        console.print(
            "\n[dim]To execute: add --execute flag or run the SQL manually[/dim]"
        )
