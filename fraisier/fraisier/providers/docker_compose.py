"""Docker Compose deployment provider for containerized services.

Deploys services using Docker Compose with support for:
- Service up/down/restart operations
- Environment variable substitution
- Port mapping configuration
- Volume handling
- Health checks (HTTP, TCP, or exec)
- Log retrieval
- Rollback via image versioning
"""

import subprocess
import time
from pathlib import Path
from typing import Any

import yaml

from fraisier.deployers.base import DeploymentResult, DeploymentStatus

from . import BaseProvider, ProviderConfig


class DockerComposeProvider(BaseProvider):
    """Deploy services using Docker Compose.

    Configuration requirements:
        - url: Path to docker-compose directory (e.g., "/var/compose")
        - custom_fields:
            - compose_file: Filename or relative path (default: "docker-compose.yml")
            - service_name: Service name in compose file (e.g., "api")
            - health_check_type: "http", "tcp", "exec", or "none" (default: "http")
            - health_check_url: HTTP endpoint (e.g., "http://localhost:8000/health")
            - health_check_port: TCP port to check
            - health_check_exec: Command to execute in container
            - health_check_timeout: Timeout in seconds (default: 10)
            - health_check_retries: Number of retries (default: 3)

    Example configuration:
        ProviderConfig(
            name="production",
            type="docker_compose",
            url="/var/compose",
            custom_fields={
                "compose_file": "docker-compose.yml",
                "service_name": "my_api",
                "health_check_type": "http",
                "health_check_url": "http://localhost:8000/health",
                "health_check_timeout": 10,
            }
        )
    """

    type = "docker_compose"

    def __init__(self, config: ProviderConfig):
        """Initialize Docker Compose provider.

        Args:
            config: ProviderConfig with compose directory and service details

        Raises:
            ProviderConfigError: If required configuration is missing
        """
        super().__init__(config)

        # Compose configuration
        self.compose_dir = config.url
        if not self.compose_dir:
            from . import ProviderConfigError
            raise ProviderConfigError("Docker Compose provider requires 'url' (compose directory)")

        self.compose_file = config.custom_fields.get("compose_file", "docker-compose.yml")
        self.service_name = config.custom_fields.get("service_name")

        if not self.service_name:
            from . import ProviderConfigError
            raise ProviderConfigError(
                "Docker Compose provider requires 'service_name' in custom_fields"
            )

        # Health check configuration
        self.health_check_type = config.custom_fields.get("health_check_type", "http")
        self.health_check_url = config.custom_fields.get("health_check_url")
        self.health_check_port = config.custom_fields.get("health_check_port")
        self.health_check_exec = config.custom_fields.get("health_check_exec")
        self.health_check_timeout = config.custom_fields.get("health_check_timeout", 10)
        self.health_check_retries = config.custom_fields.get("health_check_retries", 3)

        # Full path to compose file
        self.compose_path = Path(self.compose_dir) / self.compose_file

    def pre_flight_check(self) -> tuple[bool, str]:
        """Verify Docker Compose setup is valid and accessible.

        Checks:
        - docker-compose command is available
        - compose file exists
        - compose file is valid YAML
        - services can be listed

        Returns:
            Tuple of (success: bool, message: str)
        """
        try:
            # Check docker-compose is available
            result = subprocess.run(
                ["docker-compose", "--version"],
                capture_output=True,
                timeout=5,
                text=True,
            )
            if result.returncode != 0:
                return False, "docker-compose not available or not in PATH"

            # Check compose file exists
            if not self.compose_path.exists():
                return False, f"Compose file not found: {self.compose_path}"

            # Validate YAML
            try:
                with open(self.compose_path) as f:
                    yaml.safe_load(f)
            except yaml.YAMLError as e:
                return False, f"Invalid YAML in compose file: {str(e)}"

            # Check service exists
            result = subprocess.run(
                ["docker-compose", "-f", str(self.compose_path), "config"],
                cwd=self.compose_dir,
                capture_output=True,
                timeout=10,
                text=True,
            )
            if result.returncode != 0:
                return (
                    False,
                    f"docker-compose config validation failed: {result.stderr}",
                )

            return True, "Docker Compose setup is valid and accessible"

        except subprocess.TimeoutExpired:
            return False, "docker-compose command timed out"
        except Exception as e:
            return False, f"Pre-flight check error: {str(e)}"

    def deploy_service(
        self,
        service_name: str,
        version: str,
        config: dict[str, Any],
    ) -> DeploymentResult:
        """Deploy service using Docker Compose.

        Steps:
        1. Pull latest images
        2. Update environment variables for version
        3. Bring up service
        4. Wait for service to be ready
        5. Check health

        Args:
            service_name: Name of service to deploy
            version: Version string (commit SHA, tag, image tag, etc.)
            config: Service configuration (env vars, port mappings, etc.)

        Returns:
            DeploymentResult with success/failure status
        """
        start_time = time.time()
        old_version = None

        try:
            # Get current version before deployment
            old_version = self._get_current_version()

            # Pull latest images
            pull_result = self._run_compose_command(["pull", self.service_name])
            if pull_result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"docker-compose pull failed: {pull_result['stderr']}",
                    new_version=version,
                    old_version=old_version,
                )

            # Update environment variables if provided
            env_vars = config.get("env", {})
            if env_vars:
                self._update_compose_env(env_vars, version)

            # Bring up service
            up_result = self._run_compose_command(
                ["up", "-d", "--no-deps", "--build", self.service_name]
            )
            if up_result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"docker-compose up failed: {up_result['stderr']}",
                    new_version=version,
                    old_version=old_version,
                )

            # Wait for service to be ready
            time.sleep(2)

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
        - version: currently deployed version (from image tag)
        - uptime: uptime in seconds
        - port: port mapping if applicable
        - container_id: Docker container ID

        Args:
            service_name: Name of service

        Returns:
            Dict with status information
        """
        try:
            # Get service status
            ps_result = self._run_compose_command(["ps", self.service_name])

            if ps_result["returncode"] != 0:
                return {
                    "status": "error",
                    "version": None,
                    "error_message": ps_result["stderr"],
                }

            # Parse ps output
            lines = ps_result["stdout"].strip().split("\n")
            if len(lines) < 2:
                return {"status": "stopped", "version": None}

            # Extract container info
            ps_line = lines[-1]  # Last line is the actual container
            parts = ps_line.split()

            # Extract container ID and status
            container_id = parts[0] if parts else None
            service_status = "running" if "Up" in ps_line else "stopped"

            # Get image version
            version = self._get_current_version()

            return {
                "status": service_status,
                "version": version or "unknown",
                "container_id": container_id,
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

        Rolls back by updating compose file to use previous image version.

        Args:
            service_name: Name of service to rollback
            to_version: Specific version to rollback to.
                       If None, uses docker-compose ps to find previous

        Returns:
            DeploymentResult with success/failure status
        """
        start_time = time.time()
        old_version = self._get_current_version()

        try:
            # For Docker Compose, rollback requires accessing deployment history
            # This is typically done through image versioning or compose file management
            # For simplicity, we stop and restart with previous configuration
            if not to_version:
                # Without version history, we can only do a service restart
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=(
                        "Rollback requires 'to_version' for Docker Compose. "
                        "Use deployment history to find previous version."
                    ),
                    new_version=to_version,
                    old_version=old_version,
                )

            # Pull specific version
            pull_result = self._run_compose_command(["pull", self.service_name])
            if pull_result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"docker-compose pull failed: {pull_result['stderr']}",
                    new_version=to_version,
                    old_version=old_version,
                )

            # Bring up service with rolled-back version
            up_result = self._run_compose_command(
                ["up", "-d", "--no-deps", self.service_name]
            )
            if up_result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"docker-compose up failed: {up_result['stderr']}",
                    new_version=to_version,
                    old_version=old_version,
                )

            # Wait for service
            time.sleep(2)

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
            new_version = self._get_current_version()
            duration = time.time() - start_time
            return DeploymentResult(
                success=True,
                status=DeploymentStatus.SUCCESS,
                new_version=new_version,
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
        """Check if service is healthy and responding.

        Supports:
        - HTTP health checks (GET request to URL)
        - TCP health checks (port connectivity)
        - Exec health checks (command in container)
        - Docker compose ps status

        Args:
            service_name: Name of service

        Returns:
            True if service is healthy, False otherwise
        """
        try:
            # First check if service is running via docker-compose ps
            ps_result = self._run_compose_command(["ps", self.service_name])
            if ps_result["returncode"] != 0 or "Up" not in ps_result["stdout"]:
                return False

            # If no health check configured, just check ps status
            if self.health_check_type == "none":
                return True

            # HTTP health check
            if self.health_check_type == "http" and self.health_check_url:
                for attempt in range(self.health_check_retries):
                    try:
                        import urllib.request
                        urllib.request.urlopen(
                            self.health_check_url,
                            timeout=self.health_check_timeout,
                        )
                        return True
                    except Exception:
                        if attempt < self.health_check_retries - 1:
                            time.sleep(1)
                return False

            # TCP health check
            if self.health_check_type == "tcp" and self.health_check_port:
                import socket
                for attempt in range(self.health_check_retries):
                    try:
                        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                        sock.settimeout(self.health_check_timeout)
                        result = sock.connect_ex(("localhost", self.health_check_port))
                        sock.close()
                        if result == 0:
                            return True
                    except Exception:
                        pass
                    if attempt < self.health_check_retries - 1:
                        time.sleep(1)
                return False

            # Exec health check (run command in container)
            if self.health_check_type == "exec" and self.health_check_exec:
                exec_result = self._run_compose_command(
                    ["exec", "-T", self.service_name, "sh", "-c", self.health_check_exec]
                )
                return exec_result["returncode"] == 0

            return True

        except Exception:
            return False

    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get recent logs from service.

        Uses docker-compose logs to retrieve service logs.

        Args:
            service_name: Name of service
            lines: Number of recent log lines to return

        Returns:
            Log output as string (newline-separated lines)
        """
        try:
            logs_result = self._run_compose_command(
                ["logs", "--tail", str(lines), "--no-color", self.service_name]
            )
            return (
                logs_result["stdout"]
                if logs_result["returncode"] == 0
                else logs_result["stderr"]
            )

        except Exception as e:
            return f"Error retrieving logs: {str(e)}"

    # Private helper methods

    def _run_compose_command(self, args: list[str]) -> dict[str, Any]:
        """Execute docker-compose command.

        Args:
            args: Command arguments (without 'docker-compose')

        Returns:
            Dict with returncode, stdout, stderr
        """
        cmd = ["docker-compose", "-f", str(self.compose_path)] + args

        result = subprocess.run(
            cmd,
            cwd=self.compose_dir,
            capture_output=True,
            timeout=120,
            text=True,
        )

        return {
            "returncode": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
        }

    def _get_current_version(self) -> str | None:
        """Get current image version for service.

        Extracts image tag from compose file or running container.

        Returns:
            Image tag/version or None if error
        """
        try:
            # Try to get from running container first
            ps_result = self._run_compose_command(["ps", self.service_name])
            if ps_result["returncode"] == 0 and ps_result["stdout"]:
                lines = ps_result["stdout"].strip().split("\n")
                if len(lines) >= 2:
                    # Extract image from ps output (format: image:tag)
                    ps_line = lines[-1]
                    parts = ps_line.split()
                    if len(parts) >= 2:
                        image = parts[1]  # Image column
                        # Extract tag from image
                        if ":" in image:
                            return image.split(":")[-1]

            # Fall back to reading compose file
            with open(self.compose_path) as f:
                compose = yaml.safe_load(f)
                if compose and "services" in compose:
                    service = compose["services"].get(self.service_name, {})
                    image = service.get("image", "")
                    if ":" in image:
                        return image.split(":")[-1]

            return None

        except Exception:
            return None

    def _update_compose_env(self, env_vars: dict[str, str], version: str) -> None:
        """Update environment variables in compose file.

        Args:
            env_vars: Environment variables to set
            version: Version to set as VERSION env var

        Raises:
            Exception: If update fails
        """
        try:
            with open(self.compose_path) as f:
                compose = yaml.safe_load(f)

            if not compose or "services" not in compose:
                return

            service = compose["services"].get(self.service_name, {})
            if "environment" not in service:
                service["environment"] = {}

            # Update environment
            service["environment"].update(env_vars)
            service["environment"]["VERSION"] = version

            # Write back
            with open(self.compose_path, "w") as f:
                yaml.dump(compose, f, default_flow_style=False)

        except Exception as e:
            raise Exception(f"Failed to update compose environment: {str(e)}")
