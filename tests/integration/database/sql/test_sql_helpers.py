import pytest

"""Tests for SQL helper utilities."""

from fraiseql.utils.sql_helpers import (
    check_field_exists,
    generate_field_update_blocks,
    generate_jsonb_build_object,
    generate_partial_update_checks,
    generate_partial_update_function,
    get_jsonb_field_value,
)

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.mark.unit
class TestSQLHelpers:
    """Test SQL helper functions."""

    def test_generate_partial_update_checks(self) -> None:
        """Test generating CASE statements for partial updates."""
        fields = {"ipAddress": "ip_address", "hostname": "hostname", "macAddress": "mac_address"}

        result = generate_partial_update_checks(fields)

        assert "ip_address = CASE WHEN p_input ? 'ipAddress'" in result
        assert "THEN p_input->>'ipAddress' ELSE ip_address END" in result
        assert "hostname = CASE WHEN p_input ? 'hostname'" in result
        assert "mac_address = CASE WHEN p_input ? 'macAddress'" in result

    def test_generate_field_update_blocks(self) -> None:
        """Test generating individual IF blocks for updates."""
        fields = {"ipAddress": "ip_address", "hostname": "hostname"}

        result = generate_field_update_blocks(fields, "tb_router", "tenant")

        assert "IF p_input ? 'ipAddress' THEN" in result
        assert "UPDATE tenant.tb_router" in result
        assert "SET ip_address = p_input->>'ipAddress'" in result
        assert "v_updated_fields := array_append(v_updated_fields, 'ipAddress')" in result
        assert "IF p_input ? 'hostname' THEN" in result

    def test_generate_jsonb_build_object(self) -> None:
        """Test generating jsonb_build_object calls."""
        fields = {"id": "id", "ipAddress": "ip_address", "hostname": "hostname"}

        result = generate_jsonb_build_object(fields)

        assert "'id', id" in result
        assert "'ipAddress', ip_address" in result
        assert "'hostname', hostname" in result
        assert "jsonb_build_object(" in result

        # Test with table alias
        result_with_alias = generate_jsonb_build_object(fields, "r")
        assert "'id', r.id" in result_with_alias
        assert "'ipAddress', r.ip_address" in result_with_alias

    def test_check_field_exists(self) -> None:
        """Test field existence check generation."""
        # Test without camelCase check
        result = check_field_exists("p_input", "hostname", False)
        assert result == "p_input ? 'hostname'"

        # Test with camelCase check
        result = check_field_exists("p_input", "ip_address", True)
        assert "(p_input ? 'ip_address' OR p_input ? 'ipAddress')" in result

        # Test when field is already camelCase
        result = check_field_exists("p_input", "hostname", True)
        assert result == "p_input ? 'hostname'"

    def test_get_jsonb_field_value(self) -> None:
        """Test JSONB field value extraction."""
        # Test without camelCase check
        result = get_jsonb_field_value("p_input", "hostname", False)
        assert result == "p_input->>'hostname'"

        # Test with camelCase check
        result = get_jsonb_field_value("p_input", "ip_address", True)
        assert "COALESCE(p_input->>'ip_address', p_input->>'ipAddress')" in result

        # Test when field is already camelCase
        result = get_jsonb_field_value("p_input", "hostname", True)
        assert result == "p_input->>'hostname'"

    def test_generate_partial_update_function(self) -> None:
        """Test generating complete partial update function."""
        fields = {"ipAddress": "ip_address", "hostname": "hostname", "location": "location"}

        result = generate_partial_update_function(
            function_name="update_router",
            table_name="tb_router",
            entity_name="Router",
            fields=fields,
            schema="app",
        )

        # Check function signature
        assert "CREATE OR REPLACE FUNCTION app.update_router(" in result
        assert "p_input JSONB" in result
        assert "RETURNS app.mutation_result" in result

        # Check validation
        assert "IF NOT p_input ? 'id' THEN" in result
        assert "'Router ID is required'" in result

        # Check field updates
        assert "IF p_input ? 'ipAddress' THEN" in result
        assert "SET ip_address = p_input->>'ipAddress'" in result

        # Check result building
        assert "'ipAddress', ip_address" in result
        assert "'createdAt', created_at" in result
        assert "'updatedAt', updated_at" in result

        # Check metadata
        assert "'entity', 'router'" in result
        assert "'operation', 'update'" in result

    def test_generate_partial_update_function_no_timestamps(self) -> None:
        """Test generating function without timestamp fields."""
        fields = {"name": "name", "value": "value"}

        result = generate_partial_update_function(
            function_name="update_config",
            table_name="tb_config",
            entity_name="Config",
            fields=fields,
            schema="public",
            include_timestamps=False,
        )

        # Should not include timestamp fields
        assert "'createdAt'" not in result
        assert "'updatedAt'" not in result

        # Should still have the main fields
        assert "'name', name" in result
        assert "'value', value" in result
