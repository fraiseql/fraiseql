"""Regression test for FraiseQL v0.7.16 empty string validation issue.

This test ensures that the v0.7.16 regression where validation was applied
during field resolution (breaking existing data queries) does not reoccur.

Issue: https://github.com/fraiseql/fraiseql/issues/v0716-validation-regression
"""

from typing import Optional

import pytest

import fraiseql


@fraiseql.type
class PrintServer:
    """Output type representing a print server entity."""

    id: str
    hostname: str
    operating_system: str  # Required field that may have empty strings in existing data
    ip_address: str


@fraiseql.input
class CreatePrintServerInput:
    """Input type for creating a print server."""

    hostname: str
    operating_system: str  # Should reject empty strings for new input
    ip_address: str


class TestV0716EmptyStringValidationRegression:
    """Test suite for the v0.7.16 validation regression fix."""

    def test_output_type_can_load_existing_data_with_empty_strings(self) -> None:
        """Test that @fraiseql.type can load existing database records with empty string fields.

        This reproduces the regression where validation was incorrectly applied during
        from_dict() calls, preventing existing data from being queried. The fix ensures
        that validation is only applied to @fraiseql.input types, not output types.
        """
        # Simulate existing database data with empty operating_system field
        existing_data = {
            "id": "test-print-server-001",
            "hostname": "printer01.example.com",
            "operating_system": "",  # Empty string from existing database record
            "ip_address": "192.168.1.100",
        }

        # This should work - output types should load existing data even with empty fields
        print_server = PrintServer.from_dict(existing_data)

        assert print_server.id == "test-print-server-001"
        assert print_server.hostname == "printer01.example.com"
        assert print_server.operating_system == ""  # Empty string preserved
        assert print_server.ip_address == "192.168.1.100"

    def test_output_type_can_load_existing_data_with_whitespace_strings(self) -> None:
        """Test that @fraiseql.type can load existing data with whitespace-only strings."""
        existing_data = {
            "id": "test-print-server-002",
            "hostname": "printer02.example.com",
            "operating_system": "   ",  # Whitespace-only string
            "ip_address": "192.168.1.101",
        }

        # This should work - output types should load whitespace-only strings
        print_server = PrintServer.from_dict(existing_data)

        assert print_server.operating_system == "   "  # Whitespace preserved

    def test_input_type_validation_still_rejects_empty_strings(self) -> None:
        """Test that @fraiseql.input still properly validates empty strings.

        This ensures that the regression fix doesn't break the intended validation
        behavior for new input data.
        """
        # Input validation should still reject empty strings
        with pytest.raises(ValueError, match="Field 'operating_system' cannot be empty"):
            CreatePrintServerInput(
                hostname="printer03.example.com",
                operating_system="",  # Empty string should be rejected
                ip_address="192.168.1.102",
            )

    def test_input_type_validation_rejects_whitespace_only_strings(self) -> None:
        """Test that @fraiseql.input properly validates whitespace-only strings."""
        # Input validation should reject whitespace-only strings
        with pytest.raises(ValueError, match="Field 'operating_system' cannot be empty"):
            CreatePrintServerInput(
                hostname="printer04.example.com",
                operating_system="   ",  # Whitespace-only string should be rejected
                ip_address="192.168.1.103",
            )

    def test_input_type_validation_allows_valid_strings(self) -> None:
        """Test that @fraiseql.input accepts valid non-empty strings."""
        # Valid input should work
        input_obj = CreatePrintServerInput(
            hostname="printer05.example.com",
            operating_system="Linux Ubuntu 22.04",  # Valid non-empty string
            ip_address="192.168.1.104",
        )

        assert input_obj.hostname == "printer05.example.com"
        assert input_obj.operating_system == "Linux Ubuntu 22.04"
        assert input_obj.ip_address == "192.168.1.104"

    def test_organizational_unit_regression_case(self) -> None:
        """Test the specific organizational unit case mentioned in the bug report."""

        @fraiseql.type
        class OrganizationalUnit:
            id: str
            name: str  # May be empty in existing data
            description: Optional[str] = None

        # Simulate the failing case from the bug report
        existing_ou_data = {
            "id": "ou-001",
            "name": "",  # Empty name from existing data
            "description": "Legacy organizational unit",
        }

        # This should work without throwing "Field 'name' cannot be empty"
        ou = OrganizationalUnit.from_dict(existing_ou_data)

        assert ou.id == "ou-001"
        assert ou.name == ""  # Empty name preserved
        assert ou.description == "Legacy organizational unit"

    def test_nested_array_resolution_regression_case(self) -> None:
        """Test the nested array resolution case from the bug report."""

        @fraiseql.type
        class NetworkConfiguration:
            id: str
            name: str
            print_servers: list[PrintServer]

        # Simulate nested data with empty fields
        network_data = {
            "id": "net-config-001",
            "name": "Main Network",
            "print_servers": [
                {
                    "id": "printer-001",
                    "hostname": "printer01",
                    "operating_system": "",  # Empty OS in nested object
                    "ip_address": "192.168.1.100",
                },
                {
                    "id": "printer-002",
                    "hostname": "printer02",
                    "operating_system": "Windows Server 2019",  # Valid OS
                    "ip_address": "192.168.1.101",
                },
            ],
        }

        # This should work without failing on nested empty fields
        network_config = NetworkConfiguration.from_dict(network_data)

        assert network_config.id == "net-config-001"
        assert len(network_config.print_servers) == 2
        assert network_config.print_servers[0].operating_system == ""  # Empty OS preserved
        assert network_config.print_servers[1].operating_system == "Windows Server 2019"
