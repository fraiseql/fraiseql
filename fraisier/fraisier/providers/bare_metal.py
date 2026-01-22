"""Bare Metal provider for SSH + systemd deployments.

Handles deployment to bare metal or VM infrastructure via SSH,
with systemd service management and TCP health checks.
"""

import asyncio
import logging
from typing import Any

from .base import DeploymentProvider, HealthCheck, HealthCheckType, ProviderType

logger = logging.getLogger(__name__)


class BareMetalProvider(DeploymentProvider):
    """Deploy to bare metal servers via SSH.

    Supports:
    - SSH key-based authentication
    - systemd service management
    - TCP and HTTP health checks
    - Command execution
    - File operations (upload/download)
    """

    def __init__(self, config: dict[str, Any]):
        """Initialize bare metal provider.

        Config should include:
            host: SSH hostname or IP
            port: SSH port (default 22)
            username: SSH username
            key_path: Path to SSH private key
            known_hosts_path: Optional custom known_hosts file
        """
        super().__init__(config)
        self.host = config.get("host")
        self.port = config.get("port", 22)
        self.username = config.get("username", "root")
        self.key_path = config.get("key_path")
        self.known_hosts_path = config.get("known_hosts_path")
        self.ssh_client = None
        self._connection_timeout = 10

        if not self.host:
            raise ValueError("Bare Metal provider requires 'host' configuration")

    def _get_provider_type(self) -> ProviderType:
        """Return provider type."""
        return ProviderType.BARE_METAL

    async def connect(self) -> None:
        """Establish SSH connection.

        Raises:
            ConnectionError: If SSH connection fails
        """
        try:
            import asyncssh

            # Create SSH connection options
            options = asyncssh.SSHClientConnectionOptions()
            if self.key_path:
                options.client_keys = [self.key_path]
            if self.known_hosts_path:
                options.known_hosts = self.known_hosts_path
            else:
                options.known_hosts = None  # Accept unknown hosts

            # Establish connection
            self.ssh_client = await asyncssh.connect(
                self.host,
                port=self.port,
                username=self.username,
                options=options,
                connect_timeout=self._connection_timeout,
            )
            logger.info(f"Connected to {self.username}@{self.host}:{self.port}")

        except ImportError:
            raise ConnectionError(
                "asyncssh not installed. Install with: pip install asyncssh"
            )
        except Exception as e:
            raise ConnectionError(
                f"Failed to connect to {self.host}:{self.port}: {e}"
            ) from e

    async def disconnect(self) -> None:
        """Close SSH connection."""
        if self.ssh_client:
            self.ssh_client.close()
            await self.ssh_client.wait_closed()
            logger.info(f"Disconnected from {self.host}")

    async def execute_command(
        self,
        command: str,
        timeout: int = 300,
    ) -> tuple[int, str, str]:
        """Execute command via SSH.

        Args:
            command: Command to execute
            timeout: Command timeout in seconds

        Returns:
            Tuple of (return_code, stdout, stderr)

        Raises:
            RuntimeError: If connection not established or command fails
        """
        if not self.ssh_client:
            raise RuntimeError("Not connected to SSH server")

        try:
            result = await asyncio.wait_for(
                self.ssh_client.run(command),
                timeout=timeout,
            )
            return (
                result.exit_status,
                result.stdout or "",
                result.stderr or "",
            )
        except asyncio.TimeoutError:
            raise RuntimeError(f"Command timed out after {timeout} seconds: {command}")
        except Exception as e:
            raise RuntimeError(f"Command execution failed: {e}") from e

    async def upload_file(self, local_path: str, remote_path: str) -> None:
        """Upload file via SCP.

        Args:
            local_path: Local file path
            remote_path: Remote destination path

        Raises:
            FileNotFoundError: If local file doesn't exist
            RuntimeError: If upload fails
        """
        if not self.ssh_client:
            raise RuntimeError("Not connected to SSH server")

        try:
            import asyncssh

            async with asyncssh.connect(
                self.host,
                port=self.port,
                username=self.username,
            ) as conn:
                await conn.copy_files(local_path, (conn, remote_path))
                logger.info(f"Uploaded {local_path} to {remote_path}")

        except FileNotFoundError as e:
            raise FileNotFoundError(f"Local file not found: {local_path}") from e
        except Exception as e:
            raise RuntimeError(f"File upload failed: {e}") from e

    async def download_file(self, remote_path: str, local_path: str) -> None:
        """Download file via SCP.

        Args:
            remote_path: Remote file path
            local_path: Local destination path

        Raises:
            RuntimeError: If download fails
        """
        if not self.ssh_client:
            raise RuntimeError("Not connected to SSH server")

        try:
            import asyncssh

            async with asyncssh.connect(
                self.host,
                port=self.port,
                username=self.username,
            ) as conn:
                await conn.copy_files((conn, remote_path), local_path)
                logger.info(f"Downloaded {remote_path} to {local_path}")

        except Exception as e:
            raise RuntimeError(f"File download failed: {e}") from e

    async def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get systemd service status.

        Args:
            service_name: Service name (without .service suffix)

        Returns:
            Dict with status information
        """
        try:
            exit_code, stdout, stderr = await self.execute_command(
                f"systemctl is-active {service_name}.service"
            )

            if exit_code == 0:
                # Also get details
                _, details, _ = await self.execute_command(
                    f"systemctl show {service_name}.service -p ActiveState,SubState"
                )

                return {
                    "service": service_name,
                    "active": True,
                    "state": stdout.strip(),
                    "details": details,
                }

            return {
                "service": service_name,
                "active": False,
                "state": "inactive",
                "error": stderr,
            }

        except Exception as e:
            return {
                "service": service_name,
                "active": False,
                "error": str(e),
            }

    async def check_health(self, health_check: HealthCheck) -> bool:
        """Check service health.

        Supports HTTP, TCP, exec, and systemd checks.

        Args:
            health_check: Health check configuration

        Returns:
            True if healthy, False otherwise
        """
        for attempt in range(health_check.retries):
            try:
                if health_check.type == HealthCheckType.HTTP:
                    return await self._check_http(health_check)

                elif health_check.type == HealthCheckType.TCP:
                    return await self._check_tcp(health_check)

                elif health_check.type == HealthCheckType.EXEC:
                    return await self._check_exec(health_check)

                elif health_check.type == HealthCheckType.SYSTEMD:
                    return await self._check_systemd(health_check)

                else:
                    logger.warning(f"Unknown health check type: {health_check.type}")
                    return False

            except Exception as e:
                logger.warning(
                    f"Health check attempt {attempt + 1}/{health_check.retries} failed: {e}"
                )
                if attempt < health_check.retries - 1:
                    await asyncio.sleep(health_check.retry_delay)
                continue

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

    async def _check_exec(self, health_check: HealthCheck) -> bool:
        """Check using exec command."""
        if not health_check.command:
            logger.error("Exec health check requires 'command'")
            return False

        try:
            exit_code, _, _ = await self.execute_command(
                health_check.command,
                timeout=health_check.timeout,
            )
            return exit_code == 0

        except Exception as e:
            logger.debug(f"Exec health check failed: {e}")
            return False

    async def _check_systemd(self, health_check: HealthCheck) -> bool:
        """Check systemd service status."""
        if not health_check.service:
            logger.error("Systemd health check requires 'service'")
            return False

        try:
            exit_code, _, _ = await self.execute_command(
                f"systemctl is-active {health_check.service}.service"
            )
            return exit_code == 0

        except Exception as e:
            logger.debug(f"Systemd health check failed: {e}")
            return False

    async def start_service(self, service_name: str, timeout: int = 60) -> bool:
        """Start a systemd service.

        Args:
            service_name: Service name (without .service)
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"systemctl start {service_name}.service",
                timeout=timeout,
            )
            if exit_code == 0:
                logger.info(f"Started service {service_name}")
                return True
            else:
                logger.error(f"Failed to start {service_name}: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error starting service {service_name}: {e}")
            return False

    async def stop_service(self, service_name: str, timeout: int = 60) -> bool:
        """Stop a systemd service.

        Args:
            service_name: Service name (without .service)
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"systemctl stop {service_name}.service",
                timeout=timeout,
            )
            if exit_code == 0:
                logger.info(f"Stopped service {service_name}")
                return True
            else:
                logger.error(f"Failed to stop {service_name}: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error stopping service {service_name}: {e}")
            return False

    async def restart_service(self, service_name: str, timeout: int = 60) -> bool:
        """Restart a systemd service.

        Args:
            service_name: Service name (without .service)
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"systemctl restart {service_name}.service",
                timeout=timeout,
            )
            if exit_code == 0:
                logger.info(f"Restarted service {service_name}")
                return True
            else:
                logger.error(f"Failed to restart {service_name}: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error restarting service {service_name}: {e}")
            return False

    async def enable_service(self, service_name: str) -> bool:
        """Enable a systemd service (auto-start on boot).

        Args:
            service_name: Service name (without .service)

        Returns:
            True if successful
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"systemctl enable {service_name}.service"
            )
            if exit_code == 0:
                logger.info(f"Enabled service {service_name}")
                return True
            else:
                logger.error(f"Failed to enable {service_name}: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error enabling service {service_name}: {e}")
            return False
