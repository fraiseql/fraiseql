"""Docker Compose provider for containerized deployments.

Handles deployment to Docker Compose stacks with service management,
health checks, and container orchestration.
"""

import asyncio
import json
import logging
from typing import Any, TYPE_CHECKING

from .base import DeploymentProvider, HealthCheck, HealthCheckType, ProviderType
from fraisier.nats.provider import NatsEventProvider

if TYPE_CHECKING:
    from fraisier.nats.client import NatsEventBus

logger = logging.getLogger(__name__)


class DockerComposeProvider(DeploymentProvider, NatsEventProvider):
    """Deploy services using Docker Compose.

    Supports:
    - Docker Compose stack management
    - Service deployment and updates
    - Container health checks
    - Log streaming
    - Volume management
    - Network configuration
    """

    def __init__(
        self,
        config: dict[str, Any],
        event_bus: "NatsEventBus | None" = None,
        region: str | None = None,
    ):
        """Initialize Docker Compose provider.

        Config should include:
            compose_file: Path to docker-compose.yml
            project_name: Docker Compose project name
            docker_host: Docker daemon socket/host (optional)
            timeout: Command timeout in seconds (default 300)

        Args:
            config: Provider configuration
            event_bus: Optional NATS event bus for emitting deployment events
            region: Optional region identifier for multi-region deployments
        """
        super().__init__(config)
        self.compose_file = config.get("compose_file", "docker-compose.yml")
        self.project_name = config.get("project_name", "fraisier")
        self.docker_host = config.get("docker_host")
        self.timeout = config.get("timeout", 300)
        self.docker_available = False

        # NATS event bus for emitting events
        self.event_bus = event_bus
        self.region = region

    def _get_provider_type(self) -> ProviderType:
        """Return provider type."""
        return ProviderType.DOCKER_COMPOSE

    async def connect(self) -> None:
        """Verify Docker and docker-compose availability.

        Raises:
            ConnectionError: If Docker or docker-compose not available
        """
        try:
            # Check docker availability
            exit_code, stdout, stderr = await self.execute_command("docker --version")
            if exit_code != 0:
                raise ConnectionError(f"Docker not available: {stderr}")

            # Check docker-compose availability
            exit_code, stdout, stderr = await self.execute_command("docker-compose --version")
            if exit_code != 0:
                raise ConnectionError(f"docker-compose not available: {stderr}")

            self.docker_available = True
            logger.info("Connected to Docker daemon")

        except Exception as e:
            raise ConnectionError(f"Failed to connect to Docker: {e}") from e

    async def disconnect(self) -> None:
        """Disconnect from Docker (no-op for Docker Compose)."""
        self.docker_available = False
        logger.info("Disconnected from Docker daemon")

    async def execute_command(
        self,
        command: str,
        timeout: int | None = None,
    ) -> tuple[int, str, str]:
        """Execute a shell command.

        Args:
            command: Command to execute
            timeout: Command timeout in seconds

        Returns:
            Tuple of (return_code, stdout, stderr)

        Raises:
            RuntimeError: If command fails
        """
        if timeout is None:
            timeout = self.timeout

        try:
            process = await asyncio.create_subprocess_shell(
                command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )

            try:
                stdout_data, stderr_data = await asyncio.wait_for(
                    process.communicate(),
                    timeout=timeout,
                )
            except asyncio.TimeoutError:
                process.kill()
                raise RuntimeError(f"Command timed out after {timeout} seconds: {command}")

            return (
                process.returncode,
                stdout_data.decode(),
                stderr_data.decode(),
            )

        except asyncio.TimeoutError as e:
            raise RuntimeError(f"Command timed out: {command}") from e
        except Exception as e:
            raise RuntimeError(f"Command execution failed: {e}") from e

    async def upload_file(self, local_path: str, remote_path: str) -> None:
        """Upload file to container via docker cp.

        Args:
            local_path: Local file path
            remote_path: Remote path (container_name:path)

        Raises:
            FileNotFoundError: If local file doesn't exist
            RuntimeError: If upload fails
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker cp {local_path} {remote_path}"
            )
            if exit_code != 0:
                raise RuntimeError(f"Docker cp failed: {stderr}")

            logger.info(f"Uploaded {local_path} to {remote_path}")

        except FileNotFoundError as e:
            raise FileNotFoundError(f"Local file not found: {local_path}") from e
        except Exception as e:
            raise RuntimeError(f"File upload failed: {e}") from e

    async def download_file(self, remote_path: str, local_path: str) -> None:
        """Download file from container via docker cp.

        Args:
            remote_path: Remote path (container_name:path)
            local_path: Local destination path

        Raises:
            RuntimeError: If download fails
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker cp {remote_path} {local_path}"
            )
            if exit_code != 0:
                raise RuntimeError(f"Docker cp failed: {stderr}")

            logger.info(f"Downloaded {remote_path} to {local_path}")

        except Exception as e:
            raise RuntimeError(f"File download failed: {e}") from e

    async def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get Docker Compose service status.

        Args:
            service_name: Service name (from docker-compose.yml)

        Returns:
            Dict with status information
        """
        try:
            # Get container status
            exit_code, stdout, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"ps {service_name} --format json"
            )

            if exit_code != 0:
                return {
                    "service": service_name,
                    "active": False,
                    "state": "unknown",
                    "error": stderr,
                }

            # Parse JSON output
            containers = json.loads(stdout)
            if isinstance(containers, list) and len(containers) > 0:
                container = containers[0]
                return {
                    "service": service_name,
                    "active": container.get("State") == "running",
                    "state": container.get("State", "unknown"),
                    "container_id": container.get("ID", "")[:12],
                    "image": container.get("Image", ""),
                }

            return {
                "service": service_name,
                "active": False,
                "state": "not_running",
            }

        except Exception as e:
            return {
                "service": service_name,
                "active": False,
                "error": str(e),
            }

    async def check_health(self, health_check: HealthCheck) -> bool:
        """Check service health.

        Supports HTTP, TCP, and exec checks.
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

                elif health_check.type == HealthCheckType.EXEC:
                    result = await self._check_exec(health_check)

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

    async def _check_exec(self, health_check: HealthCheck) -> bool:
        """Check using docker exec command."""
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

    async def start_service(self, service_name: str, timeout: int | None = None) -> bool:
        """Start a service in the Docker Compose stack.

        Args:
            service_name: Service name
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        if timeout is None:
            timeout = self.timeout

        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"up -d {service_name}",
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

    async def stop_service(self, service_name: str, timeout: int | None = None) -> bool:
        """Stop a service in the Docker Compose stack.

        Args:
            service_name: Service name
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        if timeout is None:
            timeout = self.timeout

        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"stop {service_name}",
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

    async def restart_service(self, service_name: str, timeout: int | None = None) -> bool:
        """Restart a service in the Docker Compose stack.

        Args:
            service_name: Service name
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        if timeout is None:
            timeout = self.timeout

        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"restart {service_name}",
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

    async def pull_image(self, service_name: str, timeout: int | None = None) -> bool:
        """Pull latest image for a service.

        Args:
            service_name: Service name
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        if timeout is None:
            timeout = self.timeout

        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"pull {service_name}",
                timeout=timeout,
            )
            if exit_code == 0:
                logger.info(f"Pulled latest image for {service_name}")
                return True
            else:
                logger.error(f"Failed to pull image for {service_name}: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error pulling image for {service_name}: {e}")
            return False

    async def get_container_logs(
        self,
        service_name: str,
        lines: int = 100,
    ) -> str:
        """Get container logs for a service.

        Args:
            service_name: Service name
            lines: Number of log lines to retrieve

        Returns:
            Log output as string
        """
        try:
            exit_code, stdout, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"logs --tail {lines} {service_name}"
            )
            if exit_code == 0:
                return stdout
            else:
                return f"Error getting logs: {stderr}"

        except Exception as e:
            return f"Error getting logs: {e}"

    async def get_service_env(self, service_name: str) -> dict[str, str]:
        """Get environment variables for a service.

        Args:
            service_name: Service name

        Returns:
            Dict of environment variables
        """
        try:
            exit_code, stdout, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"exec {service_name} env"
            )
            if exit_code != 0:
                logger.warning(f"Failed to get env for {service_name}: {stderr}")
                return {}

            env_dict = {}
            for line in stdout.strip().split("\n"):
                if "=" in line:
                    key, value = line.split("=", 1)
                    env_dict[key] = value

            return env_dict

        except Exception as e:
            logger.warning(f"Error getting service env: {e}")
            return {}

    async def scale_service(
        self,
        service_name: str,
        replicas: int,
        timeout: int | None = None,
    ) -> bool:
        """Scale a service to desired number of replicas.

        Args:
            service_name: Service name
            replicas: Number of desired replicas
            timeout: Timeout in seconds

        Returns:
            True if successful
        """
        if timeout is None:
            timeout = self.timeout

        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} -p {self.project_name} "
                f"up -d --scale {service_name}={replicas}",
                timeout=timeout,
            )
            if exit_code == 0:
                logger.info(f"Scaled {service_name} to {replicas} replicas")
                return True
            else:
                logger.error(f"Failed to scale {service_name}: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error scaling service {service_name}: {e}")
            return False

    async def validate_compose_file(self) -> bool:
        """Validate docker-compose.yml syntax.

        Returns:
            True if valid
        """
        try:
            exit_code, _, stderr = await self.execute_command(
                f"docker-compose -f {self.compose_file} config > /dev/null"
            )
            if exit_code == 0:
                logger.info(f"Compose file {self.compose_file} is valid")
                return True
            else:
                logger.error(f"Compose file validation failed: {stderr}")
                return False

        except Exception as e:
            logger.error(f"Error validating compose file: {e}")
            return False
