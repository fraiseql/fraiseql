"""CLI interface for Fraisier deployment system.

Commands:
    fraisier list                           # List all fraises
    fraisier deploy <fraise> <environment>  # Deploy a fraise
    fraisier status <fraise> <environment>  # Check fraise status
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

    Manage deployments for all your fraises (services).

    \b
    Examples:
        fraisier list
        fraisier deploy my_api production
        fraisier deploy etl production --dry-run
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

    # TODO: Add actual version/health checking once deployers are complete
    console.print("\n[yellow]Detailed status checking not yet implemented[/yellow]")


@main.command(name="status-all")
@click.option("--environment", "-e", help="Filter by environment")
@click.option("--type", "-t", "fraise_type", help="Filter by fraise type")
@click.pass_context
def status_all(ctx: click.Context, environment: str | None, fraise_type: str | None) -> None:
    """Check status of all fraises."""
    config = ctx.obj["config"]
    deployments = config.list_all_deployments()

    if environment:
        deployments = [d for d in deployments if d["environment"] == environment]
    if fraise_type:
        deployments = [d for d in deployments if d["type"] == fraise_type]

    table = Table(title="Fraise Status")
    table.add_column("Fraise", style="cyan")
    table.add_column("Environment", style="magenta")
    table.add_column("Type", style="green")
    table.add_column("Version", style="yellow")
    table.add_column("Status")

    for d in deployments:
        # TODO: Implement actual status checking
        table.add_row(
            d["fraise"],
            d["environment"],
            d["type"],
            "?.?.?",
            "[yellow]Unknown[/yellow]",
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
