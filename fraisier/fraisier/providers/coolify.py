"""Coolify provider for PaaS deployments.

Handles deployment to Coolify self-hosted PaaS with API integration,
service management, and deployment orchestration.
"""

import asyncio
import json
import logging
import time
from typing import Any, TYPE_CHECKING

from .base import DeploymentProvider, HealthCheck, HealthCheckType, ProviderType
from fraisier.nats.provider import NatsEventProvider

if TYPE_CHECKING:
    from fraisier.nats.client import NatsEventBus

logger = logging.getLogger(__name__)


class CoolifyProvider(DeploymentProvider, NatsEventProvider):
    """Deploy services using Coolify PaaS.

    Supports:
    - Coolify API integration
    - Application deployment and management
    - Service health checks
    - Deployment logs and monitoring
    - Configuration management
    - Rollback capabilities
    """

    def __init__(
        self,
        config: dict[str, Any],
        event_bus: "NatsEventBus | None" = None,
        region: str | None = None,
    ):
        """Initialize Coolify provider.

        Config should include:
            api_url: Coolify API base URL
            api_token: Coolify API authentication token
            application_id: Coolify application ID
            timeout: API request timeout (default 300)

        Args:
            config: Provider configuration
            event_bus: Optional NATS event bus for emitting deployment events
            region: Optional region identifier for multi-region deployments
        """
        super().__init__(config)
        self.api_url = config.get("api_url", "http://localhost:3000/api")
        self.api_token = config.get("api_token")
        self.application_id = config.get("application_id")
        self.timeout = config.get("timeout", 300)
        self.http_client = None

        # NATS event bus for emitting events
        self.event_bus = event_bus
        self.region = region

        if not self.api_token:
            raise ValueError("Coolify provider requires 'api_token' configuration")
        if not self.application_id:
            raise ValueError("Coolify provider requires 'application_id' configuration")

    def _get_provider_type(self) -> ProviderType:
        """Return provider type."""
        return ProviderType.COOLIFY

    async def connect(self) -> None:
        """Verify Coolify API availability.

        Raises:
            ConnectionError: If Coolify API not available
        """
        try:
            # Try to import httpx
            import httpx

            self.http_client = httpx.AsyncClient(
                base_url=self.api_url,
                headers={"Authorization": f"Bearer {self.api_token}"},
                timeout=self.timeout,
            )

            # Test connection by getting application status
            response = await self.http_client.get(
                f"/applications/{self.application_id}",
                headers={"Authorization": f"Bearer {self.api_token}"},
            )

            if response.status_code not in (200, 401, 404):
                raise ConnectionError(f"Coolify API returned status {response.status_code}")

            logger.info(f"Connected to Coolify API at {self.api_url}")

        except ImportError:
            raise ConnectionError(
                "httpx not installed. Install with: pip install httpx"
            )
        except Exception as e:
            raise ConnectionError(f"Failed to connect to Coolify: {e}") from e

    async def disconnect(self) -> None:
        """Close Coolify API connection."""
        if self.http_client:
            await self.http_client.aclose()
            logger.info("Disconnected from Coolify API")

    async def _api_request(
        self,
        method: str,
        endpoint: str,
        **kwargs,
    ) -> dict[str, Any]:
        """Make authenticated request to Coolify API.

        Args:
            method: HTTP method (GET, POST, PUT, DELETE)
            endpoint: API endpoint path
            **kwargs: Additional arguments for request

        Returns:
            Response JSON as dict

        Raises:
            RuntimeError: If request fails
        """
        if not self.http_client:
            raise RuntimeError("Not connected to Coolify API")

        try:
            method_func = getattr(self.http_client, method.lower())
            response = await method_func(endpoint, **kwargs)

            if response.status_code >= 400:
                raise RuntimeError(
                    f"Coolify API error {response.status_code}: {response.text}"
                )

            return response.json() if response.text else {}

        except Exception as e:
            raise RuntimeError(f"Coolify API request failed: {e}") from e

    async def execute_command(
        self,
        command: str,
        timeout: int = 300,
    ) -> tuple[int, str, str]:
        """Execute a command on the Coolify server.

        Args:
            command: Command to execute
            timeout: Command timeout in seconds

        Returns:
            Tuple of (return_code, stdout, stderr)

        Raises:
            RuntimeError: If command execution fails
        """
        # For Coolify, commands are typically executed via container exec
        # This is a placeholder that would be implemented based on deployment
        logger.warning("Direct command execution not supported for Coolify")
        return (1, "", "Command execution not supported for Coolify provider")

    async def upload_file(self, local_path: str, remote_path: str) -> None:
        """Upload file to Coolify environment.

        Args:
            local_path: Local file path
            remote_path: Remote destination path

        Raises:
            FileNotFoundError: If local file doesn't exist
            RuntimeError: If upload fails
        """
        try:
            # For Coolify, file uploads would typically be done via
            # environment variable management or build artifacts
            logger.warning("Direct file upload not implemented for Coolify")
            raise NotImplementedError(
                "Use Coolify's environment/artifact management instead"
            )

        except Exception as e:
            raise RuntimeError(f"File upload failed: {e}") from e

    async def download_file(self, remote_path: str, local_path: str) -> None:
        """Download file from Coolify environment.

        Args:
            remote_path: Remote file path
            local_path: Local destination path

        Raises:
            RuntimeError: If download fails
        """
        try:
            logger.warning("Direct file download not implemented for Coolify")
            raise NotImplementedError(
                "Use Coolify's log retrieval and artifact download instead"
            )

        except Exception as e:
            raise RuntimeError(f"File download failed: {e}") from e

    async def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get Coolify application status.

        Args:
            service_name: Application identifier

        Returns:
            Dict with status information
        """
        try:
            response = await self._api_request(
                "GET",
                f"/applications/{self.application_id}",
            )

            return {
                "service": service_name,
                "active": response.get("status") == "running",
                "state": response.get("status", "unknown"),
                "version": response.get("version", "unknown"),
                "git_branch": response.get("git_branch"),
                "git_commit": response.get("git_commit"),
                "updated_at": response.get("updated_at"),
            }

        except Exception as e:
            return {
                "service": service_name,
                "active": False,
                "error": str(e),
            }

    async def check_health(self, health_check: HealthCheck) -> bool:
        """Check application health.

        Supports HTTP and TCP checks.
        Emits NATS events for health check results.

        Args:
            health_check: Health check configuration

        Returns:
            True if healthy, False otherwise
        """
        import time

        service_name = getattr(health_check, "service", "unknown")
        check_type = health_check.type.value if hasattr(health_check.type, "value") else str(health_check.type)

        # Emit health check started event
        await self.emit_health_check_started(
            service_name=service_name,
            check_type=check_type,
            endpoint=health_check.url or getattr(health_check, "port", None),
        )

        start_time = time.time()

        for attempt in range(health_check.retries):
            try:
                if health_check.type == HealthCheckType.HTTP:
                    result = await self._check_http(health_check)

                elif health_check.type == HealthCheckType.TCP:
                    result = await self._check_tcp(health_check)

                else:
                    logger.warning(f"Unsupported health check type: {health_check.type}")
                    result = False

                if result:
                    # Emit health check passed event
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self.emit_health_check_passed(
                        service_name=service_name,
                        check_type=check_type,
                        duration_ms=duration_ms,
                    )
                    return True

            except Exception as e:
                logger.warning(
                    f"Health check attempt {attempt + 1}/{health_check.retries} failed: {e}"
                )
                if attempt < health_check.retries - 1:
                    await asyncio.sleep(health_check.retry_delay)
                continue

        # Emit health check failed event
        duration_ms = int((time.time() - start_time) * 1000)
        await self.emit_health_check_failed(
            service_name=service_name,
            check_type=check_type,
            reason="Health check failed after all retries",
            duration_ms=duration_ms,
        )

        return False

    async def _check_http(self, health_check: HealthCheck) -> bool:
        """Check HTTP endpoint."""
        if not health_check.url:
            logger.error("HTTP health check requires 'url'")
            return False

        try:
            import httpx

            async with httpx.AsyncClient(timeout=health_check.timeout) as client:
                response = await client.get(health_check.url)
                return response.status_code < 400

        except ImportError:
            logger.error("httpx not installed. Install with: pip install httpx")
            return False
        except Exception as e:
            logger.debug(f"HTTP health check failed: {e}")
            return False

    async def _check_tcp(self, health_check: HealthCheck) -> bool:
        """Check TCP connectivity."""
        if not health_check.port:
            logger.error("TCP health check requires 'port'")
            return False

        try:
            reader, writer = await asyncio.wait_for(
                asyncio.open_connection("127.0.0.1", health_check.port),
                timeout=health_check.timeout,
            )
            writer.close()
            await writer.wait_closed()
            return True

        except asyncio.TimeoutError:
            logger.debug(f"TCP connection timeout on port {health_check.port}")
            return False
        except Exception as e:
            logger.debug(f"TCP health check failed: {e}")
            return False

    async def deploy(self, git_branch: str = "main") -> dict[str, Any]:
        """Trigger a deployment via Coolify.

        Args:
            git_branch: Git branch to deploy from

        Returns:
            Deployment status dict
        """
        try:
            response = await self._api_request(
                "POST",
                f"/applications/{self.application_id}/deploy",
                json={"git_branch": git_branch},
            )

            return {
                "success": True,
                "deployment_id": response.get("deployment_id"),
                "status": response.get("status"),
                "timestamp": time.time(),
            }

        except Exception as e:
            return {
                "success": False,
                "error": str(e),
                "timestamp": time.time(),
            }

    async def get_deployment_logs(self, deployment_id: str) -> str:
        """Get logs for a specific deployment.

        Args:
            deployment_id: Deployment ID

        Returns:
            Deployment logs as string
        """
        try:
            response = await self._api_request(
                "GET",
                f"/applications/{self.application_id}/deployments/{deployment_id}/logs",
            )

            logs = response.get("logs", "")
            return logs if isinstance(logs, str) else json.dumps(logs, indent=2)

        except Exception as e:
            return f"Error retrieving logs: {e}"

    async def get_recent_deployments(self, limit: int = 10) -> list[dict[str, Any]]:
        """Get recent deployments for the application.

        Args:
            limit: Maximum number of deployments to retrieve

        Returns:
            List of deployment records
        """
        try:
            response = await self._api_request(
                "GET",
                f"/applications/{self.application_id}/deployments?limit={limit}",
            )

            deployments = response.get("deployments", [])
            return deployments if isinstance(deployments, list) else []

        except Exception as e:
            logger.error(f"Failed to get deployments: {e}")
            return []

    async def rollback_deployment(
        self,
        deployment_id: str | None = None,
    ) -> dict[str, Any]:
        """Rollback to a previous deployment.

        Args:
            deployment_id: Specific deployment to rollback to, or None for previous

        Returns:
            Rollback status dict
        """
        try:
            endpoint = (
                f"/applications/{self.application_id}/rollback/{deployment_id}"
                if deployment_id
                else f"/applications/{self.application_id}/rollback"
            )

            response = await self._api_request("POST", endpoint)

            return {
                "success": True,
                "deployment_id": response.get("deployment_id"),
                "status": response.get("status"),
                "timestamp": time.time(),
            }

        except Exception as e:
            return {
                "success": False,
                "error": str(e),
                "timestamp": time.time(),
            }

    async def get_application_config(self) -> dict[str, Any]:
        """Get application configuration from Coolify.

        Returns:
            Application configuration dict
        """
        try:
            response = await self._api_request(
                "GET",
                f"/applications/{self.application_id}/config",
            )

            return response

        except Exception as e:
            logger.error(f"Failed to get application config: {e}")
            return {}

    async def update_environment_variables(
        self,
        env_vars: dict[str, str],
    ) -> bool:
        """Update environment variables for the application.

        Args:
            env_vars: Dictionary of environment variables

        Returns:
            True if successful
        """
        try:
            await self._api_request(
                "PUT",
                f"/applications/{self.application_id}/config",
                json={"environment_variables": env_vars},
            )

            logger.info("Updated environment variables")
            return True

        except Exception as e:
            logger.error(f"Failed to update environment variables: {e}")
            return False

    async def get_metrics(self) -> dict[str, Any]:
        """Get application metrics from Coolify.

        Returns:
            Metrics dict with CPU, memory, uptime, etc.
        """
        try:
            response = await self._api_request(
                "GET",
                f"/applications/{self.application_id}/metrics",
            )

            return {
                "cpu_usage": response.get("cpu_usage"),
                "memory_usage": response.get("memory_usage"),
                "uptime": response.get("uptime"),
                "restart_count": response.get("restart_count"),
                "last_deployment": response.get("last_deployment"),
            }

        except Exception as e:
            logger.error(f"Failed to get metrics: {e}")
            return {}

    async def wait_for_deployment(
        self,
        deployment_id: str,
        timeout: int = 3600,
        check_interval: int = 10,
    ) -> bool:
        """Wait for a deployment to complete.

        Args:
            deployment_id: Deployment ID to wait for
            timeout: Maximum wait time in seconds
            check_interval: How often to check status in seconds

        Returns:
            True if deployment succeeded
        """
        start_time = time.time()

        while time.time() - start_time < timeout:
            try:
                response = await self._api_request(
                    "GET",
                    f"/applications/{self.application_id}/deployments/{deployment_id}",
                )

                status = response.get("status")

                if status == "success":
                    logger.info(f"Deployment {deployment_id} succeeded")
                    return True

                elif status == "failed":
                    logger.error(f"Deployment {deployment_id} failed")
                    return False

                elif status in ("running", "queued"):
                    logger.info(f"Deployment {deployment_id} status: {status}")
                    await asyncio.sleep(check_interval)
                    continue

            except Exception as e:
                logger.warning(f"Error checking deployment status: {e}")
                await asyncio.sleep(check_interval)
                continue

        logger.error(f"Deployment {deployment_id} timed out after {timeout}s")
        return False
