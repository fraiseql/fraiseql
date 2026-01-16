"""Base deployer interface for all fraise types."""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class DeploymentStatus(Enum):
    """Deployment status values."""
    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    SUCCESS = "success"
    FAILED = "failed"
    ROLLED_BACK = "rolled_back"


@dataclass
class DeploymentResult:
    """Result of a deployment operation."""
    success: bool
    status: DeploymentStatus
    old_version: str | None = None
    new_version: str | None = None
    duration_seconds: float = 0.0
    error_message: str | None = None
    details: dict[str, Any] = field(default_factory=dict)


class BaseDeployer(ABC):
    """Abstract base class for fraise deployers.

    Each fraise type (api, etl, scheduled, backup) has its own deployer
    that implements this interface.
    """

    def __init__(self, config: dict[str, Any]):
        """Initialize deployer with fraise configuration.

        Args:
            config: Fraise configuration from fraises.yaml
        """
        self.config = config
        self.fraise_name = config.get("fraise_name", "unknown")
        self.environment = config.get("environment", "unknown")

    @abstractmethod
    def get_current_version(self) -> str | None:
        """Get the currently deployed version.

        Returns:
            Version string or None if not deployed
        """
        pass

    @abstractmethod
    def get_latest_version(self) -> str | None:
        """Get the latest available version (e.g., from git).

        Returns:
            Version string or None if unavailable
        """
        pass

    def is_deployment_needed(self) -> bool:
        """Check if deployment is needed (versions differ).

        Returns:
            True if deployment should proceed
        """
        current = self.get_current_version()
        latest = self.get_latest_version()

        if current is None or latest is None:
            return True

        return current != latest

    @abstractmethod
    def execute(self) -> DeploymentResult:
        """Execute the deployment.

        Returns:
            DeploymentResult with success/failure status
        """
        pass

    def rollback(self, to_version: str | None = None) -> DeploymentResult:
        """Rollback to a previous version.

        Args:
            to_version: Specific version to rollback to, or previous if None

        Returns:
            DeploymentResult with rollback status
        """
        # Default implementation - subclasses can override
        return DeploymentResult(
            success=False,
            status=DeploymentStatus.FAILED,
            error_message="Rollback not implemented for this fraise type",
        )

    def health_check(self) -> bool:
        """Check if the fraise is healthy after deployment.

        Returns:
            True if healthy
        """
        # Default implementation - subclasses can override
        return True
