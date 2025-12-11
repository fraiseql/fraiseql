"""Unit tests for field name conversion helper method.

This module tests the _convert_field_name_to_database() helper method
that converts GraphQL camelCase field names to database snake_case names.

Note: Automatic field name conversion in WHERE clauses is not yet implemented.
These tests verify only the helper method itself.
"""

import pytest
from unittest.mock import MagicMock

from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository


class TestFieldNameMapping:
    """Test field name conversion helper method."""

    def setup_method(self) -> None:
        """Set up test repository with mock pool."""
        self.mock_pool = MagicMock(spec=AsyncConnectionPool)
        self.repo = FraiseQLRepository(self.mock_pool)

    def test_empty_where_clause_handling(self) -> None:
        """Empty WHERE clauses should raise ValueError."""
        with pytest.raises(ValueError, match="WHERE clause cannot be empty dict"):
            self.repo._normalize_where({}, "test_view", {"status"})

    def test_none_field_values_ignored(self) -> None:
        """None values in WHERE clauses should be ignored."""
        where_clause = {
            "ipAddress": {"eq": "192.168.1.1"},  # Valid
            "status": None,  # Should be ignored
            "deviceName": {"contains": None},  # Should be ignored
        }

        clause = self.repo._normalize_where(
            where_clause, "test_view", {"ip_address", "status", "device_name"}
        )
        result, params = clause.to_sql()
        assert result is not None

        sql_str = result.as_string(None)

        # Should contain the valid field (currently not converted)
        assert "ipAddress" in sql_str

        # Should not contain ignored fields
        assert "status" not in sql_str
        assert "deviceName" not in sql_str

    def test_edge_case_field_names(self) -> None:
        """Test edge cases like empty strings and unusual field names."""
        # Test empty field name handling
        assert self.repo._convert_field_name_to_database("") == ""

        # Test single character names
        assert self.repo._convert_field_name_to_database("a") == "a"
        assert self.repo._convert_field_name_to_database("A") == "a"

        # Test numbers in field names
        assert self.repo._convert_field_name_to_database("id1") == "id1"
        assert self.repo._convert_field_name_to_database("apiV2Key") == "api_v2_key"

    def test_method_is_idempotent(self) -> None:
        """Calling the conversion method multiple times should produce the same result."""
        test_cases = ["ipAddress", "ip_address", "deviceName", "status"]

        for field_name in test_cases:
            # First conversion
            first_result = self.repo._convert_field_name_to_database(field_name)

            # Second conversion on the result
            second_result = self.repo._convert_field_name_to_database(first_result)

            # Should be the same
            assert first_result == second_result, f"Method not idempotent for {field_name}"

    def test_camel_case_conversion_examples(self) -> None:
        """Test basic camelCase to snake_case conversion."""
        # Test various conversions
        assert self.repo._convert_field_name_to_database("ipAddress") == "ip_address"
        assert self.repo._convert_field_name_to_database("macAddress") == "mac_address"
        assert self.repo._convert_field_name_to_database("deviceName") == "device_name"
        assert self.repo._convert_field_name_to_database("createdAt") == "created_at"

        # Snake case should be unchanged (idempotent)
        assert self.repo._convert_field_name_to_database("ip_address") == "ip_address"
        assert self.repo._convert_field_name_to_database("status") == "status"

    def test_complex_camel_case_conversions(self) -> None:
        """Test more complex camelCase to snake_case conversions."""
        test_cases = [
            ("ipAddress", "ip_address"),
            ("macAddress", "mac_address"),
            ("deviceName", "device_name"),
            ("createdAt", "created_at"),
            ("updatedAt", "updated_at"),
            ("userId", "user_id"),
            ("organizationId", "organization_id"),
            ("APIKey", "api_key"),  # Multiple capitals
            ("HTTPPort", "http_port"),  # Multiple capitals
            ("XMLData", "xml_data"),  # Multiple capitals
        ]

        for camel_case, expected_snake_case in test_cases:
            result = self.repo._convert_field_name_to_database(camel_case)
            assert result == expected_snake_case, (
                f"Conversion failed: {camel_case} -> {result} "
                f"(expected: {expected_snake_case})"
            )
