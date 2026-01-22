"""Bare Metal deployment provider for SSH/systemd deployments.

Deploys to bare metal servers using SSH connections and systemd service management.
Supports:
- SSH key-based authentication
- Systemd service restart
- Git operations on remote servers
- Health checks via TCP or HTTP
- Log retrieval via journalctl
- Rollback via git checkout
"""

import subprocess
import time
from typing import Any

from fraisier.deployers.base import DeploymentResult, DeploymentStatus

from . import BaseProvider, ProviderConfig


class BareMetalProvider(BaseProvider):
    """Deploy to bare metal servers via SSH and systemd.

    Configuration requirements:
        - url: SSH host (e.g., "prod.example.com")
        - custom_fields:
            - ssh_user: SSH username (default: "deploy")
            - ssh_key_path: Path to SSH private key (default: "~/.ssh/id_rsa")
            - app_path: Application path on remote (e.g., "/var/app")
            - systemd_service: Systemd service name (e.g., "my_api.service")
            - health_check_type: "http", "tcp", or "none" (default: "http")
            - health_check_url: HTTP endpoint to check (e.g., "http://localhost:8000/health")
            - health_check_port: TCP port to check if tcp type
            - health_check_timeout: Timeout in seconds (default: 10)

    Example configuration:
        ProviderConfig(
            name="production",
            type="bare_metal",
            url="prod.example.com",
            custom_fields={
                "ssh_user": "deploy",
                "ssh_key_path": "/etc/fraisier/keys/prod.pem",
                "app_path": "/var/app",
                "systemd_service": "my_api.service",
                "health_check_type": "http",
                "health_check_url": "http://localhost:8000/health",
                "health_check_timeout": 10,
            }
        )
    """

    type = "bare_metal"

    def __init__(self, config: ProviderConfig):
        """Initialize Bare Metal provider with SSH configuration.

        Args:
            config: ProviderConfig with SSH and service details

        Raises:
            ProviderConfigError: If required configuration is missing
        """
        super().__init__(config)

        # SSH configuration
        self.ssh_host = config.url
        if not self.ssh_host:
            from . import ProviderConfigError
            raise ProviderConfigError("Bare Metal provider requires 'url' (SSH host)")

        self.ssh_user = config.custom_fields.get("ssh_user", "deploy")
        self.ssh_key_path = config.custom_fields.get("ssh_key_path", "~/.ssh/id_rsa")

        # Application configuration
        self.app_path = config.custom_fields.get("app_path")
        self.systemd_service = config.custom_fields.get("systemd_service")

        # Health check configuration
        self.health_check_type = config.custom_fields.get("health_check_type", "http")
        self.health_check_url = config.custom_fields.get("health_check_url")
        self.health_check_port = config.custom_fields.get("health_check_port")
        self.health_check_timeout = config.custom_fields.get("health_check_timeout", 10)

    def pre_flight_check(self) -> tuple[bool, str]:
        """Verify SSH connection to remote server.

        Returns:
            Tuple of (success: bool, message: str)
            - True if SSH connection successful
            - False if connection failed or server unreachable
        """
        try:
            cmd = [
                "ssh",
                "-i", self.ssh_key_path,
                f"{self.ssh_user}@{self.ssh_host}",
                "echo 'SSH connection test'",
            ]
            result = subprocess.run(
                cmd,
                capture_output=True,
                timeout=10,
                text=True,
            )

            if result.returncode == 0:
                return True, f"SSH connection to {self.ssh_host} successful"
            else:
                return False, f"SSH connection failed: {result.stderr}"

        except subprocess.TimeoutExpired:
            return False, f"SSH connection to {self.ssh_host} timed out"
        except Exception as e:
            return False, f"SSH connection error: {str(e)}"

    def deploy_service(
        self,
        service_name: str,
        version: str,
        config: dict[str, Any],
    ) -> DeploymentResult:
        """Deploy service via SSH and systemd.

        Steps:
        1. SSH to remote server
        2. Change to app directory
        3. Git pull to get latest code
        4. Systemctl restart service
        5. Wait for service to start
        6. Check health

        Args:
            service_name: Name of service to deploy
            version: Version string (commit SHA, tag, etc.)
            config: Service configuration (paths, ports, env vars, etc.)

        Returns:
            DeploymentResult with success/failure status

        Raises:
            ProviderDeploymentError: If deployment fails
        """
        import time as time_module

        start_time = time_module.time()
        old_version = None

        try:
            # Get current version before deployment
            old_version = self._get_remote_version()

            # Verify required configuration
            if not self.app_path:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="app_path not configured",
                    new_version=version,
                    old_version=old_version,
                )

            if not self.systemd_service:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="systemd_service not configured",
                    new_version=version,
                    old_version=old_version,
                )

            # Get branch from config (default to main)
            branch = config.get("branch", "main")

            # Build SSH command to pull latest code
            pull_cmd = (
                f"cd {self.app_path} && "
                f"git pull --ff-only origin {branch}"
            )

            result = self._run_ssh_command(pull_cmd)
            if result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"Git pull failed: {result['stderr']}",
                    new_version=version,
                    old_version=old_version,
                )

            # Restart systemd service
            restart_cmd = f"sudo systemctl restart {self.systemd_service}"
            result = self._run_ssh_command(restart_cmd)
            if result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"Systemctl restart failed: {result['stderr']}",
                    new_version=version,
                    old_version=old_version,
                )

            # Wait for service to start
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
        - version: currently deployed version (from git)
        - uptime: uptime in seconds
        - last_error: if any
        - port: if applicable

        Args:
            service_name: Name of service

        Returns:
            Dict with status information
        """
        try:
            # Get service status
            status_cmd = f"sudo systemctl is-active {self.systemd_service}"
            result = self._run_ssh_command(status_cmd)

            service_status = "running" if result["returncode"] == 0 else "stopped"

            # Get current version
            version = self._get_remote_version()

            # Get service info
            info_cmd = (
                f"sudo systemctl show {self.systemd_service} "
                f"-p MainPID,ExecMainStartTimestampMonotonic"
            )
            result = self._run_ssh_command(info_cmd)

            return {
                "status": service_status,
                "version": version or "unknown",
                "uptime": None,  # Would require parsing systemctl output
                "last_error": None,
                "port": None,
                "custom": {"stdout": result["stdout"]},
            }

        except Exception as e:
            return {
                "status": "error",
                "version": None,
                "last_error": str(e),
            }

    def rollback_service(
        self,
        service_name: str,
        to_version: str | None = None,
    ) -> DeploymentResult:
        """Rollback service to previous version.

        Uses git to checkout previous version or specific version.

        Args:
            service_name: Name of service to rollback
            to_version: Specific version to rollback to.
                       If None, rollback to HEAD~1 (previous commit)

        Returns:
            DeploymentResult with success/failure status
        """
        import time as time_module

        start_time = time_module.time()
        old_version = self._get_remote_version()

        try:
            if not self.app_path or not self.systemd_service:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message="app_path and systemd_service required for rollback",
                    new_version=to_version,
                    old_version=old_version,
                )

            # Determine target version
            target = to_version if to_version else "HEAD~1"

            # Checkout version
            checkout_cmd = f"cd {self.app_path} && git checkout {target}"
            result = self._run_ssh_command(checkout_cmd)
            if result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"Git checkout failed: {result['stderr']}",
                    new_version=to_version,
                    old_version=old_version,
                )

            # Restart service
            restart_cmd = f"sudo systemctl restart {self.systemd_service}"
            result = self._run_ssh_command(restart_cmd)
            if result["returncode"] != 0:
                return DeploymentResult(
                    success=False,
                    status=DeploymentStatus.FAILED,
                    error_message=f"Systemctl restart failed: {result['stderr']}",
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
            new_version = self._get_remote_version()
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
        - HTTP health checks (GET request)
        - TCP health checks (port connectivity)
        - Systemd status check

        Args:
            service_name: Name of service

        Returns:
            True if service is healthy, False otherwise
        """
        try:
            # First check if systemd service is running
            status_cmd = f"sudo systemctl is-active {self.systemd_service}"
            result = self._run_ssh_command(status_cmd)
            if result["returncode"] != 0:
                return False

            # If no health check configured, just check systemd status
            if self.health_check_type == "none":
                return True

            # HTTP health check
            if self.health_check_type == "http" and self.health_check_url:
                # Use curl on remote server to check HTTP endpoint
                curl_cmd = (
                    f"curl -s -f -m {self.health_check_timeout} "
                    f"{self.health_check_url}"
                )
                result = self._run_ssh_command(curl_cmd)
                return result["returncode"] == 0

            # TCP health check
            if self.health_check_type == "tcp" and self.health_check_port:
                # Use nc (netcat) on remote server to check port
                nc_cmd = (
                    f"timeout {self.health_check_timeout} "
                    f"nc -zv localhost {self.health_check_port}"
                )
                result = self._run_ssh_command(nc_cmd)
                return result["returncode"] == 0

            return True

        except Exception:
            return False

    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get recent logs from systemd service.

        Uses journalctl to retrieve service logs.

        Args:
            service_name: Name of service
            lines: Number of recent log lines to return

        Returns:
            Log output as string (newline-separated lines)
        """
        try:
            log_cmd = (
                f"sudo journalctl -u {self.systemd_service} "
                f"-n {lines} --no-pager"
            )
            result = self._run_ssh_command(log_cmd)
            return result["stdout"] if result["returncode"] == 0 else result["stderr"]

        except Exception as e:
            return f"Error retrieving logs: {str(e)}"

    # Private helper methods

    def _run_ssh_command(self, remote_cmd: str) -> dict[str, Any]:
        """Execute command on remote server via SSH.

        Args:
            remote_cmd: Command to execute on remote server

        Returns:
            Dict with returncode, stdout, stderr
        """
        cmd = [
            "ssh",
            "-i", self.ssh_key_path,
            f"{self.ssh_user}@{self.ssh_host}",
            remote_cmd,
        ]

        result = subprocess.run(
            cmd,
            capture_output=True,
            timeout=60,
            text=True,
        )

        return {
            "returncode": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
        }

    def _get_remote_version(self) -> str | None:
        """Get current git commit SHA from remote server.

        Returns:
            Commit SHA (short form) or None if error
        """
        try:
            if not self.app_path:
                return None

            version_cmd = f"cd {self.app_path} && git rev-parse --short HEAD"
            result = self._run_ssh_command(version_cmd)

            if result["returncode"] == 0:
                return result["stdout"].strip()
            return None

        except Exception:
            return None
