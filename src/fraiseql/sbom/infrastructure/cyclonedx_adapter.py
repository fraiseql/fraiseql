"""CycloneDX Format Adapter.

Infrastructure adapter that serializes SBOM domain models to CycloneDX
format (JSON or XML).

CycloneDX is an OWASP standard for Software Bill of Materials, widely
adopted for global supply chain security compliance (US EO 14028,
EU CRA, PCI-DSS 4.0, ISO 27001).
"""

import json
import logging
from typing import Any

from fraiseql.sbom.domain.models import SBOM, Component, License

logger = logging.getLogger(__name__)


class CycloneDXAdapter:
    """Adapter for CycloneDX SBOM format serialization.

    Converts SBOM domain models to CycloneDX JSON or XML format
    following the CycloneDX 1.5 specification.

    Attributes:
        spec_version: CycloneDX specification version (default: "1.5")
    """

    def __init__(self, spec_version: str = "1.5") -> None:
        """Initialize CycloneDX adapter.

        Args:
            spec_version: CycloneDX specification version
        """
        self.spec_version = spec_version

    def to_json(self, sbom: SBOM, indent: int = 2) -> str:
        """Serialize SBOM to CycloneDX JSON format.

        Args:
            sbom: SBOM aggregate to serialize
            indent: JSON indentation (default: 2)

        Returns:
            CycloneDX JSON string
        """
        cyclonedx_dict = self._to_cyclonedx_dict(sbom)
        return json.dumps(cyclonedx_dict, indent=indent, sort_keys=False)

    def to_xml(self, sbom: SBOM) -> str:
        """Serialize SBOM to CycloneDX XML format.

        Args:
            sbom: SBOM aggregate to serialize

        Returns:
            CycloneDX XML string

        Note:
            XML serialization requires additional dependencies.
            This is a placeholder for future implementation.
        """
        raise NotImplementedError("XML serialization not yet implemented. Use JSON format.")

    def _to_cyclonedx_dict(self, sbom: SBOM) -> dict[str, Any]:
        """Convert SBOM to CycloneDX dictionary structure.

        Args:
            sbom: SBOM aggregate

        Returns:
            Dictionary following CycloneDX 1.5 schema
        """
        cyclonedx: dict[str, Any] = {
            "bomFormat": "CycloneDX",
            "specVersion": self.spec_version,
            "serialNumber": sbom.serial_number,
            "version": sbom.version,
            "metadata": self._build_metadata(sbom),
            "components": [self._component_to_dict(comp) for comp in sbom.components],
        }

        # Add dependencies if present
        if sbom.dependencies:
            cyclonedx["dependencies"] = [
                {"ref": comp_ref, "dependsOn": dep_refs}
                for comp_ref, dep_refs in sbom.dependencies.items()
            ]

        return cyclonedx

    def _build_metadata(self, sbom: SBOM) -> dict[str, Any]:
        """Build CycloneDX metadata section.

        Args:
            sbom: SBOM aggregate

        Returns:
            Metadata dictionary
        """
        metadata: dict[str, Any] = {
            "timestamp": sbom.timestamp.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "tools": [{"name": tool, "vendor": "FraiseQL"} for tool in sbom.tools],
        }

        # Add component metadata (the software being described)
        if sbom.component_name and sbom.component_version:
            metadata["component"] = {
                "type": "application",
                "name": sbom.component_name,
                "version": sbom.component_version,
            }

            if sbom.component_description:
                metadata["component"]["description"] = sbom.component_description

            if sbom.supplier:
                metadata["component"]["supplier"] = {
                    "name": sbom.supplier.name,
                }
                if sbom.supplier.url:
                    metadata["component"]["supplier"]["url"] = [sbom.supplier.url]
                if sbom.supplier.contact:
                    metadata["component"]["supplier"]["contact"] = [
                        {"email": sbom.supplier.contact}
                    ]

        # Add authors
        if sbom.authors:
            metadata["authors"] = [{"name": author} for author in sbom.authors]

        return metadata

    def _component_to_dict(self, component: Component) -> dict[str, Any]:
        """Convert Component entity to CycloneDX dictionary.

        Args:
            component: Component entity

        Returns:
            Component dictionary following CycloneDX schema
        """
        comp_dict: dict[str, Any] = {
            "bom-ref": component.bom_ref,
            "type": component.type.value,
            "name": component.identifier.name,
            "version": component.identifier.version,
            "purl": component.identifier.purl,
        }

        # Add description
        if component.description:
            comp_dict["description"] = component.description

        # Add supplier
        if component.supplier:
            comp_dict["supplier"] = {
                "name": component.supplier.name,
            }
            if component.supplier.url:
                comp_dict["supplier"]["url"] = [component.supplier.url]

        # Add licenses
        if component.licenses:
            comp_dict["licenses"] = [self._license_to_dict(lic) for lic in component.licenses]

        # Add hashes
        if component.hashes:
            comp_dict["hashes"] = [
                {"alg": hash_obj.algorithm.value, "content": hash_obj.value}
                for hash_obj in component.hashes
            ]

        # Add external references
        if component.external_references:
            comp_dict["externalReferences"] = [
                {"type": ref_type, "url": url}
                for ref_type, url in component.external_references.items()
            ]

        # Add CPE if present
        if component.identifier.cpe:
            comp_dict["cpe"] = component.identifier.cpe

        return comp_dict

    def _license_to_dict(self, license: License) -> dict[str, Any]:
        """Convert License value object to CycloneDX dictionary.

        Args:
            license: License value object

        Returns:
            License dictionary
        """
        license_dict: dict[str, Any] = {
            "license": {
                "id": license.id,
                "name": license.name,
            }
        }

        if license.url:
            license_dict["license"]["url"] = license.url

        return license_dict

    @classmethod
    def from_json(cls, json_str: str) -> SBOM:
        """Deserialize CycloneDX JSON to SBOM domain model.

        Args:
            json_str: CycloneDX JSON string

        Returns:
            SBOM aggregate

        Note:
            This is a placeholder for future implementation.
            Deserialization is less critical than serialization for SBOM generation.
        """
        raise NotImplementedError(
            "SBOM deserialization not yet implemented. This feature is planned for future releases."
        )
