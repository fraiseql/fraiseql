"""SBOM Generator Application Service.

Orchestrates the creation of Software Bill of Materials by coordinating
between domain models and infrastructure repositories.

This is an Application Service in DDD terms - it contains application logic
but delegates domain logic to domain models and services.
"""

import logging
from datetime import UTC, datetime
from pathlib import Path
from typing import Optional

from fraiseql.sbom.domain.models import (
    SBOM,
    Component,
    ComponentIdentifier,
    ComponentType,
    Hash,
    HashAlgorithm,
    Supplier,
)
from fraiseql.sbom.domain.repositories import PackageMetadataRepository

logger = logging.getLogger(__name__)


class SBOMGenerator:
    """Application Service for generating Software Bill of Materials.

    Orchestrates the SBOM generation process by:
    1. Scanning installed packages (via repository)
    2. Creating domain objects (Components, Licenses, Hashes)
    3. Assembling the SBOM aggregate
    4. Delegating serialization to infrastructure adapters

    Attributes:
        metadata_repository: Repository for accessing package metadata
        include_dev_dependencies: Whether to include dev dependencies
    """

    def __init__(
        self,
        metadata_repository: PackageMetadataRepository,
        include_dev_dependencies: bool = False,
    ) -> None:
        """Initialize SBOM generator.

        Args:
            metadata_repository: Repository for package metadata
            include_dev_dependencies: Include development dependencies
        """
        self.metadata_repository = metadata_repository
        self.include_dev_dependencies = include_dev_dependencies

    def generate(
        self,
        component_name: str,
        component_version: str,
        component_description: Optional[str] = None,
        supplier: Optional[Supplier] = None,
        authors: Optional[list[str]] = None,
    ) -> SBOM:
        """Generate SBOM for the specified software component.

        Args:
            component_name: Name of the main component (e.g., "fraiseql")
            component_version: Version of the main component (e.g., "1.5.0")
            component_description: Optional description
            supplier: Optional supplier information
            authors: Optional list of author names

        Returns:
            Complete SBOM aggregate with all components

        Raises:
            ValueError: If required metadata is missing
        """
        logger.info(
            f"Generating SBOM for {component_name} v{component_version}",
            extra={
                "component": component_name,
                "version": component_version,
                "include_dev": self.include_dev_dependencies,
            },
        )

        # Create SBOM aggregate root
        sbom = SBOM(
            component_name=component_name,
            component_version=component_version,
            component_description=component_description,
            supplier=supplier,
            timestamp=datetime.now(UTC),
            tools=["fraiseql-sbom-generator"],
            authors=authors or [],
        )

        # Scan and add all dependencies as components
        component_identifiers = self.metadata_repository.get_installed_packages()

        logger.info(
            f"Found {len(component_identifiers)} packages to inventory",
            extra={"package_count": len(component_identifiers)},
        )

        for identifier in component_identifiers:
            try:
                component = self._create_component_from_identifier(identifier)
                sbom.add_component(component)
                logger.debug(
                    f"Added component: {identifier}",
                    extra={
                        "component": identifier.name,
                        "version": identifier.version,
                    },
                )
            except Exception as e:
                logger.warning(
                    f"Failed to add component {identifier}: {e}",
                    extra={
                        "component": identifier.name,
                        "version": identifier.version,
                        "error": str(e),
                    },
                )

        # Validate SBOM
        validation_issues = sbom.validate()
        if validation_issues:
            logger.warning(
                f"SBOM validation issues: {validation_issues}",
                extra={"issues": validation_issues},
            )

        logger.info(
            f"SBOM generation complete: {sbom.component_count()} components",
            extra={
                "component_count": sbom.component_count(),
                "has_copyleft": sbom.has_copyleft_components(),
            },
        )

        return sbom

    def _create_component_from_identifier(self, identifier: ComponentIdentifier) -> Component:
        """Create a Component entity from an identifier by enriching with metadata.

        Args:
            identifier: Basic component identification

        Returns:
            Fully populated Component entity
        """
        # Create component entity
        component = Component(
            identifier=identifier,
            type=ComponentType.LIBRARY,
            description=self.metadata_repository.get_package_description(
                identifier.name, identifier.version
            ),
        )

        # Add license information
        pkg_license = self.metadata_repository.get_package_license(
            identifier.name, identifier.version
        )
        if pkg_license:
            component.add_license(pkg_license)
        else:
            logger.debug(
                f"No license information found for {identifier.name}",
                extra={"component": identifier.name},
            )

        # Add hash for integrity verification
        package_hash = self.metadata_repository.get_package_hash(
            identifier.name, identifier.version
        )
        if package_hash:
            component.add_hash(Hash(algorithm=HashAlgorithm.SHA256, value=package_hash))

        # Add external reference (homepage)
        homepage = self.metadata_repository.get_package_homepage(
            identifier.name, identifier.version
        )
        if homepage:
            component.external_references["homepage"] = homepage

        return component

    def generate_and_save(
        self,
        output_path: Path,
        component_name: str,
        component_version: str,
        format: str = "json",
        component_description: Optional[str] = None,
        supplier: Optional[Supplier] = None,
        authors: Optional[list[str]] = None,
    ) -> Path:
        """Generate SBOM and save to file.

        Args:
            output_path: Path where SBOM file should be saved
            component_name: Name of the main component
            component_version: Version of the main component
            format: Output format ("json" or "xml")
            component_description: Optional description
            supplier: Optional supplier information
            authors: Optional list of authors

        Returns:
            Path to saved SBOM file

        Raises:
            ValueError: If format is not supported
            IOError: If file cannot be written
        """
        # Generate SBOM
        sbom = self.generate(
            component_name=component_name,
            component_version=component_version,
            component_description=component_description,
            supplier=supplier,
            authors=authors,
        )

        # Delegate serialization to infrastructure adapter
        # (will be implemented in infrastructure layer)
        from fraiseql.sbom.infrastructure.cyclonedx_adapter import CycloneDXAdapter

        adapter = CycloneDXAdapter()

        if format.lower() == "json":
            content = adapter.to_json(sbom)
            output_path.write_text(content)
        elif format.lower() == "xml":
            content = adapter.to_xml(sbom)
            output_path.write_text(content)
        else:
            raise ValueError(f"Unsupported format: {format}. Use 'json' or 'xml'.")

        logger.info(
            f"SBOM saved to {output_path}",
            extra={"output_path": str(output_path), "format": format},
        )

        return output_path
