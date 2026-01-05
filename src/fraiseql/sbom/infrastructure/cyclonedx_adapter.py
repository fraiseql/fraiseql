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
from uuid import uuid4

from fraiseql.sbom.domain.models import (
    SBOM,
    Component,
    ComponentIdentifier,
    ComponentType,
    Hash,
    HashAlgorithm,
    License,
    Supplier,
)

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
                        {"email": sbom.supplier.contact},
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
            },
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

        Raises:
            ValueError: If JSON is invalid or missing required fields
        """
        try:
            data = json.loads(json_str)
        except json.JSONDecodeError as e:
            raise ValueError(f"Invalid JSON: {e}") from e

        # Validate basic structure
        if not isinstance(data, dict):
            raise TypeError("CycloneDX data must be a JSON object")

        if data.get("bomFormat") != "CycloneDX":
            raise ValueError("Invalid bomFormat, expected 'CycloneDX'")

        # Create SBOM instance
        sbom = cls._from_cyclonedx_dict(data)
        return sbom

    @classmethod
    def _from_cyclonedx_dict(cls, data: dict[str, Any]) -> SBOM:
        """Parse CycloneDX dictionary to SBOM domain model.

        Args:
            data: CycloneDX dictionary

        Returns:
            SBOM aggregate
        """
        # Basic SBOM fields
        sbom = SBOM(
            serial_number=data.get("serialNumber", f"urn:uuid:{uuid4()}"),
            version=data.get("version", 1),
            spec_version=data.get("specVersion", "1.5"),
            bom_format=data.get("bomFormat", "CycloneDX"),
        )

        # Parse metadata
        metadata = data.get("metadata", {})
        if isinstance(metadata, dict):
            cls._parse_metadata(sbom, metadata)

        # Parse components
        components_data = data.get("components", [])
        if isinstance(components_data, list):
            for comp_data in components_data:
                if isinstance(comp_data, dict):
                    try:
                        component = cls._parse_component(comp_data)
                        sbom.add_component(component)
                    except ValueError as e:
                        logger.warning(f"Skipping invalid component: {e}")

        # Parse dependencies
        dependencies_data = data.get("dependencies", [])
        if isinstance(dependencies_data, list):
            sbom.dependencies = {}
            for dep_data in dependencies_data:
                if isinstance(dep_data, dict):
                    comp_ref = dep_data.get("ref")
                    depends_on = dep_data.get("dependsOn", [])
                    if comp_ref and isinstance(depends_on, list):
                        sbom.dependencies[comp_ref] = depends_on

        return sbom

    @classmethod
    def _parse_metadata(cls, sbom: SBOM, metadata: dict[str, Any]) -> None:
        """Parse CycloneDX metadata into SBOM.

        Args:
            sbom: SBOM instance to update
            metadata: Metadata dictionary
        """
        from datetime import datetime

        # Parse timestamp
        timestamp_str = metadata.get("timestamp")
        if timestamp_str:
            try:
                # Handle different timestamp formats
                if "T" in timestamp_str:
                    sbom.timestamp = datetime.fromisoformat(timestamp_str.replace("Z", "+00:00"))
                else:
                    sbom.timestamp = datetime.fromisoformat(timestamp_str)
            except ValueError:
                logger.warning(f"Could not parse timestamp {timestamp_str}, using current time")

        # Parse tools
        tools_data = metadata.get("tools", [])
        if isinstance(tools_data, list):
            sbom.tools = [tool.get("name", "") for tool in tools_data if isinstance(tool, dict)]

        # Parse component metadata (the software being described)
        component_data = metadata.get("component", {})
        if isinstance(component_data, dict):
            sbom.component_name = component_data.get("name", "")
            sbom.component_version = component_data.get("version", "")
            sbom.component_description = component_data.get("description")

            # Parse supplier
            supplier_data = component_data.get("supplier", {})
            if isinstance(supplier_data, dict):
                name = supplier_data.get("name")
                url = supplier_data.get("url")
                if name:
                    sbom.supplier = Supplier(name=name, url=url)

    @classmethod
    def _parse_component(cls, comp_data: dict[str, Any]) -> Component:
        """Parse CycloneDX component dictionary to Component domain object.

        Args:
            comp_data: Component dictionary

        Returns:
            Component instance

        Raises:
            ValueError: If required component data is missing
        """
        # Parse identifier - require name and version
        name = comp_data.get("name")
        version = comp_data.get("version")
        if not name or not version:
            raise ValueError(f"Component missing required name or version: {comp_data}")

        # Create identifier - use PURL if available, otherwise construct one
        purl = comp_data.get("purl")
        if not purl:
            # Construct basic PURL from name and version
            purl = f"pkg:generic/{name}@{version}"

        identifier = ComponentIdentifier(name=name, version=version, purl=purl)

        # Parse component type
        comp_type_str = comp_data.get("type", "library")
        try:
            comp_type = ComponentType(comp_type_str)
        except ValueError:
            comp_type = ComponentType.LIBRARY  # Default fallback

        # Create component
        bom_ref = comp_data.get("bom-ref")
        if bom_ref is None:
            bom_ref = str(uuid4())

        component = Component(
            identifier=identifier,
            bom_ref=bom_ref,
            description=comp_data.get("description"),
            type=comp_type,
        )

        # Parse hashes
        hashes_data = comp_data.get("hashes", [])
        if isinstance(hashes_data, list):
            for hash_data in hashes_data:
                if isinstance(hash_data, dict):
                    alg = hash_data.get("alg")
                    content = hash_data.get("content")
                    if alg and content:
                        try:
                            hash_obj = Hash(algorithm=HashAlgorithm(alg), value=content)
                            component.add_hash(hash_obj)
                        except ValueError:
                            logger.warning(f"Unsupported hash algorithm: {alg}")

        # Parse licenses
        licenses_data = comp_data.get("licenses", [])
        if isinstance(licenses_data, list):
            for lic_data in licenses_data:
                if isinstance(lic_data, dict):
                    # Handle both license objects and license expressions
                    license_obj = lic_data.get("license")
                    if isinstance(license_obj, dict):
                        lic_id = license_obj.get("id")
                        lic_name = license_obj.get("name")
                        if lic_id and lic_name:
                            license_inst = License(id=lic_id, name=lic_name)
                            component.add_license(license_inst)
                    elif isinstance(license_obj, str):
                        # License expression - use as both id and name
                        license_inst = License(id=license_obj, name=license_obj)
                        component.add_license(license_inst)

        return component
