"""Unit tests for SBOM domain models.

Tests the core domain logic following Domain-Driven Design principles.
"""

import pytest

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


class TestHash:
    """Tests for Hash value object."""

    def test_hash_creation(self) -> None:
        """Test creating a valid hash."""
        hash = Hash(
            algorithm=HashAlgorithm.SHA256,
            value="abc123def456",
        )
        assert hash.algorithm == HashAlgorithm.SHA256
        assert hash.value == "abc123def456"

    def test_hash_string_representation(self) -> None:
        """Test hash string format."""
        hash = Hash(algorithm=HashAlgorithm.SHA256, value="abc123")
        assert str(hash) == "SHA-256:abc123"

    def test_hash_invalid_value(self) -> None:
        """Test hash validation rejects invalid hex values."""
        with pytest.raises(ValueError, match="Invalid hex hash value"):
            Hash(algorithm=HashAlgorithm.SHA256, value="xyz123")  # Not hex

    def test_hash_empty_value(self) -> None:
        """Test hash validation rejects empty values."""
        with pytest.raises(ValueError, match="Hash value cannot be empty"):
            Hash(algorithm=HashAlgorithm.SHA256, value="")

    def test_hash_immutability(self) -> None:
        """Test that Hash is immutable (frozen dataclass)."""
        hash = Hash(algorithm=HashAlgorithm.SHA256, value="abc123")
        with pytest.raises(AttributeError):
            hash.value = "def456"  # type: ignore


class TestLicense:
    """Tests for License value object."""

    def test_license_creation(self) -> None:
        """Test creating a license."""
        license = License(
            id="MIT",
            name="MIT License",
            url="https://opensource.org/licenses/MIT",
        )
        assert license.id == "MIT"
        assert license.name == "MIT License"
        assert license.url == "https://opensource.org/licenses/MIT"

    def test_permissive_license_detection(self) -> None:
        """Test detection of permissive licenses."""
        mit = License(id="MIT", name="MIT License")
        assert mit.is_permissive is True
        assert mit.is_copyleft is False

        apache = License(id="Apache-2.0", name="Apache License 2.0")
        assert apache.is_permissive is True

    def test_copyleft_license_detection(self) -> None:
        """Test detection of copyleft licenses."""
        gpl = License(id="GPL-3.0", name="GNU General Public License v3")
        assert gpl.is_copyleft is True
        assert gpl.is_permissive is False

        # LGPL should NOT be detected as copyleft (more permissive)
        lgpl = License(id="LGPL-3.0", name="GNU Lesser General Public License v3")
        assert lgpl.is_copyleft is False

    def test_license_validation(self) -> None:
        """Test license validation."""
        with pytest.raises(ValueError, match="License ID and name are required"):
            License(id="", name="")


class TestSupplier:
    """Tests for Supplier value object."""

    def test_supplier_creation(self) -> None:
        """Test creating a supplier."""
        supplier = Supplier(
            name="Evolution Digitale",
            url="https://fraiseql.com",
            contact="security@fraiseql.com",
        )
        assert supplier.name == "Evolution Digitale"
        assert supplier.url == "https://fraiseql.com"
        assert supplier.contact == "security@fraiseql.com"

    def test_supplier_validation(self) -> None:
        """Test supplier validation."""
        with pytest.raises(ValueError, match="Supplier name is required"):
            Supplier(name="")


class TestComponentIdentifier:
    """Tests for ComponentIdentifier value object."""

    def test_component_identifier_creation(self) -> None:
        """Test creating a component identifier."""
        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        assert identifier.name == "fastapi"
        assert identifier.version == "0.115.12"
        assert identifier.purl == "pkg:pypi/fastapi@0.115.12"

    def test_component_identifier_string_representation(self) -> None:
        """Test identifier string format."""
        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        assert str(identifier) == "fastapi@0.115.12"

    def test_component_identifier_validation(self) -> None:
        """Test identifier validation."""
        with pytest.raises(ValueError, match="Name, version, and PURL are required"):
            ComponentIdentifier(name="", version="", purl="")

        with pytest.raises(ValueError, match="Invalid PURL format"):
            ComponentIdentifier(
                name="fastapi",
                version="1.0.0",
                purl="invalid-purl",  # Missing pkg: prefix
            )


class TestComponent:
    """Tests for Component entity."""

    def test_component_creation(self) -> None:
        """Test creating a component."""
        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        component = Component(identifier=identifier)

        assert component.identifier == identifier
        assert component.type == ComponentType.LIBRARY
        assert len(component.bom_ref) > 0  # UUID generated

    def test_component_add_license(self) -> None:
        """Test adding a license to a component."""
        identifier = ComponentIdentifier(name="test", version="1.0.0", purl="pkg:pypi/test@1.0.0")
        component = Component(identifier=identifier)

        license = License(id="MIT", name="MIT License")
        component.add_license(license)

        assert len(component.licenses) == 1
        assert component.licenses[0] == license

    def test_component_add_duplicate_license(self) -> None:
        """Test that adding duplicate license raises error."""
        identifier = ComponentIdentifier(name="test", version="1.0.0", purl="pkg:pypi/test@1.0.0")
        component = Component(identifier=identifier)

        license = License(id="MIT", name="MIT License")
        component.add_license(license)

        with pytest.raises(ValueError, match="License MIT already exists"):
            component.add_license(license)

    def test_component_add_hash(self) -> None:
        """Test adding a hash to a component."""
        identifier = ComponentIdentifier(name="test", version="1.0.0", purl="pkg:pypi/test@1.0.0")
        component = Component(identifier=identifier)

        hash = Hash(algorithm=HashAlgorithm.SHA256, value="abc123")
        component.add_hash(hash)

        assert len(component.hashes) == 1
        assert component.hashes[0] == hash

    def test_component_add_duplicate_hash_algorithm(self) -> None:
        """Test that adding hash with same algorithm raises error."""
        identifier = ComponentIdentifier(name="test", version="1.0.0", purl="pkg:pypi/test@1.0.0")
        component = Component(identifier=identifier)

        hash1 = Hash(algorithm=HashAlgorithm.SHA256, value="abc123")
        component.add_hash(hash1)

        hash2 = Hash(algorithm=HashAlgorithm.SHA256, value="def456")
        with pytest.raises(ValueError, match="Hash for SHA-256 already exists"):
            component.add_hash(hash2)

    def test_component_license_checks(self) -> None:
        """Test component license type checking."""
        identifier = ComponentIdentifier(name="test", version="1.0.0", purl="pkg:pypi/test@1.0.0")
        component = Component(identifier=identifier)

        mit = License(id="MIT", name="MIT License")
        gpl = License(id="GPL-3.0", name="GNU GPL v3")

        component.add_license(mit)
        assert component.has_permissive_license() is True
        assert component.has_copyleft_license() is False

        component.add_license(gpl)
        assert component.has_copyleft_license() is True


class TestSBOM:
    """Tests for SBOM aggregate root."""

    def test_sbom_creation(self) -> None:
        """Test creating an SBOM."""
        sbom = SBOM(
            component_name="fraiseql",
            component_version="1.5.0",
            component_description="GraphQL framework",
        )

        assert sbom.component_name == "fraiseql"
        assert sbom.component_version == "1.5.0"
        assert sbom.bom_format == "CycloneDX"
        assert sbom.spec_version == "1.5"
        assert len(sbom.serial_number) > 0  # UUID URN

    def test_sbom_add_component(self) -> None:
        """Test adding a component to SBOM."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        component = Component(identifier=identifier)

        sbom.add_component(component)

        assert sbom.component_count() == 1
        assert sbom.components[0] == component

    def test_sbom_add_duplicate_component(self) -> None:
        """Test that adding duplicate component raises error."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        component1 = Component(identifier=identifier)
        component2 = Component(identifier=identifier)

        sbom.add_component(component1)

        with pytest.raises(ValueError, match="already exists"):
            sbom.add_component(component2)

    def test_sbom_find_component(self) -> None:
        """Test finding a component by name and version."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        identifier = ComponentIdentifier(
            name="fastapi",
            version="0.115.12",
            purl="pkg:pypi/fastapi@0.115.12",
        )
        component = Component(identifier=identifier)
        sbom.add_component(component)

        found = sbom.find_component("fastapi", "0.115.12")
        assert found is not None
        assert found.identifier.name == "fastapi"

        not_found = sbom.find_component("nonexistent", "1.0.0")
        assert not_found is None

    def test_sbom_copyleft_detection(self) -> None:
        """Test SBOM-level copyleft detection."""
        sbom = SBOM(component_name="test", component_version="1.0.0")

        # Add component with MIT license
        mit_id = ComponentIdentifier(
            name="mit-package",
            version="1.0.0",
            purl="pkg:pypi/mit-package@1.0.0",
        )
        mit_comp = Component(identifier=mit_id)
        mit_comp.add_license(License(id="MIT", name="MIT"))
        sbom.add_component(mit_comp)

        assert sbom.has_copyleft_components() is False

        # Add component with GPL license
        gpl_id = ComponentIdentifier(
            name="gpl-package",
            version="1.0.0",
            purl="pkg:pypi/gpl-package@1.0.0",
        )
        gpl_comp = Component(identifier=gpl_id)
        gpl_comp.add_license(License(id="GPL-3.0", name="GPL v3"))
        sbom.add_component(gpl_comp)

        assert sbom.has_copyleft_components() is True
        copyleft_comps = sbom.get_copyleft_components()
        assert len(copyleft_comps) == 1
        assert copyleft_comps[0].identifier.name == "gpl-package"

    def test_sbom_validation(self) -> None:
        """Test SBOM validation."""
        # Valid SBOM
        sbom = SBOM(component_name="test", component_version="1.0.0")
        identifier = ComponentIdentifier(
            name="fastapi",
            version="1.0.0",
            purl="pkg:pypi/fastapi@1.0.0",
        )
        component = Component(identifier=identifier)
        component.add_license(License(id="MIT", name="MIT"))
        sbom.add_component(component)

        issues = sbom.validate()
        assert len(issues) == 0

        # Invalid SBOM - missing metadata
        invalid_sbom = SBOM()
        issues = invalid_sbom.validate()
        assert any("component_name is required" in issue for issue in issues)

        # Invalid SBOM - component without license
        no_license_sbom = SBOM(component_name="test", component_version="1.0.0")
        no_license_comp = Component(identifier=identifier)
        no_license_sbom.add_component(no_license_comp)

        issues = no_license_sbom.validate()
        assert any("no license information" in issue for issue in issues)
