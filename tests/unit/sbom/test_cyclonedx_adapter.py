"""Unit tests for CycloneDX adapter."""

import json

import pytest

from fraiseql.sbom.domain.models import (
    SBOM,
    Component,
    ComponentIdentifier,
    Hash,
    HashAlgorithm,
    License,
    Supplier,
)
from fraiseql.sbom.infrastructure.cyclonedx_adapter import CycloneDXAdapter


class TestCycloneDXAdapter:
    """Tests for CycloneDX serialization adapter."""

    def test_to_json_basic(self) -> None:
        """Test basic JSON serialization."""
        # Create simple SBOM
        sbom = SBOM(
            component_name="fraiseql",
            component_version="1.5.0",
            component_description="GraphQL framework",
        )

        # Add a component
        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        component = Component(identifier=identifier)
        component.add_license(License(id="MIT", name="MIT License"))
        sbom.add_component(component)

        # Serialize
        adapter = CycloneDXAdapter()
        json_str = adapter.to_json(sbom)

        # Parse and validate
        data = json.loads(json_str)

        assert data["bomFormat"] == "CycloneDX"
        assert data["specVersion"] == "1.5"
        assert data["version"] == 1
        assert "serialNumber" in data
        assert data["serialNumber"].startswith("urn:uuid:")

        # Check metadata
        assert data["metadata"]["component"]["name"] == "fraiseql"
        assert data["metadata"]["component"]["version"] == "1.5.0"

        # Check components
        assert len(data["components"]) == 1
        assert data["components"][0]["name"] == "fastapi"
        assert data["components"][0]["version"] == "0.115.12"
        assert data["components"][0]["purl"] == "pkg:pypi/fastapi@0.115.12"
        assert data["components"][0]["licenses"][0]["license"]["id"] == "MIT"

    def test_to_json_with_supplier(self) -> None:
        """Test JSON serialization with supplier information."""
        supplier = Supplier(
            name="Evolution Digitale",
            url="https://fraiseql.com",
            contact="security@fraiseql.com",
        )

        sbom = SBOM(
            component_name="fraiseql",
            component_version="1.5.0",
            supplier=supplier,
        )

        adapter = CycloneDXAdapter()
        json_str = adapter.to_json(sbom)
        data = json.loads(json_str)

        assert data["metadata"]["component"]["supplier"]["name"] == "Evolution Digitale"
        assert data["metadata"]["component"]["supplier"]["url"] == ["https://fraiseql.com"]

    def test_to_json_with_hashes(self) -> None:
        """Test JSON serialization with cryptographic hashes."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        identifier = ComponentIdentifier(
            name="package",
            version="1.0.0",
            purl="pkg:pypi/package@1.0.0",
        )
        component = Component(identifier=identifier)
        component.add_hash(Hash(algorithm=HashAlgorithm.SHA256, value="abc123def456"))
        component.add_license(License(id="MIT", name="MIT"))
        sbom.add_component(component)

        adapter = CycloneDXAdapter()
        json_str = adapter.to_json(sbom)
        data = json.loads(json_str)

        hashes = data["components"][0]["hashes"]
        assert len(hashes) == 1
        assert hashes[0]["alg"] == "SHA-256"
        assert hashes[0]["content"] == "abc123def456"

    def test_to_json_with_external_references(self) -> None:
        """Test JSON serialization with external references."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        identifier = ComponentIdentifier(
            name="package",
            version="1.0.0",
            purl="pkg:pypi/package@1.0.0",
        )
        component = Component(identifier=identifier)
        component.external_references["homepage"] = "https://example.com"
        component.add_license(License(id="MIT", name="MIT"))
        sbom.add_component(component)

        adapter = CycloneDXAdapter()
        json_str = adapter.to_json(sbom)
        data = json.loads(json_str)

        ext_refs = data["components"][0]["externalReferences"]
        assert len(ext_refs) == 1
        assert ext_refs[0]["type"] == "homepage"
        assert ext_refs[0]["url"] == "https://example.com"

    def test_to_json_with_authors(self) -> None:
        """Test JSON serialization with author information."""
        sbom = SBOM(
            component_name="fraiseql",
            component_version="1.5.0",
            authors=["Lionel Hamayon", "Contributor Name"],
        )

        adapter = CycloneDXAdapter()
        json_str = adapter.to_json(sbom)
        data = json.loads(json_str)

        authors = data["metadata"]["authors"]
        assert len(authors) == 2
        assert authors[0]["name"] == "Lionel Hamayon"
        assert authors[1]["name"] == "Contributor Name"

    def test_json_structure_validity(self) -> None:
        """Test that generated JSON matches CycloneDX schema structure."""
        sbom = SBOM(component_name="fraiseql", component_version="1.5.0")

        # Add multiple components
        for i in range(3):
            identifier = ComponentIdentifier(
                name=f"package{i}",
                version=f"1.0.{i}",
                purl=f"pkg:pypi/package{i}@1.0.{i}",
            )
            component = Component(identifier=identifier)
            component.add_license(License(id="MIT", name="MIT License"))
            sbom.add_component(component)

        adapter = CycloneDXAdapter()
        json_str = adapter.to_json(sbom)
        data = json.loads(json_str)

        # Validate required top-level fields
        required_fields = [
            "bomFormat",
            "specVersion",
            "serialNumber",
            "version",
            "metadata",
            "components",
        ]
        for field in required_fields:
            assert field in data, f"Missing required field: {field}"

        # Validate component structure
        for comp in data["components"]:
            assert "bom-ref" in comp
            assert "type" in comp
            assert "name" in comp
            assert "version" in comp
            assert "purl" in comp

    def test_xml_not_implemented(self) -> None:
        """Test that XML serialization raises NotImplementedError."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        adapter = CycloneDXAdapter()
        with pytest.raises(NotImplementedError, match="XML serialization not yet implemented"):
            adapter.to_xml(sbom)

    def test_from_json_not_implemented(self) -> None:
        """Test that deserialization raises NotImplementedError."""
        with pytest.raises(NotImplementedError, match="deserialization not yet implemented"):
            CycloneDXAdapter.from_json('{"bomFormat": "CycloneDX"}')
