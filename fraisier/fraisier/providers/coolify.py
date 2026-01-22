"""Coolify deployment provider for cloud-based deployments.

Deploys services using Coolify platform with support for:
- Project and application management
- Deployment triggering and status polling
- Environment variable management
- Health checks via Coolify
- Log retrieval
- Webhook support
"""

import time
from typing import Any

from fraisier.deployers.base import DeploymentResult, DeploymentStatus

from . import BaseProvider, ProviderConfig
from .coolify_client import CoolifyAPIError, CoolifyClient, CoolifyNotFoundError


class CoolifyProvider(BaseProvider):
    """Deploy services using Coolify cloud platform.

    Configuration requirements:
        - url: Coolify instance URL (e.g., "https://coolify.example.com")
        - api_key: Coolify API key for authentication
        - custom_fields:
            - application_id: UUID of application in Coolify
            - project_id: UUID of project in Coolify (optional, for listing)
            - health_check_type: "status_api", "http", or "none" (default: "status_api")
            - health_check_url: HTTP endpoint for health checks (if http type)
            - health_check_timeout: Timeout in seconds (default: 10)
            - poll_interval: Deployment status poll interval in seconds (default: 5)
            - poll_timeout: Timeout for deployment completion in seconds (default: 300)

    Example configuration:
        ProviderConfig(
            name="production",
            type="coolify",
            url="https://coolify.example.com",
            api_key="coolify_api_key_xyz",
            custom_fields={
                "application_id": "app-uuid-123",
                "project_id": "proj-uuid-456",
                "health_check_type": "status_api",
                "poll_interval": 5,
                "poll_timeout": 300,
            }
        )
    """

    type = "coolify"

    def __init__(self, config: ProviderConfig):
        """Initialize Coolify provider.

        Args:
            config: ProviderConfig with Coolify details

        Raises:
            ProviderConfigError: If required configuration is missing
        """
        super().__init__(config)

        # Coolify configuration
        self.coolify_url = config.url
        self.api_key = config.api_key

        if not self.coolify_url or not self.api_key:
            from . import ProviderConfigError
            raise ProviderConfigError(
                "Coolify provider requires 'url' and 'api_key'"
            )

        self.application_id = config.custom_fields.get("application_id")
        if not self.application_id:
            from . import ProviderConfigError
            raise ProviderConfigError(
                "Coolify provider requires 'application_id' in custom_fields"
            )

        self.project_id = config.custom_fields.get("project_id")

        # Health check configuration
        self.health_check_type = config.custom_fields.get(
            "health_check_type", "status_api"
        )
        self.health_check_url = config.custom_fields.get("health_check_url")
        self.health_check_timeout = config.custom_fields.get(
            "health_check_timeout", 10
        )

        # Polling configuration
        self.poll_interval = config.custom_fields.get("poll_interval", 5)
        self.poll_timeout = config.custom_fields.get("poll_timeout", 300)

        # Initialize Coolify client
        self.client = CoolifyClient(self.coolify_url, self.api_key)

    def pre_flight_check(self) -> tuple[bool, str]:
        """Verify Coolify API is accessible and application exists.

        Checks:
        - Coolify API is reachable
        - API key is valid
        - Application exists and is accessible

        Returns:
            Tuple of (success: bool, message: str)
        """
        try:
            # Check API health
            if not self.client.health_check():
                return False, "Coolify API is not accessible"

            # Check application exists
            app = self.client.get_application(self.application_id)
            app_name = app.get("name", "unknown")

            return True, f"Coolify API accessible, application '{app_name}' found"

        except CoolifyNotFoundError:
            return False, f"Application {self.application_id} not found in Coolify"
        except CoolifyAPIError as e:
            return False, f"Coolify API error: {str(e)}"
        except Exception as e:
            return False, f"Pre-flight check error: {str(e)}"

    def deploy_service(
        self,
        service_name: str,
        version: str,
        config: dict[str, Any],
    ) -> DeploymentResult:
        """Deploy service via Coolify.

        Steps:
        1. Get current application status
        2. Trigger deployment with version
        3. Poll deployment status
        4. Check health

        Args:
            service_name: Name of service to deploy
            version: Version string (image tag, commit SHA, etc.)
            config: Service configuration (env vars, ports, etc.)

        Returns:
            DeploymentResult with success/failure status
        """
        start_time = time.time()
        old_version = None

        try:
            # Get current version
            old_version = self._get_current_version()

            # Trigger deployment
            deploy_payload = {
                "tag": version,
                **(config.get("env", {})),  # Include env vars from config
            }

            deployment = self.client.deploy_application(
                self.application_id, deploy_payload
            )
            deployment_id = deployment.get("id")

            if not deployment_id:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="No deployment ID returned from Coolify",
                    new_version=version,
                    old_version=old_version,
                )

            # Poll deployment status
            deployment_status = self._poll_deployment_status(
                deployment_id, version
            )

            if not deployment_status["success"]:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=deployment_status.get(
                        "error_message", "Deployment failed"
                    ),
                    new_version=version,
                    old_version=old_version,
                )

            # Check health
            if not self.health_check(service_name):
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="Health check failed after deployment",
                    new_version=version,
                    old_version=old_version,
                )

            # Success
            duration = time.time() - start_time
            return DeploymentResult(
                success=True,
                status=DeploymentStatus.SUCCESS,
                new_version=version,
                old_version=old_version,
                duration_seconds=duration,
            )

        except CoolifyAPIError as e:
            duration = time.time() - start_time
            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                error_message=f"Coolify API error: {str(e)}",
                new_version=version,
                old_version=old_version,
                duration_seconds=duration,
            )
        except Exception as e:
            duration = time.time() - start_time
            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                error_message=f"Deployment error: {str(e)}",
                new_version=version,
                old_version=old_version,
                duration_seconds=duration,
            )

    def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get current status of deployed service.

        Returns dict with:
        - status: "running", "stopped", or "error"
        - version: currently deployed version
        - container_id: Container/deployment ID
        - uptime: Uptime in seconds

        Args:
            service_name: Name of service

        Returns:
            Dict with status information
        """
        try:
            status = self.client.get_application_status(self.application_id)

            return {
                "status": status.get("status", "unknown"),
                "version": self._get_current_version(),
                "container_id": status.get("container_id"),
                "uptime": status.get("uptime"),
            }

        except Exception as e:
            return {
                "status": "error",
                "version": None,
                "error_message": str(e),
            }

    def rollback_service(
        self,
        service_name: str,
        to_version: str | None = None,
    ) -> DeploymentResult:
        """Rollback service to previous version.

        Triggers deployment of previous version via Coolify.

        Args:
            service_name: Name of service to rollback
            to_version: Specific version to rollback to.
                       If None, requires deployment history

        Returns:
            DeploymentResult with success/failure status
        """
        start_time = time.time()
        old_version = self._get_current_version()

        try:
            if not to_version:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=(
                        "Rollback requires 'to_version' for Coolify. "
                        "Use deployment history to find previous version."
                    ),
                    new_version=to_version,
                    old_version=old_version,
                )

            # Trigger rollback deployment
            deploy_payload = {"tag": to_version}
            deployment = self.client.deploy_application(
                self.application_id, deploy_payload
            )
            deployment_id = deployment.get("id")

            if not deployment_id:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="No deployment ID returned from Coolify",
                    new_version=to_version,
                    old_version=old_version,
                )

            # Poll deployment status
            deployment_status = self._poll_deployment_status(
                deployment_id, to_version
            )

            if not deployment_status["success"]:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=deployment_status.get(
                        "error_message", "Rollback failed"
                    ),
                    new_version=to_version,
                    old_version=old_version,
                )

            # Check health
            if not self.health_check(service_name):
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="Health check failed after rollback",
                    new_version=to_version,
                    old_version=old_version,
                )

            # Success
            duration = time.time() - start_time
            return DeploymentResult(
                success=True,
                status=DeploymentStatus.SUCCESS,
                new_version=to_version,
                old_version=old_version,
                duration_seconds=duration,
            )

        except CoolifyAPIError as e:
            duration = time.time() - start_time
            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                error_message=f"Coolify API error: {str(e)}",
                new_version=to_version,
                old_version=old_version,
                duration_seconds=duration,
            )
        except Exception as e:
            duration = time.time() - start_time
            return DeploymentResult(
                success=False,
                status=DeploymentStatus.FAILED,
                error_message=f"Rollback error: {str(e)}",
                new_version=to_version,
                old_version=old_version,
                duration_seconds=duration,
            )

    def health_check(self, service_name: str) -> bool:
        """Check if service is healthy.

        Supports:
        - Coolify API status endpoint
        - HTTP health checks
        - Custom health check logic

        Args:
            service_name: Name of service

        Returns:
            True if service is healthy, False otherwise
        """
        try:
            if self.health_check_type == "status_api":
                # Check via Coolify status API
                status = self.client.get_application_status(
                    self.application_id
                )
                return status.get("status") == "running"

            elif self.health_check_type == "http" and self.health_check_url:
                # Check via HTTP endpoint
                import urllib.request
                try:
                    urllib.request.urlopen(
                        self.health_check_url,
                        timeout=self.health_check_timeout,
                    )
                    return True
                except Exception:
                    return False

            elif self.health_check_type == "none":
                return True

            return False

        except Exception:
            return False

    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get recent logs from service.

        Args:
            service_name: Name of service
            lines: Number of recent log lines to return

        Returns:
            Log output as string
        """
        try:
            logs = self.client.get_application_logs(
                self.application_id, lines=lines
            )
            return logs if logs else "(no logs available)"

        except Exception as e:
            return f"Error retrieving logs: {str(e)}"

    # Private helper methods

    def _get_current_version(self) -> str | None:
        """Get current deployed version.

        Returns:
            Version string from deployment or None if error
        """
        try:
            app = self.client.get_application(self.application_id)
            # Try to get tag from application config
            return app.get("tag") or app.get("version")
        except Exception:
            return None

    def _poll_deployment_status(
        self, deployment_id: str, version: str
    ) -> dict[str, Any]:
        """Poll deployment status until completion.

        Args:
            deployment_id: Deployment ID from Coolify
            version: Version being deployed

        Returns:
            Dict with success status and optional error message
        """
        poll_count = 0
        max_polls = self.poll_timeout // self.poll_interval

        while poll_count < max_polls:
            try:
                status = self.client.get_deployment_status(
                    self.application_id, deployment_id
                )

                deployment_status = status.get("status", "pending")

                # Check completion states
                if deployment_status == "completed":
                    return {"success": True}

                if deployment_status == "failed":
                    error_msg = status.get(
                        "error_message", "Deployment failed"
                    )
                    return {"success": False, "error_message": error_msg}

                if deployment_status == "canceled":
                    return {
                        "success": False,
                        "error_message": "Deployment was canceled",
                    }

                # Still in progress, wait and retry
                time.sleep(self.poll_interval)
                poll_count += 1

            except CoolifyAPIError as e:
                return {
                    "success": False,
                    "error_message": f"API error during deployment: {str(e)}",
                }
            except Exception as e:
                return {
                    "success": False,
                    "error_message": f"Error polling deployment: {str(e)}",
                }

        # Timeout reached
        return {
            "success": False,
            "error_message": f"Deployment polling timed out after "
            f"{self.poll_timeout}s",
        }
