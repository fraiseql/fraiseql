"""SBOM (Software Bill of Materials) generation module.

This module implements SBOM generation following Domain-Driven Design principles
to comply with industry supply chain security standards and regulatory requirements
(US EO 14028, EU NIS2/CRA, PCI-DSS 4.0, ISO 27001, etc.).

Bounded Context: Software Supply Chain Management

Key Concepts:
    - SBOM: Aggregate root representing a complete software bill of materials
    - Component: Entity representing a software dependency
    - License: Value object for license information
    - Hash: Value object for cryptographic integrity verification
"""

from fraiseql.sbom.application.sbom_generator import SBOMGenerator
from fraiseql.sbom.domain.models import (
    SBOM,
    Component,
    ComponentIdentifier,
    Hash,
    HashAlgorithm,
    License,
    Supplier,
)

__all__ = [
    "SBOM",
    "Component",
    "ComponentIdentifier",
    "Hash",
    "HashAlgorithm",
    "License",
    "SBOMGenerator",
    "Supplier",
]
