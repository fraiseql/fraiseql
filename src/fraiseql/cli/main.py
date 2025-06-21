"""FraiseQL CLI main entry point."""

import sys

import click

from .commands import check, dev, generate, init, sql


@click.group()
@click.version_option(version="0.1.0", prog_name="fraiseql")
def cli() -> None:
    """FraiseQL - Lightweight GraphQL-to-PostgreSQL query builder.

    A complete GraphQL API framework that provides strongly-typed
    GraphQL-to-PostgreSQL translation with built-in FastAPI integration.
    """


# Register commands
cli.add_command(init.init)
cli.add_command(dev.dev)
cli.add_command(generate.generate)
cli.add_command(check.check)
cli.add_command(sql.sql)


def main() -> None:
    """Main entry point for the CLI."""
    try:
        cli()
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
