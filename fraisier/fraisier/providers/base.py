"""Base provider interface for deployment infrastructure.

All deployment providers (Bare Metal, Docker Compose, Coolify, etc.) implement
this interface to provide a consistent API for infrastructure operations.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class ProviderType(Enum):
    """Supported deployment provider types."""

    BARE_METAL = "bare_metal"
    DOCKER_COMPOSE = "docker_compose"
    COOLIFY = "coolify"


class HealthCheckType(Enum):
    """Health check methods."""

    HTTP = "http"
    TCP = "tcp"
    EXEC = "exec"
    SYSTEMD = "systemd"


@dataclass
class HealthCheck:
    """Health check configuration."""

    type: HealthCheckType = HealthCheckType.HTTP
    url: str | None = None  # For HTTP checks
    port: int | None = None  # For TCP checks
    command: str | None = None  # For EXEC checks
    service: str | None = None  # For systemd checks
    timeout: int = 30
    retries: int = 3
    retry_delay: int = 2


@dataclass
class ProviderStatus:
    """Provider status information."""

    available: bool
    version: str | None = None
    message: str = ""
    details: dict[str, Any] = field(default_factory=dict)


class DeploymentProvider(ABC):
    """Abstract base class for deployment providers.

    Providers handle infrastructure-specific operations:
    - Connection management (SSH for Bare Metal, API for Coolify, etc.)
    - Service deployment and management
    - Health checking
    - Logging and error handling
    """

    def __init__(self, config: dict[str, Any]):
        """Initialize provider with configuration.

        Args:
            config: Provider-specific configuration
        """
        self.config = config
        self.provider_type = self._get_provider_type()

    @abstractmethod
    def _get_provider_type(self) -> ProviderType:
        """Get the provider type.

        Returns:
            ProviderType enum value
        """
        pass

    @abstractmethod
    async def connect(self) -> None:
        """Establish connection to infrastructure.

        Raises:
            ConnectionError: If connection cannot be established
        """
        pass

    @abstractmethod
    async def disconnect(self) -> None:
        """Close connection to infrastructure."""
        pass

    @abstractmethod
    async def check_health(self, health_check: HealthCheck) -> bool:
        """Perform health check on deployed service.

        Args:
            health_check: Health check configuration

        Returns:
            True if healthy, False otherwise
        """
        pass

    @abstractmethod
    async def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get status of a deployed service.

        Args:
            service_name: Name of the service

        Returns:
            Dict with status information
        """
        pass

    @abstractmethod
    async def execute_command(self, command: str, timeout: int = 300) -> tuple[int, str, str]:
        """Execute a command on the infrastructure.

        Args:
            command: Command to execute
            timeout: Command timeout in seconds

        Returns:
            Tuple of (return_code, stdout, stderr)

        Raises:
            RuntimeError: If command execution fails
        """
        pass

    @abstractmethod
    async def upload_file(self, local_path: str, remote_path: str) -> None:
        """Upload a file to the infrastructure.

        Args:
            local_path: Local file path
            remote_path: Remote destination path

        Raises:
            FileNotFoundError: If local file doesn't exist
            RuntimeError: If upload fails
        """
        pass

    @abstractmethod
    async def download_file(self, remote_path: str, local_path: str) -> None:
        """Download a file from the infrastructure.

        Args:
            remote_path: Remote file path
            local_path: Local destination path

        Raises:
            RuntimeError: If download fails
        """
        pass

    async def check_provider_health(self) -> ProviderStatus:
        """Check overall provider health and availability.

        Returns:
            ProviderStatus with availability and details
        """
        try:
            await self.connect()
            await self.disconnect()
            return ProviderStatus(
                available=True,
                message="Provider is available",
            )
        except Exception as e:
            return ProviderStatus(
                available=False,
                message=f"Provider health check failed: {e}",
                details={"error": str(e)},
            )
