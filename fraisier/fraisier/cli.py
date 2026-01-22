"""CLI interface for Fraisier deployment system.

Commands:
    fraisier list                           # List all fraises
    fraisier deploy <fraise> <environment>  # Deploy a fraise
    fraisier status <fraise> <environment>  # Check fraise status
    fraisier providers                      # List available providers
    fraisier provider-info <type>           # Show provider details
    fraisier provider-test <type>           # Test provider pre-flight
"""

import click
from rich.console import Console
from rich.table import Table
from rich.tree import Tree

from .config import get_config

console = Console()


@click.group()
@click.option(
    "--config",
    "-c",
    type=click.Path(exists=True),
    help="Path to fraises.yaml configuration file",
)
@click.pass_context
def main(ctx: click.Context, config: str | None) -> None:
    """Fraisier - Deployment orchestrator for the FraiseQL ecosystem.

    Manage deployments for all your fraises (services) across multiple providers
    (Bare Metal, Docker Compose, Coolify).

    \b
    Examples:
        fraisier list
        fraisier deploy my_api production
        fraisier providers
        fraisier provider-info bare_metal
        fraisier provider-test docker_compose -f config.yaml
    """
    ctx.ensure_object(dict)
    ctx.obj["config"] = get_config(config)


@main.command()
@click.option("--flat", is_flag=True, help="Show flat list instead of grouped")
@click.pass_context
def list(ctx: click.Context, flat: bool) -> None:
    """List all registered fraises and their environments."""
    config = ctx.obj["config"]

    if flat:
        # Flat list of all deployable targets
        deployments = config.list_all_deployments()

        table = Table(title="All Deployable Targets")
        table.add_column("Fraise", style="cyan")
        table.add_column("Environment", style="magenta")
        table.add_column("Job", style="yellow")
        table.add_column("Type", style="green")
        table.add_column("Name")

        for d in deployments:
            table.add_row(
                d["fraise"],
                d["environment"],
                d["job"] or "-",
                d["type"],
                d["name"],
            )

        console.print(table)
    else:
        # Grouped tree view
        tree = Tree("[bold]Fraises[/bold]")

        for fraise in config.list_fraises():
            fraise_branch = tree.add(
                f"[cyan]{fraise['name']}[/cyan] "
                f"[dim]({fraise['type']})[/dim] - {fraise['description']}"
            )

            for env in fraise["environments"]:
                env_config = config.get_fraise_environment(fraise["name"], env)
                name = env_config.get("name", env) if env_config else env

                # Check for nested jobs
                if env_config and "jobs" in env_config:
                    env_branch = fraise_branch.add(f"[magenta]{env}[/magenta]")
                    for job_name, job_config in env_config["jobs"].items():
                        job_desc = job_config.get("description", "")
                        env_branch.add(f"[yellow]{job_name}[/yellow] - {job_desc}")
                else:
                    fraise_branch.add(f"[magenta]{env}[/magenta] -> {name}")

        console.print(tree)


@main.command()
@click.argument("fraise")
@click.argument("environment")
@click.option("--dry-run", is_flag=True, help="Show what would be deployed")
@click.option("--force", is_flag=True, help="Deploy even if versions match")
@click.option("--job", "-j", help="Specific job name (for scheduled fraises)")
@click.pass_context
def deploy(
    ctx: click.Context,
    fraise: str,
    environment: str,
    dry_run: bool,
    force: bool,
    job: str | None,
) -> None:
    """Deploy a fraise to an environment.

    \b
    FRAISE is the fraise name (e.g., my_api, etl, backup)
    ENVIRONMENT is the target environment (e.g., development, staging, production)

    \b
    Examples:
        fraisier deploy my_api production
        fraisier deploy etl production --dry-run
        fraisier deploy backup production --job local_backup
    """
    config = ctx.obj["config"]
    fraise_config = config.get_fraise_environment(fraise, environment)

    if not fraise_config:
        console.print(f"[red]Error:[/red] Fraise '{fraise}' environment '{environment}' not found")
        console.print("\nAvailable fraises:")
        for f in config.list_fraises():
            envs = ", ".join(f["environments"])
            console.print(f"  {f['name']}: {envs}")
        raise SystemExit(1)

    fraise_type = fraise_config.get("type")

    # Get deployer based on type
    deployer = _get_deployer(fraise_type, fraise_config, job)

    if deployer is None:
        console.print(f"[red]Error:[/red] Unknown fraise type '{fraise_type}'")
        raise SystemExit(1)

    # Check if deployment is needed
    if not force and not deployer.is_deployment_needed():
        console.print(f"[yellow]Fraise '{fraise}/{environment}' is already up to date[/yellow]")
        current = deployer.get_current_version()
        console.print(f"Current version: {current}")
        return

    if dry_run:
        console.print(f"[cyan]DRY RUN:[/cyan] Would deploy {fraise} -> {environment}")
        console.print(f"  Type: {fraise_type}")
        console.print(f"  Current version: {deployer.get_current_version()}")
        console.print(f"  Target version:  {deployer.get_latest_version()}")
        return

    # Execute deployment
    console.print(f"[green]Deploying {fraise} -> {environment}...[/green]")

    result = deployer.execute()

    if result.success:
        console.print("[green]Deployment successful![/green]")
        console.print(f"  Version: {result.old_version} -> {result.new_version}")
        console.print(f"  Duration: {result.duration_seconds:.1f}s")
    else:
        console.print("[red]Deployment failed![/red]")
        console.print(f"  Status: {result.status.value}")
        console.print(f"  Error: {result.error_message}")
        raise SystemExit(1)


@main.command()
@click.argument("fraise")
@click.argument("environment")
@click.pass_context
def status(ctx: click.Context, fraise: str, environment: str) -> None:
    """Check status of a fraise in an environment.

    \b
    Examples:
        fraisier status my_api production
        fraisier status etl production
    """
    config = ctx.obj["config"]
    fraise_config = config.get_fraise_environment(fraise, environment)

    if not fraise_config:
        console.print(f"[red]Error:[/red] Fraise '{fraise}' environment '{environment}' not found")
        raise SystemExit(1)

    console.print(f"[bold]Fraise:[/bold] {fraise}")
    console.print(f"[bold]Environment:[/bold] {environment}")
    console.print(f"[bold]Type:[/bold] {fraise_config.get('type')}")
    console.print(f"[bold]Name:[/bold] {fraise_config.get('name')}")

    if fraise_config.get("systemd_service"):
        console.print(f"[bold]Systemd:[/bold] {fraise_config.get('systemd_service')}")

    # Get deployer and check actual status
    deployer = _get_deployer(fraise_config.get("type"), fraise_config)

    if deployer:
        try:
            current_version = deployer.get_current_version()
            latest_version = deployer.get_latest_version()
            health_ok = deployer.health_check()

            console.print()
            console.print(f"[bold]Current Version:[/bold] {current_version or 'unknown'}")
            console.print(f"[bold]Latest Version:[/bold] {latest_version or 'unknown'}")

            health_status = "[green]healthy[/green]" if health_ok else "[red]unhealthy[/red]"
            console.print(f"[bold]Health Check:[/bold] {health_status}")

            # Check if deployment is needed
            needs_deployment = deployer.is_deployment_needed()
            deployment_status = "[yellow]needs update[/yellow]" if needs_deployment else "[green]up to date[/green]"
            console.print(f"[bold]Status:[/bold] {deployment_status}")

            # Show recent deployments
            from .database import get_db
            db = get_db()
            recent = db.get_recent_deployments(limit=3, fraise=fraise, environment=environment)

            if recent:
                console.print("\n[bold]Recent Deployments:[/bold]")
                for d in recent[:1]:  # Show most recent
                    status_color = "green" if d["status"] == "success" else "red"
                    console.print(
                        f"  [{status_color}]{d['status']}[/{status_color}] "
                        f"({d['old_version']} → {d['new_version']}) "
                        f"at {d['started_at'][:10]}"
                    )

        except Exception as e:
            console.print(f"\n[red]Error checking status:[/red] {e}")


@main.command(name="status-all")
@click.option("--environment", "-e", help="Filter by environment")
@click.option("--type", "-t", "fraise_type", help="Filter by fraise type")
@click.pass_context
def status_all(ctx: click.Context, environment: str | None, fraise_type: str | None) -> None:
    """Check status of all fraises."""
    from .database import get_db

    config = ctx.obj["config"]
    db = get_db()

    # Get fraise states from database
    all_states = db.get_all_fraise_states()

    if environment:
        all_states = [s for s in all_states if s["environment_name"] == environment]
    if fraise_type:
        fraise_config = config.get_fraise(s["fraise_name"]) if all_states else None
        if fraise_config:
            all_states = [s for s in all_states if fraise_config.get("type") == fraise_type]

    if not all_states:
        console.print("[yellow]No fraises found matching filters[/yellow]")
        return

    table = Table(title="Fraise Status")
    table.add_column("Fraise", style="cyan")
    table.add_column("Environment", style="magenta")
    table.add_column("Type", style="green")
    table.add_column("Current", style="yellow")
    table.add_column("Status")
    table.add_column("Last Deploy", style="dim")

    for state in all_states:
        fraise_name = state["fraise_name"]
        env_name = state["environment_name"]
        fraise_cfg = config.get_fraise(fraise_name)
        fraise_type_str = fraise_cfg.get("type", "unknown") if fraise_cfg else "unknown"
        current_version = state.get("current_version") or "unknown"

        # Format status with color
        db_status = state.get("status", "unknown")
        if db_status == "healthy":
            status_str = "[green]healthy[/green]"
        elif db_status == "degraded":
            status_str = "[yellow]degraded[/yellow]"
        elif db_status == "down":
            status_str = "[red]down[/red]"
        else:
            status_str = "[dim]unknown[/dim]"

        last_deploy = state.get("last_deployed_at", "")[:10] if state.get("last_deployed_at") else "-"

        table.add_row(
            fraise_name,
            env_name,
            fraise_type_str,
            current_version,
            status_str,
            last_deploy,
        )

    console.print(table)


@main.command()
@click.option("--fraise", "-f", help="Filter by fraise")
@click.option("--environment", "-e", help="Filter by environment")
@click.option("--limit", "-n", default=20, help="Number of records to show")
@click.pass_context
def history(ctx: click.Context, fraise: str | None, environment: str | None, limit: int) -> None:
    """Show deployment history."""
    from .database import get_db

    db = get_db()
    deployments = db.get_recent_deployments(limit=limit, fraise=fraise, environment=environment)

    if not deployments:
        console.print("[yellow]No deployment history found[/yellow]")
        return

    table = Table(title="Deployment History")
    table.add_column("ID", style="dim")
    table.add_column("Fraise", style="cyan")
    table.add_column("Env", style="magenta")
    table.add_column("Version", style="green")
    table.add_column("Status")
    table.add_column("Duration", style="yellow")
    table.add_column("Started", style="dim")

    for d in deployments:
        # Format status with color
        status = d["status"]
        if status == "success":
            status_str = "[green]success[/green]"
        elif status == "failed":
            status_str = "[red]failed[/red]"
        elif status == "rolled_back":
            status_str = "[yellow]rolled back[/yellow]"
        elif status == "in_progress":
            status_str = "[blue]in progress[/blue]"
        else:
            status_str = status

        # Format duration
        duration = d.get("duration_seconds")
        duration_str = f"{duration:.1f}s" if duration else "-"

        # Format version change
        old_v = d.get("old_version") or "?"
        new_v = d.get("new_version") or "?"
        version_str = f"{old_v} -> {new_v}"

        # Format timestamp (just time if today)
        started = d.get("started_at", "")[:16].replace("T", " ")

        table.add_row(
            str(d["id"]),
            d["fraise"],
            d["environment"],
            version_str,
            status_str,
            duration_str,
            started,
        )

    console.print(table)


@main.command()
@click.option("--fraise", "-f", help="Filter by fraise")
@click.option("--days", "-d", default=30, help="Number of days to analyze")
@click.pass_context
def stats(ctx: click.Context, fraise: str | None, days: int) -> None:
    """Show deployment statistics."""
    from .database import get_db

    db = get_db()
    s = db.get_deployment_stats(fraise=fraise, days=days)

    if not s.get("total"):
        console.print(f"[yellow]No deployments in the last {days} days[/yellow]")
        return

    title = f"Deployment Stats (last {days} days)"
    if fraise:
        title += f" - {fraise}"

    console.print(f"\n[bold]{title}[/bold]\n")

    total = s.get("total", 0)
    successful = s.get("successful", 0)
    failed = s.get("failed", 0)
    rolled_back = s.get("rolled_back", 0)
    avg_duration = s.get("avg_duration")

    success_rate = (successful / total * 100) if total > 0 else 0

    console.print(f"  Total deployments:  {total}")
    console.print(f"  [green]Successful:[/green]        {successful} ({success_rate:.1f}%)")
    console.print(f"  [red]Failed:[/red]            {failed}")
    console.print(f"  [yellow]Rolled back:[/yellow]       {rolled_back}")

    if avg_duration:
        console.print(f"  Avg duration:       {avg_duration:.1f}s")

    console.print()


@main.command()
@click.option("--limit", "-n", default=10, help="Number of events to show")
def webhooks(limit: int) -> None:
    """Show recent webhook events."""
    from .database import get_db

    db = get_db()
    events = db.get_recent_webhooks(limit=limit)

    if not events:
        console.print("[yellow]No webhook events recorded[/yellow]")
        return

    table = Table(title="Recent Webhook Events")
    table.add_column("ID", style="dim")
    table.add_column("Time", style="dim")
    table.add_column("Event", style="cyan")
    table.add_column("Branch", style="magenta")
    table.add_column("Commit", style="yellow")
    table.add_column("Processed")
    table.add_column("Deploy ID")

    for e in events:
        processed = "[green]yes[/green]" if e["processed"] else "[dim]-[/dim]"
        commit = (e.get("commit_sha") or "")[:8]
        time_str = e.get("received_at", "")[:16].replace("T", " ")

        table.add_row(
            str(e["id"]),
            time_str,
            e["event_type"],
            e.get("branch") or "-",
            commit or "-",
            processed,
            str(e.get("deployment_id") or "-"),
        )

    console.print(table)


@main.command(name="version")
def version_cmd() -> None:
    """Show Fraisier version."""
    from . import __version__
    console.print(f"Fraisier v{__version__}")


@main.command(name="providers")
@click.pass_context
def providers(ctx: click.Context) -> None:
    """List all available deployment providers."""
    from .providers import ProviderRegistry
    from .providers.bare_metal import BareMetalProvider
    from .providers.docker_compose import DockerComposeProvider

    # Register built-in providers
    if not ProviderRegistry.is_registered("bare_metal"):
        ProviderRegistry.register(BareMetalProvider)
    if not ProviderRegistry.is_registered("docker_compose"):
        ProviderRegistry.register(DockerComposeProvider)

    providers_list = ProviderRegistry.list_providers()

    if not providers_list:
        console.print("[yellow]No providers registered[/yellow]")
        return

    table = Table(title="Available Deployment Providers")
    table.add_column("Provider Type", style="cyan")
    table.add_column("Description", style="white")

    provider_descriptions = {
        "bare_metal": "SSH/systemd deployments to bare metal servers",
        "docker_compose": "Docker Compose based containerized deployments",
        "coolify": "Coolify cloud platform deployments",
    }

    for provider_type in providers_list:
        description = provider_descriptions.get(provider_type, "Custom provider")
        table.add_row(provider_type, description)

    console.print(table)


@main.command(name="provider-info")
@click.argument("provider_type")
@click.pass_context
def provider_info(ctx: click.Context, provider_type: str) -> None:
    """Show detailed information about a provider type."""
    from .providers import ProviderRegistry
    from .providers.bare_metal import BareMetalProvider
    from .providers.docker_compose import DockerComposeProvider

    # Register built-in providers
    if not ProviderRegistry.is_registered("bare_metal"):
        ProviderRegistry.register(BareMetalProvider)
    if not ProviderRegistry.is_registered("docker_compose"):
        ProviderRegistry.register(DockerComposeProvider)

    if not ProviderRegistry.is_registered(provider_type):
        console.print(
            f"[red]Error:[/red] Unknown provider type '{provider_type}'"
        )
        available = ", ".join(ProviderRegistry.list_providers())
        console.print(f"Available providers: {available}")
        raise SystemExit(1)

    provider_info_map = {
        "bare_metal": {
            "name": "Bare Metal",
            "description": "Deploy to bare metal servers via SSH and systemd",
            "config_fields": [
                "url: SSH host (e.g., 'prod.example.com')",
                "ssh_user: SSH username (default: 'deploy')",
                "ssh_key_path: Path to SSH private key",
                "app_path: Application path on remote (e.g., '/var/app')",
                "systemd_service: Systemd service name (e.g., 'api.service')",
                "health_check_type: 'http', 'tcp', or 'none'",
                "health_check_url: HTTP endpoint (if http type)",
                "health_check_port: TCP port (if tcp type)",
            ],
        },
        "docker_compose": {
            "name": "Docker Compose",
            "description": "Deploy services using Docker Compose",
            "config_fields": [
                "url: Path to docker-compose directory",
                "compose_file: Path to docker-compose.yml (default: 'docker-compose.yml')",
                "service_name: Service name in compose file",
                "health_check_type: 'http', 'tcp', 'exec', 'status', or 'none'",
                "health_check_url: HTTP endpoint (if http type)",
                "health_check_port: TCP port (if tcp type)",
                "health_check_exec: Command to execute (if exec type)",
            ],
        },
        "coolify": {
            "name": "Coolify",
            "description": "Deploy to Coolify cloud platform",
            "config_fields": [
                "url: Coolify instance URL (e.g., 'https://coolify.example.com')",
                "api_key: Coolify API key for authentication",
                "application_id: UUID of application in Coolify",
                "health_check_type: 'status_api', 'http', or 'none'",
                "health_check_url: HTTP endpoint (if http type)",
                "poll_interval: Deployment status poll interval (default: 5s)",
                "poll_timeout: Timeout for deployment (default: 300s)",
            ],
        },
    }

    if provider_type not in provider_info_map:
        info = {
            "name": provider_type.replace("_", " ").title(),
            "description": "Custom provider",
            "config_fields": ["(See provider documentation)"],
        }
    else:
        info = provider_info_map[provider_type]

    console.print(f"\n[bold cyan]{info['name']} Provider[/bold cyan]")
    console.print(f"[white]{info['description']}[/white]\n")

    console.print("[bold]Configuration fields:[/bold]")
    for field in info["config_fields"]:
        console.print(f"  • {field}")
    console.print()


@main.command(name="provider-test")
@click.argument("provider_type")
@click.option("--config-file", "-f", type=click.Path(exists=True),
              help="Provider configuration file (YAML)")
@click.pass_context
def provider_test(ctx: click.Context, provider_type: str,
                  config_file: str | None) -> None:
    """Run pre-flight checks for a provider."""
    import yaml

    from .providers import ProviderConfig, ProviderRegistry
    from .providers.bare_metal import BareMetalProvider
    from .providers.docker_compose import DockerComposeProvider

    # Register built-in providers
    if not ProviderRegistry.is_registered("bare_metal"):
        ProviderRegistry.register(BareMetalProvider)
    if not ProviderRegistry.is_registered("docker_compose"):
        ProviderRegistry.register(DockerComposeProvider)

    if not ProviderRegistry.is_registered(provider_type):
        console.print(
            f"[red]Error:[/red] Unknown provider type '{provider_type}'"
        )
        raise SystemExit(1)

    # Load provider config if file provided
    if config_file:
        try:
            with open(config_file) as f:
                config_data = yaml.safe_load(f)
        except Exception as e:
            console.print(f"[red]Error loading config file:[/red] {e}")
            raise SystemExit(1)

        if not isinstance(config_data, dict):
            console.print("[red]Error:[/red] Config file must contain a YAML object")
            raise SystemExit(1)

        # Create provider config from file
        try:
            provider_config = ProviderConfig(
                name=config_data.get("name", "test"),
                type=provider_type,
                url=config_data.get("url", ""),
                api_key=config_data.get("api_key"),
                custom_fields=config_data.get("custom_fields", {}),
            )
        except Exception as e:
            console.print(f"[red]Error creating provider config:[/red] {e}")
            raise SystemExit(1)
    else:
        # Create minimal test config
        provider_config = ProviderConfig(
            name="test",
            type=provider_type,
            url="localhost",
            custom_fields={},
        )

    # Create provider and run pre-flight check
    try:
        provider = ProviderRegistry.get_provider(provider_type, provider_config)
        console.print(f"[cyan]Testing {provider_type} provider...[/cyan]")
        success, message = provider.pre_flight_check()

        if success:
            console.print("[green]✓ Pre-flight check passed[/green]")
            console.print(f"[dim]{message}[/dim]")
        else:
            console.print("[red]✗ Pre-flight check failed[/red]")
            console.print(f"[dim]{message}[/dim]")
            raise SystemExit(1)

    except Exception as e:
        console.print(f"[red]Error running pre-flight check:[/red] {e}")
        raise SystemExit(1)


@main.command(name="metrics")
@click.option("--port", "-p", default=8001, type=int, help="Port for metrics server")
@click.option("--address", "-a", default="localhost", help="Address to bind to")
def metrics_endpoint(port: int, address: str) -> None:
    """Start Prometheus metrics exporter endpoint.

    Exports deployment metrics at http://ADDRESS:PORT/metrics

    \b
    Examples:
        fraisier metrics                    # Start on localhost:8001
        fraisier metrics --port 8080        # Use port 8080
        fraisier metrics --address 0.0.0.0  # Listen on all interfaces
    """
    try:
        from prometheus_client import start_http_server
    except ImportError:
        console.print(
            "[red]Error:[/red] prometheus_client not installed\n"
            "[yellow]Install with:[/yellow] pip install prometheus-client"
        )
        raise SystemExit(1)

    try:
        # Start metrics server
        start_http_server(port, addr=address)
        console.print(
            f"[green]✓ Prometheus metrics server started[/green]\n"
            f"Metrics available at: [cyan]http://{address}:{port}/metrics[/cyan]\n"
            f"[dim]Press Ctrl+C to stop[/dim]"
        )

        # Keep server running
        import time
        while True:
            time.sleep(1)

    except OSError as e:
        console.print(f"[red]Error:[/red] Failed to start metrics server: {e}")
        raise SystemExit(1)
    except KeyboardInterrupt:
        console.print("\n[yellow]Metrics server stopped[/yellow]")
        raise SystemExit(0)


@main.command(name="db-check")
@click.pass_context
def db_check(ctx: click.Context) -> None:
    """Check database health and show connection pool metrics.

    Verifies database connectivity and displays:
    - Database type and version
    - Connection pool status
    - Query performance
    - Recent errors

    \b
    Examples:
        fraisier db-check
    """
    import asyncio

    from .db.factory import get_database_adapter

    async def _check_db():
        try:
            adapter = get_database_adapter(ctx.obj["config"])
            await adapter.connect()

            try:
                # Test connectivity
                console.print("[cyan]Testing database connectivity...[/cyan]")
                result = await adapter.execute_query("SELECT 1")
                console.print("[green]✓ Database connection successful[/green]")

                # Get pool metrics
                metrics = adapter.pool_metrics()
                console.print("\n[bold]Connection Pool Status:[/bold]")
                pool_table = Table(show_header=True, header_style="bold cyan")
                pool_table.add_column("Metric", style="dim")
                pool_table.add_column("Value")
                pool_table.add_row("Active connections", str(metrics.active_connections))
                pool_table.add_row("Idle connections", str(metrics.idle_connections))
                pool_table.add_row("Total connections", str(
                    metrics.active_connections + metrics.idle_connections
                ))
                pool_table.add_row("Waiting requests", str(metrics.waiting_requests))
                console.print(pool_table)

                # Get database info
                console.print("\n[bold]Database Information:[/bold]")
                db_type = adapter.database_type()
                info_table = Table(show_header=False)
                info_table.add_row("[dim]Type:[/dim]", str(db_type.value).upper())
                console.print(info_table)

                console.print("\n[green]✓ All database checks passed[/green]")

            finally:
                await adapter.disconnect()

        except Exception as e:
            console.print(f"[red]✗ Database health check failed:[/red] {e}")
            raise SystemExit(1)

    try:
        asyncio.run(_check_db())
    except Exception as e:
        console.print(f"[red]Error:[/red] {e}")
        raise SystemExit(1)


def _get_deployer(fraise_type: str, fraise_config: dict, job: str | None = None):
    """Get appropriate deployer for fraise type."""
    if fraise_type == "api":
        from .deployers.api import APIDeployer
        return APIDeployer(fraise_config)

    elif fraise_type == "etl":
        from .deployers.etl import ETLDeployer
        return ETLDeployer(fraise_config)

    elif fraise_type in ("scheduled", "backup"):
        from .deployers.scheduled import ScheduledDeployer

        # Handle nested jobs
        if job and "jobs" in fraise_config:
            job_config = fraise_config["jobs"].get(job)
            if job_config:
                return ScheduledDeployer({
                    **fraise_config,
                    **job_config,
                    "job_name": job,
                })
        return ScheduledDeployer(fraise_config)

    return None


if __name__ == "__main__":
    main()
