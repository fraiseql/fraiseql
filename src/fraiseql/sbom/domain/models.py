"""SBOM Domain Models.

Domain-Driven Design implementation for Software Bill of Materials.
This module contains aggregates, entities, and value objects that represent
the core supply chain concepts.

Ubiquitous Language:
    - SBOM: A complete inventory of software components (Aggregate Root)
    - Component: A software package or dependency (Entity)
    - ComponentIdentifier: Unique identifier for a component (Value Object)
    - License: Software license information (Value Object)
    - Hash: Cryptographic hash for integrity verification (Value Object)
    - Supplier: Organization that created/maintains a component (Value Object)
"""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Optional
from uuid import uuid4


class HashAlgorithm(str, Enum):
    """Supported cryptographic hash algorithms for component integrity."""

    SHA256 = "SHA-256"
    SHA384 = "SHA-384"
    SHA512 = "SHA-512"
    SHA3_256 = "SHA3-256"
    SHA3_384 = "SHA3-384"
    SHA3_512 = "SHA3-512"


class ComponentType(str, Enum):
    """Types of software components in the supply chain."""

    APPLICATION = "application"  # Main application
    LIBRARY = "library"  # Reusable library
    FRAMEWORK = "framework"  # Framework
    CONTAINER = "container"  # Container image
    OPERATING_SYSTEM = "operating-system"  # OS
    DEVICE = "device"  # Hardware device
    FIRMWARE = "firmware"  # Firmware
    FILE = "file"  # Generic file


@dataclass(frozen=True)
class Hash:
    """Cryptographic hash for component integrity verification.

    Value Object - Immutable and compared by value, not identity.

    Attributes:
        algorithm: Hash algorithm used
        value: Hexadecimal hash digest
    """

    algorithm: HashAlgorithm
    value: str

    def __post_init__(self) -> None:
        """Validate hash value format."""
        if not self.value:
            raise ValueError("Hash value cannot be empty")
        if not all(c in "0123456789abcdefABCDEF" for c in self.value):
            raise ValueError(f"Invalid hex hash value: {self.value}")

    def __str__(self) -> str:
        """Return standard hash representation."""
        return f"{self.algorithm.value}:{self.value}"


@dataclass(frozen=True)
class License:
    """Software license information.

    Value Object - Immutable license details.

    Attributes:
        id: SPDX license identifier (e.g., "MIT", "Apache-2.0")
        name: Full license name
        url: URL to license text (optional)
    """

    id: str  # SPDX identifier
    name: str
    url: Optional[str] = None

    def __post_init__(self) -> None:
        """Validate license data."""
        if not self.id or not self.name:
            raise ValueError("License ID and name are required")

    @property
    def is_permissive(self) -> bool:
        """Check if license is permissive (federal-friendly)."""
        permissive_licenses = {
            "MIT",
            "Apache-2.0",
            "BSD-2-Clause",
            "BSD-3-Clause",
            "ISC",
            "0BSD",
        }
        return self.id in permissive_licenses

    @property
    def is_copyleft(self) -> bool:
        """Check if license is copyleft (GPL family)."""
        return "GPL" in self.id and "LGPL" not in self.id


@dataclass(frozen=True)
class Supplier:
    """Organization that created or maintains a software component.

    Value Object - Immutable supplier information.

    Attributes:
        name: Organization name
        url: Organization website (optional)
        contact: Contact information (optional)
    """

    name: str
    url: Optional[str] = None
    contact: Optional[str] = None

    def __post_init__(self) -> None:
        """Validate supplier data."""
        if not self.name:
            raise ValueError("Supplier name is required")


@dataclass(frozen=True)
class ComponentIdentifier:
    """Unique identifier for a software component.

    Value Object - Immutable identification data.

    Attributes:
        name: Component name (e.g., "fastapi")
        version: Semantic version (e.g., "0.115.12")
        purl: Package URL (universal identifier)
        cpe: Common Platform Enumeration (optional, for vulnerability matching)
    """

    name: str
    version: str
    purl: str  # Package URL (e.g., "pkg:pypi/fastapi@0.115.12")
    cpe: Optional[str] = None  # CPE for vulnerability databases

    def __post_init__(self) -> None:
        """Validate component identifier."""
        if not self.name or not self.version or not self.purl:
            raise ValueError("Name, version, and PURL are required")
        if not self.purl.startswith("pkg:"):
            raise ValueError(f"Invalid PURL format: {self.purl}")

    def __str__(self) -> str:
        """Return human-readable identifier."""
        return f"{self.name}@{self.version}"


@dataclass
class Component:
    """A software component (dependency) in the supply chain.

    Entity - Has identity (bom_ref) and mutable state.

    Attributes:
        bom_ref: Unique BOM reference (UUID)
        identifier: Component identification information
        type: Type of component
        supplier: Organization that created/maintains the component
        licenses: List of applicable licenses
        hashes: Cryptographic hashes for integrity
        description: Human-readable description
        external_references: URLs for documentation, source code, etc.
    """

    identifier: ComponentIdentifier
    type: ComponentType = ComponentType.LIBRARY
    bom_ref: str = field(default_factory=lambda: str(uuid4()))
    supplier: Optional[Supplier] = None
    licenses: list[License] = field(default_factory=list)
    hashes: list[Hash] = field(default_factory=list)
    description: Optional[str] = None
    external_references: dict[str, str] = field(default_factory=dict)

    def add_license(self, license: License) -> None:
        """Add a license to this component.

        Args:
            license: License to add

        Raises:
            ValueError: If license is already present
        """
        if license in self.licenses:
            raise ValueError(f"License {license.id} already exists")
        self.licenses.append(license)

    def add_hash(self, hash: Hash) -> None:
        """Add a cryptographic hash for integrity verification.

        Args:
            hash: Hash to add

        Raises:
            ValueError: If hash with same algorithm already exists
        """
        if any(h.algorithm == hash.algorithm for h in self.hashes):
            raise ValueError(f"Hash for {hash.algorithm.value} already exists")
        self.hashes.append(hash)

    def has_permissive_license(self) -> bool:
        """Check if component has at least one permissive license."""
        return any(lic.is_permissive for lic in self.licenses)

    def has_copyleft_license(self) -> bool:
        """Check if component has any copyleft licenses (GPL)."""
        return any(lic.is_copyleft for lic in self.licenses)

    def __eq__(self, other: object) -> bool:
        """Components are equal if they have the same bom_ref."""
        if not isinstance(other, Component):
            return False
        return self.bom_ref == other.bom_ref

    def __hash__(self) -> int:
        """Hash based on bom_ref (entity identity)."""
        return hash(self.bom_ref)


@dataclass
class SBOM:
    """Software Bill of Materials - Aggregate Root.

    Represents a complete inventory of software components for a specific version
    of the software. This is the aggregate root that maintains consistency of
    the entire component graph.

    Attributes:
        serial_number: Unique SBOM serial number (UUID URN)
        version: SBOM specification version
        metadata: SBOM metadata (creation time, tools, etc.)
        components: List of software components
        dependencies: Component dependency relationships
    """

    serial_number: str = field(default_factory=lambda: f"urn:uuid:{uuid4()}")
    version: int = 1
    spec_version: str = "1.5"  # CycloneDX specification version
    bom_format: str = "CycloneDX"

    # Metadata about the SBOM itself
    timestamp: datetime = field(default_factory=datetime.utcnow)
    tools: list[str] = field(default_factory=list)
    authors: list[str] = field(default_factory=list)

    # Metadata about the software being inventoried
    component_name: str = ""
    component_version: str = ""
    component_description: Optional[str] = None
    supplier: Optional[Supplier] = None

    # The inventory itself
    components: list[Component] = field(default_factory=list)
    dependencies: dict[str, list[str]] = field(
        default_factory=dict
    )  # bom_ref -> list of dependency bom_refs

    def add_component(self, component: Component) -> None:
        """Add a component to the SBOM.

        Args:
            component: Component to add

        Raises:
            ValueError: If component with same identifier already exists
        """
        # Check for duplicates based on name+version
        existing = self.find_component(component.identifier.name, component.identifier.version)
        if existing:
            raise ValueError(
                f"Component {component.identifier} already exists with bom_ref {existing.bom_ref}"
            )

        self.components.append(component)

    def find_component(self, name: str, version: str) -> Optional[Component]:
        """Find a component by name and version.

        Args:
            name: Component name
            version: Component version

        Returns:
            Component if found, None otherwise
        """
        for component in self.components:
            if component.identifier.name == name and component.identifier.version == version:
                return component
        return None

    def find_component_by_bom_ref(self, bom_ref: str) -> Optional[Component]:
        """Find a component by its BOM reference.

        Args:
            bom_ref: BOM reference (UUID)

        Returns:
            Component if found, None otherwise
        """
        for component in self.components:
            if component.bom_ref == bom_ref:
                return component
        return None

    def add_dependency(self, component_ref: str, depends_on_refs: list[str]) -> None:
        """Record dependency relationships between components.

        Args:
            component_ref: BOM reference of the component
            depends_on_refs: List of BOM references this component depends on

        Raises:
            ValueError: If component_ref doesn't exist
        """
        if not self.find_component_by_bom_ref(component_ref):
            raise ValueError(f"Component with bom_ref {component_ref} not found")

        # Validate all dependency references exist
        for dep_ref in depends_on_refs:
            if not self.find_component_by_bom_ref(dep_ref):
                raise ValueError(f"Dependency component with bom_ref {dep_ref} not found")

        self.dependencies[component_ref] = depends_on_refs

    def get_dependencies(self, component_ref: str) -> list[Component]:
        """Get all direct dependencies of a component.

        Args:
            component_ref: BOM reference of the component

        Returns:
            List of dependency components
        """
        dep_refs = self.dependencies.get(component_ref, [])
        return [
            comp for comp in self.components if comp.bom_ref in dep_refs
        ]  # Type-safe list comprehension

    def component_count(self) -> int:
        """Get total number of components in the SBOM."""
        return len(self.components)

    def has_copyleft_components(self) -> bool:
        """Check if SBOM contains any copyleft-licensed components."""
        return any(comp.has_copyleft_license() for comp in self.components)

    def get_copyleft_components(self) -> list[Component]:
        """Get all components with copyleft licenses (GPL)."""
        return [comp for comp in self.components if comp.has_copyleft_license()]

    def validate(self) -> list[str]:
        """Validate SBOM consistency and return list of issues.

        Returns:
            List of validation error messages (empty if valid)
        """
        issues: list[str] = []

        # Check required metadata
        if not self.component_name:
            issues.append("SBOM component_name is required")
        if not self.component_version:
            issues.append("SBOM component_version is required")

        # Check components have required data
        for comp in self.components:
            if not comp.licenses:
                issues.append(f"Component {comp.identifier} has no license information")

        # Check dependency references are valid
        for comp_ref, dep_refs in self.dependencies.items():
            if not self.find_component_by_bom_ref(comp_ref):
                issues.append(f"Dependency reference to unknown component: {comp_ref}")
            for dep_ref in dep_refs:
                if not self.find_component_by_bom_ref(dep_ref):
                    issues.append(f"Dependency reference to unknown component: {dep_ref}")

        return issues

    def __repr__(self) -> str:
        """Return string representation of SBOM."""
        return (
            f"SBOM(serial={self.serial_number[:13]}..., "
            f"component={self.component_name}@{self.component_version}, "
            f"components={len(self.components)})"
        )
