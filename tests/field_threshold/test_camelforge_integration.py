"""Test CamelForge integration with field threshold functionality.

Tests that FraiseQL can wrap jsonb_build_object queries with CamelForge
when field count is below threshold and CamelForge is enabled.
"""

import pytest

from fraiseql.core.ast_parser import FieldPath
from fraiseql.sql.sql_generator import build_sql_query



@pytest.mark.camelforge
class TestCamelForgeIntegration:
    """Test CamelForge integration with field threshold detection."""

    def test_camelforge_enabled_below_threshold(self):
        """Test CamelForge wrapping when field count is below threshold."""
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="ipAddress", path=["ip_address"]),  # camelCase in GraphQL
            FieldPath(alias="identifier", path=["identifier"]),
        ]

        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Should wrap jsonb_build_object with CamelForge function
        assert "turbo.fn_camelforge(" in sql_str
        assert "jsonb_build_object(" in sql_str
        assert "'dns_server'" in sql_str
        assert "data->>'ip_address'" in sql_str  # Should use snake_case for DB

    def test_camelforge_disabled_below_threshold(self):
        """Test normal behavior when CamelForge is disabled."""
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="ipAddress", path=["ip_address"]),
            FieldPath(alias="identifier", path=["identifier"]),
        ]

        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=False,  # Disabled
        )

        sql_str = query.as_string(None)

        # Should NOT wrap with CamelForge
        assert "turbo.fn_camelforge(" not in sql_str
        assert "jsonb_build_object(" in sql_str  # Still use normal jsonb_build_object

    def test_camelforge_enabled_above_threshold(self):
        """Test that CamelForge is NOT used when field count exceeds threshold."""
        # Create 25 fields (above threshold of 20)
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,  # Enabled but should be ignored
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Should fall back to full data column (no CamelForge, no jsonb_build_object)
        assert "turbo.fn_camelforge(" not in sql_str
        assert "jsonb_build_object(" not in sql_str
        assert "SELECT data AS result" in sql_str

    def test_camelforge_without_entity_type_raises_error(self):
        """Test that CamelForge requires entity_type parameter."""
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="name", path=["name"]),
        ]

        with pytest.raises(
            ValueError, match="entity_type is required when camelforge_enabled=True"
        ):
            build_sql_query(
                table="v_dns_server",
                field_paths=field_paths,
                json_output=True,
                field_limit_threshold=20,
                camelforge_enabled=True,
                camelforge_function="turbo.fn_camelforge",
                # Missing entity_type
            )

    def test_camelforge_custom_function_name(self):
        """Test CamelForge with custom function name."""
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="name", path=["name"]),
        ]

        query = build_sql_query(
            table="v_entities",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="custom.my_camelforge",  # Custom function
            entity_type="entity",
        )

        sql_str = query.as_string(None)

        # Should use custom function name
        assert "custom.my_camelforge(" in sql_str
        assert "'entity'" in sql_str

    def test_camelforge_with_raw_json_output(self):
        """Test CamelForge with raw JSON output (::text casting)."""
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="ipAddress", path=["ip_address"]),
        ]

        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            raw_json_output=True,  # Enable raw JSON
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Should cast CamelForge result to text
        assert "turbo.fn_camelforge(" in sql_str
        assert "::text AS result" in sql_str

    def test_camelforge_preserves_field_mapping(self):
        """Test that CamelForge preserves GraphQL -> DB field mapping."""
        field_paths = [
            FieldPath(alias="createdAt", path=["created_at"]),  # camelCase -> snake_case
            FieldPath(alias="ipAddress", path=["ip_address"]),  # camelCase -> snake_case
            FieldPath(alias="nTotalItems", path=["n_total_items"]),  # Number prefix
        ]

        query = build_sql_query(
            table="v_test",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="test_entity",
        )

        sql_str = query.as_string(None)

        # Should use snake_case field names for database access
        assert "data->>'created_at'" in sql_str
        assert "data->>'ip_address'" in sql_str
        assert "data->>'n_total_items'" in sql_str

        # Should pass original GraphQL field names to jsonb_build_object
        assert "'createdAt'" in sql_str
        assert "'ipAddress'" in sql_str
        assert "'nTotalItems'" in sql_str