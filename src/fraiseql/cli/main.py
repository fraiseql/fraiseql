"""FraiseQL CLI main entry point."""

import sys

import click

from .commands import check, dev, generate, init_command, sql


@click.group()
@click.version_option(version="0.7.12", prog_name="fraiseql")
def cli() -> None:
    """FraiseQL - Production-ready GraphQL API framework for PostgreSQL.

    A comprehensive GraphQL framework with CQRS architecture, type-safe mutations,
    JSONB optimization, and enterprise-grade features like conflict resolution,
    authentication, caching, and FastAPI integration.
    """


# Register commands
cli.add_command(init_command)
cli.add_command(dev)
cli.add_command(generate)
cli.add_command(check)
cli.add_command(sql)


def main() -> None:
    """Main entry point for the CLI."""
    try:
        cli()
    except Exception as e:
        click.echo(f"Error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
