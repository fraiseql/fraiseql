"""Webhook handler for event-driven deployments.

Supports any Git provider: GitHub, GitLab, Gitea, Bitbucket, or custom.
"""

import json
import logging
import os
from typing import Any

from fastapi import BackgroundTasks, FastAPI, HTTPException, Request

from .config import get_config
from .git import GitProvider, WebhookEvent, get_provider

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)

app = FastAPI(
    title="Fraisier Webhook",
    description="Receives Git webhooks and triggers fraise deployments",
    version="0.1.0",
)


def get_git_provider() -> GitProvider:
    """Get configured Git provider from environment or config."""
    config = get_config()
    git_config = config._config.get("git", {})

    provider_name = (
        os.getenv("FRAISIER_GIT_PROVIDER") or
        git_config.get("provider", "github")
    )

    provider_config = {
        "webhook_secret": os.getenv("FRAISIER_WEBHOOK_SECRET"),
        "base_url": os.getenv("FRAISIER_GIT_URL"),
        **git_config.get(provider_name, {}),
    }

    return get_provider(provider_name, provider_config)


async def execute_deployment(
    fraise_name: str,
    environment: str,
    fraise_config: dict[str, Any],
    webhook_id: int | None = None,
    git_branch: str | None = None,
    git_commit: str | None = None,
) -> None:
    """Execute deployment in background.

    Args:
        fraise_name: Fraise name (e.g., "my_api")
        environment: Environment (e.g., "production")
        fraise_config: Fraise configuration from fraises.yaml
        webhook_id: ID of webhook event that triggered this
        git_branch: Git branch being deployed
        git_commit: Git commit SHA being deployed
    """
    from .database import get_db

    db = get_db()
    deployment_id = None

    logger.info(f"Starting deployment: {fraise_name} -> {environment}")

    try:
        fraise_type = fraise_config.get("type")

        # Get deployer
        if fraise_type == "api":
            from .deployers.api import APIDeployer
            deployer = APIDeployer(fraise_config)
        elif fraise_type == "etl":
            from .deployers.etl import ETLDeployer
            deployer = ETLDeployer(fraise_config)
        else:
            logger.error(f"Unknown fraise type: {fraise_type}")
            return

        old_version = deployer.get_current_version()

        # Record deployment start
        deployment_id = db.start_deployment(
            fraise=fraise_name,
            environment=environment,
            triggered_by="webhook",
            git_branch=git_branch,
            git_commit=git_commit,
            old_version=old_version,
        )

        # Link webhook event to deployment
        if webhook_id:
            db.link_webhook_to_deployment(webhook_id, deployment_id)

        # Execute deployment
        result = deployer.execute()

        # Record completion
        db.complete_deployment(
            deployment_id=deployment_id,
            success=result.success,
            new_version=result.new_version,
            error_message=result.error_message,
        )

        if result.success:
            # Update fraise state
            db.update_fraise_state(
                fraise=fraise_name,
                environment=environment,
                version=result.new_version or "unknown",
                status="healthy",
                deployed_by="webhook",
            )
            logger.info(
                f"Deployment successful: {fraise_name}/{environment} "
                f"({result.old_version} -> {result.new_version})"
            )
        else:
            logger.error(
                f"Deployment failed: {fraise_name}/{environment} - {result.error_message}"
            )

    except Exception as e:
        logger.exception(f"Deployment error for {fraise_name}/{environment}: {e}")
        if deployment_id:
            db.complete_deployment(
                deployment_id=deployment_id,
                success=False,
                error_message=str(e),
            )


def process_webhook_event(
    event: WebhookEvent,
    background_tasks: BackgroundTasks,
    webhook_id: int,
) -> dict[str, Any]:
    """Process a normalized webhook event.

    Args:
        event: Normalized webhook event
        background_tasks: FastAPI background tasks
        webhook_id: Database ID of recorded event

    Returns:
        Response dict
    """
    config = get_config()

    # Handle push events
    if event.is_push and event.branch:
        logger.info(f"Push to branch: {event.branch} (provider: {event.provider})")

        # Get fraise for this branch
        fraise_config = config.get_fraise_for_branch(event.branch)

        if fraise_config:
            fraise_name = fraise_config["fraise_name"]
            environment = fraise_config["environment"]
            logger.info(f"Triggering deployment: {fraise_name} -> {environment}")

            # Execute deployment in background
            background_tasks.add_task(
                execute_deployment,
                fraise_name=fraise_name,
                environment=environment,
                fraise_config=fraise_config,
                webhook_id=webhook_id,
                git_branch=event.branch,
                git_commit=event.commit_sha,
            )

            return {
                "status": "deployment_triggered",
                "fraise": fraise_name,
                "environment": environment,
                "branch": event.branch,
                "provider": event.provider,
                "webhook_id": webhook_id,
            }
        else:
            logger.info(f"No fraise configured for branch: {event.branch}")
            return {
                "status": "ignored",
                "reason": f"No fraise configured for branch '{event.branch}'",
                "provider": event.provider,
                "webhook_id": webhook_id,
            }

    # Handle ping events
    if event.is_ping:
        return {
            "status": "pong",
            "message": "Webhook configured successfully",
            "provider": event.provider,
            "webhook_id": webhook_id,
        }

    # Ignore other events
    return {
        "status": "ignored",
        "event": event.event_type,
        "provider": event.provider,
        "webhook_id": webhook_id,
    }


@app.post("/webhook")
async def generic_webhook(request: Request, background_tasks: BackgroundTasks) -> dict[str, Any]:
    """Receive webhook from any Git provider.

    The provider is auto-detected from headers, or can be specified
    via query parameter: /webhook?provider=gitlab

    Returns:
        Status of the webhook processing
    """
    from .database import get_db

    # Get raw body for signature verification
    body = await request.body()
    headers = dict(request.headers)

    # Auto-detect provider from headers or use configured default
    provider_name = request.query_params.get("provider")

    if not provider_name:
        # Try to auto-detect from headers
        if "X-GitHub-Event" in headers or "x-github-event" in headers:
            provider_name = "github"
        elif "X-Gitlab-Event" in headers or "x-gitlab-event" in headers:
            provider_name = "gitlab"
        elif "X-Gitea-Event" in headers or "x-gitea-event" in headers:
            provider_name = "gitea"
        elif "X-Event-Key" in headers or "x-event-key" in headers:
            provider_name = "bitbucket"
        else:
            # Fall back to configured default
            config = get_config()
            git_config = config._config.get("git", {})
            provider_name = git_config.get("provider", "github")

    # Get provider and verify signature
    try:
        git_config = get_config()._config.get("git", {})
        provider_config = {
            "webhook_secret": os.getenv("FRAISIER_WEBHOOK_SECRET"),
            **git_config.get(provider_name, {}),
        }
        provider = get_provider(provider_name, provider_config)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))

    # Normalize headers to handle case variations
    normalized_headers = {k.title(): v for k, v in headers.items()}

    # Verify signature
    if not provider.verify_webhook_signature(body, normalized_headers):
        logger.warning(f"Invalid webhook signature from {provider_name}")
        raise HTTPException(status_code=401, detail="Invalid signature")

    # Parse payload
    try:
        payload = await request.json()
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON payload")

    # Parse event
    event = provider.parse_webhook_event(normalized_headers, payload)
    logger.info(f"Received {event.provider} event: {event.event_type}")

    # Record webhook event in database
    db = get_db()
    webhook_id = db.record_webhook_event(
        event_type=f"{event.provider}:{event.event_type}",
        payload=json.dumps(payload),
        branch=event.branch,
        commit_sha=event.commit_sha,
        sender=event.sender,
    )

    # Process the event
    return process_webhook_event(event, background_tasks, webhook_id)


# Legacy endpoint for backward compatibility
@app.post("/webhook/github")
async def github_webhook(request: Request, background_tasks: BackgroundTasks) -> dict[str, Any]:
    """GitHub-specific webhook endpoint (legacy, use /webhook instead)."""
    # Add provider hint to query params
    request._query_params = request.query_params._dict.copy()
    request._query_params["provider"] = "github"
    return await generic_webhook(request, background_tasks)


@app.get("/health")
async def health_check() -> dict[str, str]:
    """Health check endpoint."""
    return {"status": "healthy", "service": "fraisier-webhook"}


@app.get("/fraises")
async def list_fraises() -> dict[str, Any]:
    """List all configured fraises."""
    config = get_config()
    return {
        "fraises": config.list_fraises(),
        "branch_mapping": config.branch_mapping,
    }


@app.get("/providers")
async def list_providers() -> dict[str, Any]:
    """List supported Git providers."""
    from .git import list_providers
    return {
        "providers": list_providers(),
        "configured": get_config()._config.get("git", {}).get("provider", "github"),
    }


def run_server() -> None:
    """Run the webhook server."""
    import uvicorn

    host = os.getenv("FRAISIER_HOST", "0.0.0.0")
    port = int(os.getenv("FRAISIER_PORT", "8080"))

    logger.info(f"Starting Fraisier webhook server on {host}:{port}")

    uvicorn.run(
        "fraisier.webhook:app",
        host=host,
        port=port,
        log_level="info",
    )


if __name__ == "__main__":
    run_server()
