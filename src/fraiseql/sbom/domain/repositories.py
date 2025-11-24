"""SBOM Domain Repositories.

Repository interfaces define the contract for accessing and persisting
SBOM data. Infrastructure layer provides concrete implementations.

Following the Repository pattern from Domain-Driven Design to maintain
separation between domain logic and data access concerns.
"""

from abc import ABC, abstractmethod
from typing import Optional

from fraiseql.sbom.domain.models import ComponentIdentifier, License


class PackageMetadataRepository(ABC):
    """Repository interface for accessing package metadata.

    Abstracts the source of package information (pyproject.toml, uv.lock,
    pip, etc.) from the domain logic.
    """

    @abstractmethod
    def get_installed_packages(self) -> list[ComponentIdentifier]:
        """Get list of all installed packages with identifiers.

        Returns:
            List of component identifiers for installed packages
        """
        ...

    @abstractmethod
    def get_package_license(self, name: str, version: str) -> Optional[License]:
        """Get license information for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            License if found, None otherwise
        """
        ...

    @abstractmethod
    def get_package_description(self, name: str, version: str) -> Optional[str]:
        """Get description for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            Description if found, None otherwise
        """
        ...

    @abstractmethod
    def get_package_homepage(self, name: str, version: str) -> Optional[str]:
        """Get homepage URL for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            Homepage URL if found, None otherwise
        """
        ...

    @abstractmethod
    def get_package_hash(self, name: str, version: str) -> Optional[str]:
        """Get SHA256 hash for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            SHA256 hash if found, None otherwise
        """
        ...
