"""Provider abstraction layer for Fraisier deployment targets.

Supports multiple deployment providers:
- Bare Metal (SSH/systemd)
- Docker Compose
- Coolify

Each provider implements a common interface for deployment operations.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any

from fraisier.deployers.base import DeploymentResult


@dataclass
class ProviderConfig:
    """Configuration for a deployment provider.

    Attributes:
        name: Provider instance name (e.g., "production", "staging")
        type: Provider type (bare_metal, docker_compose, coolify)
        url: Base URL or connection string (SSH host, Docker daemon, Coolify URL)
        api_key: API key for providers that need authentication (Coolify)
        custom_fields: Additional provider-specific configuration
    """

    name: str
    type: str
    url: str | None = None
    api_key: str | None = None
    custom_fields: dict[str, Any] = field(default_factory=dict)


class BaseProvider(ABC):
    """Base class for all deployment providers.

    Providers handle deployment to various targets:
    - Bare metal servers (SSH/systemd)
    - Docker Compose deployments
    - Coolify platform
    - (Future) Kubernetes, cloud platforms, etc.

    All providers must implement these methods:
    - pre_flight_check: Verify provider accessibility
    - deploy_service: Deploy a service
    - get_service_status: Get current service status
    - rollback_service: Rollback to previous version
    - health_check: Check service health
    - get_logs: Retrieve service logs
    """

    name: str
    type: str

    def __init__(self, config: ProviderConfig):
        """Initialize provider with configuration.

        Args:
            config: ProviderConfig with connection and auth details
        """
        self.config = config
        self.name = config.name
        self.type = config.type

    @abstractmethod
    def pre_flight_check(self) -> tuple[bool, str]:
        """Verify provider is accessible and configured correctly.

        Returns:
            Tuple of (success: bool, message: str)
            - True if provider is ready to deploy
            - False if provider cannot be reached or is misconfigured
        """
        pass

    @abstractmethod
    def deploy_service(
        self,
        service_name: str,
        version: str,
        config: dict[str, Any],
    ) -> DeploymentResult:
        """Deploy a service to the provider.

        Args:
            service_name: Name of service to deploy
            version: Version string (commit SHA, tag, etc.)
            config: Service configuration (paths, ports, env vars, etc.)

        Returns:
            DeploymentResult with success/failure status and details

        Raises:
            ProviderError: If deployment fails
            ProviderConfigError: If configuration is invalid
        """
        pass

    @abstractmethod
    def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get current status of a deployed service.

        Args:
            service_name: Name of service

        Returns:
            Dict with status information:
            {
                "status": "running|stopped|error",
                "version": "currently deployed version",
                "uptime": "seconds",
                "last_error": "if any",
                "port": "port if applicable",
                "custom": "provider-specific info"
            }
        """
        pass

    @abstractmethod
    def rollback_service(
        self,
        service_name: str,
        to_version: str | None = None,
    ) -> DeploymentResult:
        """Rollback a service to previous version.

        Args:
            service_name: Name of service to rollback
            to_version: Specific version to rollback to.
                       If None, rollback to previous version.

        Returns:
            DeploymentResult with success/failure status
        """
        pass

    @abstractmethod
    def health_check(self, service_name: str) -> bool:
        """Check if service is healthy and responding.

        Args:
            service_name: Name of service

        Returns:
            True if service is healthy, False otherwise
        """
        pass

    @abstractmethod
    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get recent logs from a service.

        Args:
            service_name: Name of service
            lines: Number of recent log lines to return

        Returns:
            Log output as string (newline-separated lines)
        """
        pass


class ProviderRegistry:
    """Registry for managing available deployment providers.

    Maintains a mapping of provider types to provider classes.
    Allows plugins to register new providers at runtime.

    Example:
        >>> ProviderRegistry.register(BareMetalProvider)
        >>> provider = ProviderRegistry.get_provider("bare_metal", config)
    """

    _providers: dict[str, type[BaseProvider]] = {}

    @classmethod
    def register(cls, provider_class: type[BaseProvider]) -> None:
        """Register a new provider type.

        Args:
            provider_class: Provider class (must have `type` class attribute)

        Raises:
            ValueError: If provider type is already registered
        """
        if provider_class.type in cls._providers:
            raise ValueError(f"Provider '{provider_class.type}' already registered")
        cls._providers[provider_class.type] = provider_class

    @classmethod
    def get_provider(cls, provider_type: str, config: ProviderConfig) -> BaseProvider:
        """Get a provider instance.

        Args:
            provider_type: Type of provider (bare_metal, docker_compose, coolify)
            config: Provider configuration

        Returns:
            Provider instance

        Raises:
            ValueError: If provider type is not registered
        """
        if provider_type not in cls._providers:
            available = ", ".join(cls._providers.keys())
            raise ValueError(
                f"Unknown provider: {provider_type}. "
                f"Available: {available}"
            )
        return cls._providers[provider_type](config)

    @classmethod
    def list_providers(cls) -> list[str]:
        """List all available provider types.

        Returns:
            List of provider type strings
        """
        return sorted(list(cls._providers.keys()))

    @classmethod
    def is_registered(cls, provider_type: str) -> bool:
        """Check if a provider type is registered.

        Args:
            provider_type: Type of provider

        Returns:
            True if registered, False otherwise
        """
        return provider_type in cls._providers


class ProviderError(Exception):
    """Base exception for provider errors."""

    pass


class ProviderConfigError(ProviderError):
    """Exception for provider configuration errors."""

    pass


class ProviderConnectionError(ProviderError):
    """Exception for provider connection errors."""

    pass


class ProviderDeploymentError(ProviderError):
    """Exception for deployment failures."""

    pass


__all__ = [
    "BaseProvider",
    "ProviderConfig",
    "ProviderRegistry",
    "ProviderError",
    "ProviderConfigError",
    "ProviderConnectionError",
    "ProviderDeploymentError",
]
