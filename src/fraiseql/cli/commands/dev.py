"""Development server command."""

from pathlib import Path

import click

try:
    from dotenv import load_dotenv
except ImportError:
    load_dotenv = None

try:
    import uvicorn
except ImportError:
    uvicorn = None


@click.command()
@click.option(
    "--host",
    default="127.0.0.1",
    help="Host to bind to",
)
@click.option(
    "--port",
    default=8000,
    type=int,
    help="Port to bind to",
)
@click.option(
    "--reload/--no-reload",
    default=True,
    help="Enable auto-reload on code changes",
)
@click.option(
    "--app",
    default="src.main:app",
    help="Application import path (module:attribute)",
)
def dev(host: str, port: int, reload: bool, app: str) -> None:
    """Start the FraiseQL development server.

    This runs your application with uvicorn, with hot-reloading
    enabled by default for development.
    """
    # Check if we're in a FraiseQL project
    if not Path("pyproject.toml").exists():
        click.echo("Error: Not in a FraiseQL project directory", err=True)
        click.echo("Run 'fraiseql init' to create a new project", err=True)
        msg = "Not in a FraiseQL project directory"
        raise click.ClickException(msg)

    # Load .env file if it exists
    env_file = Path(".env")
    if env_file.exists() and load_dotenv is not None:
        click.echo("📋 Loading environment from .env file")
        load_dotenv(str(env_file))

    click.echo("🚀 Starting FraiseQL development server...")
    click.echo(f"   GraphQL API: http://{host}:{port}/graphql")
    click.echo(f"   Interactive GraphiQL: http://{host}:{port}/graphql")

    if reload:
        click.echo("   Auto-reload: enabled")

    click.echo("\n   Press CTRL+C to stop\n")

    # Check if uvicorn is available
    if uvicorn is None:
        click.echo("Error: uvicorn not installed. Run 'pip install uvicorn'", err=True)
        msg = "uvicorn not installed"
        raise click.ClickException(msg)

    uvicorn.run(
        app,
        host=host,
        port=port,
        reload=reload,
        log_level="info",
    )
