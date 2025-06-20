"""SQL helper CLI commands."""

import importlib.util
import sys

import click

from fraiseql.cli.sql_helper import SQLHelper, SQLPattern, ViewOptions


@click.group()
def sql() -> None:
    """SQL helper commands for generating views and patterns."""


@sql.command()
@click.argument("type_name")
@click.option("--module", "-m", help="Python module containing the type")
@click.option("--table", "-t", help="Custom table name")
@click.option("--view", "-v", help="Custom view name")
@click.option("--exclude", "-e", multiple=True, help="Fields to exclude")
@click.option("--with-comments/--no-comments", default=True, help="Include explanatory comments")
@click.option("--output", "-o", type=click.File("w"), help="Output file (default: stdout)")
def generate_view(type_name, module, table, view, exclude, with_comments, output) -> None:
    """Generate a SQL view for a FraiseQL type.

    Example:
        fraiseql sql generate-view User --module src.types
    """
    # Load the type
    cls = _load_type(type_name, module)

    # Configure options
    options = ViewOptions(
        table_name=table,
        view_name=view,
        excluded_fields=set(exclude),
        include_comments=with_comments,
    )

    # Generate SQL
    helper = SQLHelper()
    sql = helper.generate_view(cls, options)

    # Output
    if output:
        output.write(sql)
        click.echo(f"View SQL written to {output.name}")
    else:
        click.echo(sql)


@sql.command()
@click.argument("type_name")
@click.option("--module", "-m", help="Python module containing the type")
@click.option("--with-table", is_flag=True, help="Include table creation")
@click.option("--with-indexes", is_flag=True, help="Include index creation")
@click.option("--with-data", is_flag=True, help="Include sample data")
@click.option("--output", "-o", type=click.File("w"), help="Output file")
def generate_setup(type_name, module, with_table, with_indexes, with_data, output) -> None:
    """Generate complete SQL setup for a type.

    Example:
        fraiseql sql generate-setup User --with-table --with-indexes
    """
    cls = _load_type(type_name, module)

    helper = SQLHelper()
    sql = helper.generate_setup(
        cls,
        include_table=with_table,
        include_indexes=with_indexes,
        include_sample_data=with_data,
    )

    if output:
        output.write(sql)
        click.echo(f"Setup SQL written to {output.name}")
    else:
        click.echo(sql)


@sql.command()
@click.argument(
    "pattern_type",
    type=click.Choice(["pagination", "filtering", "sorting", "relationship", "aggregation"]),
)
@click.argument("table_name")
@click.option("--limit", default=20, help="Limit for pagination")
@click.option("--offset", default=0, help="Offset for pagination")
@click.option("--where", "-w", multiple=True, help="Filter conditions (field=value)")
@click.option("--order", "-o", multiple=True, help="Order by fields (field:direction)")
@click.option("--child-table", help="Child table for relationship")
@click.option("--foreign-key", help="Foreign key for relationship")
@click.option("--group-by", help="Group by field for aggregation")
def generate_pattern(
    pattern_type,
    table_name,
    limit,
    offset,
    where,
    order,
    child_table,
    foreign_key,
    group_by,
) -> None:
    """Generate common SQL patterns for FraiseQL.

    Examples:
        fraiseql sql generate-pattern pagination users --limit 10
        fraiseql sql generate-pattern filtering users -w email=test@example.com -w is_active=true
        fraiseql sql generate-pattern sorting users -o name:ASC -o created_at:DESC
    """
    if pattern_type == "pagination":
        sql = SQLPattern.pagination(table_name, limit, offset)

    elif pattern_type == "filtering":
        # Parse conditions
        conditions = {}
        for cond in where:
            field, value = cond.split("=", 1)
            # Try to parse value type
            if value.lower() in ("true", "false"):
                conditions[field] = value.lower() == "true"
            elif value.isdigit():
                conditions[field] = int(value)
            else:
                conditions[field] = value

        sql = SQLPattern.filtering(table_name, conditions)

    elif pattern_type == "sorting":
        # Parse order fields
        order_by = []
        for order_spec in order:
            parts = order_spec.split(":")
            field = parts[0]
            direction = parts[1] if len(parts) > 1 else "ASC"
            order_by.append((field, direction))

        sql = SQLPattern.sorting(table_name, order_by)

    elif pattern_type == "relationship":
        if not child_table or not foreign_key:
            click.echo("Error: --child-table and --foreign-key required for relationship pattern")
            return

        sql = SQLPattern.relationship(
            table_name,
            child_table,
            f"{child_table}",  # relationship field name
            foreign_key,
        )

    elif pattern_type == "aggregation":
        if not group_by:
            click.echo("Error: --group-by required for aggregation pattern")
            return

        # Default aggregates
        aggregates = {
            "count": "COUNT(*)",
            "total": "SUM(amount)",
            "average": "AVG(amount)",
        }

        sql = SQLPattern.aggregation(table_name, group_by, aggregates)

    click.echo(sql)


@sql.command()
@click.argument("sql_file", type=click.File("r"))
def validate(sql_file) -> None:
    """Validate SQL for FraiseQL compatibility.

    Example:
        fraiseql sql validate my_view.sql
    """
    sql = sql_file.read()

    helper = SQLHelper()
    result = helper.validate_sql(sql)

    if result.is_valid:
        click.echo(click.style("✓ SQL is valid for FraiseQL", fg="green"))
        if result.has_data_column:
            click.echo(click.style("✓ Has 'data' column", fg="green"))
        if result.returns_jsonb:
            click.echo(click.style("✓ Returns JSONB", fg="green"))
    else:
        click.echo(click.style("✗ SQL has issues:", fg="red"))
        for error in result.errors:
            click.echo(click.style(f"  - {error}", fg="red"))

    if result.warnings:
        click.echo(click.style("\nWarnings:", fg="yellow"))
        for warning in result.warnings:
            click.echo(click.style(f"  - {warning}", fg="yellow"))


@sql.command()
@click.argument("sql_file", type=click.File("r"))
def explain(sql_file) -> None:
    """Explain SQL in beginner-friendly terms.

    Example:
        fraiseql sql explain my_view.sql
    """
    sql = sql_file.read()

    helper = SQLHelper()
    explanation = helper.explain_sql(sql)

    click.echo(click.style("SQL Explanation:", fg="blue", bold=True))
    click.echo(explanation)

    # Also check for common mistakes
    issues = helper.detect_common_mistakes(sql)
    if issues:
        click.echo(click.style("\nPotential Issues:", fg="yellow", bold=True))
        for issue in issues:
            click.echo(click.style(f"  - {issue}", fg="yellow"))


def _load_type(type_name: str, module_path: str | None = None) -> type:
    """Load a type from a module.

    Args:
        type_name: Name of the type to load
        module_path: Module path (e.g., "src.types")

    Returns:
        The loaded type class
    """
    if module_path:
        # Load from specified module
        try:
            module = importlib.import_module(module_path)
            return getattr(module, type_name)
        except (ImportError, AttributeError) as e:
            click.echo(f"Error loading {type_name} from {module_path}: {e}")
            sys.exit(1)
    else:
        # Try to find in common locations
        search_paths = [
            "types",
            "src.types",
            "app.types",
            "models",
            "src.models",
            "app.models",
        ]

        for path in search_paths:
            try:
                module = importlib.import_module(path)
                if hasattr(module, type_name):
                    return getattr(module, type_name)
            except ImportError:
                continue

        click.echo(f"Could not find type {type_name}. Use --module to specify the module.")
        sys.exit(1)
