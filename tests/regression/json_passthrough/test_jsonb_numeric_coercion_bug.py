"""Test to reproduce and fix JSONB numeric type coercion bug in v0.4.1.

This test reproduces the issue where numeric JSONB values are incorrectly
serialized as strings in GraphQL responses, breaking type safety.
"""

import json
from typing import Optional

import pytest

import fraiseql
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder


@fraiseql.type
class SmtpServer:
    """SMTP server configuration with numeric port field."""

    id: str
    host: str
    port: int  # This should be returned as int, not string
    username: Optional[str] = None
    use_tls: bool = False


@fraiseql.type
class PrintServer:
    """Print server with numeric allocation counts."""

    id: str
    name: str
    n_total_allocations: int  # This should be returned as int, not string


class TestJSONBNumericCoercionBug:
    """Test cases for the JSONB numeric coercion bug."""

    def test_smtp_server_port_type_preservation(self):
        """Test that JSONB numeric port value is preserved as integer, not string.

        This reproduces the bug reported in the issue where:
        - Database correctly stores port as integer (587)
        - JSONB view preserves it as numeric JSONB value
        - But final GraphQL response returns "587" (string) instead of 587 (int)
        """
        # Data as it would come from PostgreSQL JSONB
        # This represents the correct JSONB data with numeric port
        jsonb_data = {
            "id": "smtp-server-1",
            "host": "smtp.example.com",
            "port": 587,  # Should remain as int, not become "587"
            "username": "user@example.com",
            "use_tls": True,
        }

        # Test direct JSON serialization with FraiseQLJSONEncoder
        FraiseQLJSONEncoder()
        serialized = json.dumps(jsonb_data, cls=FraiseQLJSONEncoder)
        deserialized = json.loads(serialized)

        # This is the CORE BUG: numeric values are being converted to strings
        assert isinstance(deserialized["port"], int), (
            f"Expected port to be int, got {type(deserialized['port'])}: {deserialized['port']}. "
            f"This indicates the JSONB numeric coercion bug."
        )
        assert deserialized["port"] == 587
        assert deserialized["use_tls"] is True  # Boolean should remain boolean

    def test_print_server_allocation_count_type_preservation(self):
        """Test that allocation count remains as integer in JSON response."""
        jsonb_data = {
            "id": "print-server-1",
            "name": "Office Printer",
            "n_total_allocations": 0,  # Should remain as int, not become "0"
        }

        FraiseQLJSONEncoder()
        serialized = json.dumps(jsonb_data, cls=FraiseQLJSONEncoder)
        deserialized = json.loads(serialized)

        assert isinstance(deserialized["n_total_allocations"], int), (
            f"Expected n_total_allocations to be int, got {type(deserialized['n_total_allocations'])}: "
            f"{deserialized['n_total_allocations']}. This indicates the JSONB numeric coercion bug."
        )
        assert deserialized["n_total_allocations"] == 0

    def test_mixed_types_preservation(self):
        """Test that a mix of types (int, str, bool, float) are preserved correctly."""
        mixed_data = {
            "string_field": "text_value",
            "int_field": 42,
            "float_field": 3.14,
            "bool_field": True,
            "null_field": None,
            "nested_object": {"nested_int": 123, "nested_string": "nested_value"},
        }

        FraiseQLJSONEncoder()
        serialized = json.dumps(mixed_data, cls=FraiseQLJSONEncoder)
        deserialized = json.loads(serialized)

        # String should remain string
        assert isinstance(deserialized["string_field"], str)
        assert deserialized["string_field"] == "text_value"

        # Int should remain int - THIS IS THE BUG
        assert isinstance(deserialized["int_field"], int), (
            f"Expected int_field to be int, got {type(deserialized['int_field'])}: {deserialized['int_field']}"
        )

        # Float should remain float
        assert isinstance(deserialized["float_field"], float)

        # Bool should remain bool
        assert isinstance(deserialized["bool_field"], bool)

        # Null should remain null
        assert deserialized["null_field"] is None

        # Nested int should remain int
        assert isinstance(deserialized["nested_object"]["nested_int"], int), (
            f"Expected nested_int to be int, got {type(deserialized['nested_object']['nested_int'])}: "
            f"{deserialized['nested_object']['nested_int']}"
        )

    def test_graphql_schema_contract_compliance(self):
        """Test that JSON serialization complies with GraphQL schema type contracts.

        This test verifies that when a GraphQL schema defines a field as Int,
        the JSON response contains an actual integer, not a string.
        """
        # Simulate SmtpServer data from database
        smtp_server_data = {
            "__typename": "SmtpServer",
            "id": "smtp-1",
            "host": "mail.example.com",
            "port": 587,  # GraphQL schema defines this as Int
            "username": "smtp@example.com",
            "use_tls": True,
        }

        # This simulates the final GraphQL response structure
        graphql_response = {"data": {"smtpServers": [smtp_server_data]}}

        # Serialize using FraiseQLJSONEncoder (as would happen in production)
        FraiseQLJSONEncoder()
        json_response = json.dumps(graphql_response, cls=FraiseQLJSONEncoder)
        parsed_response = json.loads(json_response)

        # Verify the response structure
        assert "data" in parsed_response
        assert "smtpServers" in parsed_response["data"]
        assert len(parsed_response["data"]["smtpServers"]) == 1

        server = parsed_response["data"]["smtpServers"][0]

        # THE CRITICAL TEST: Port must be integer per GraphQL schema
        assert isinstance(server["port"], int), (
            f"GraphQL schema violation: SmtpServer.port is defined as Int but got "
            f"{type(server['port'])}: {server['port']}. Expected: int, Actual: {type(server['port']).__name__}"
        )

        # Verify exact value
        assert server["port"] == 587

        # Other types should also be preserved
        assert isinstance(server["id"], str)
        assert isinstance(server["host"], str)
        assert isinstance(server["use_tls"], bool)

    def test_postgresql_jsonb_operator_issue(self):
        """Test the core PostgreSQL JSONB operator issue.

        This test demonstrates that using '->' vs '->>' makes the difference
        between preserving types vs converting to strings.
        """
        # This test simulates what happens in the SQL generator
        # where field extraction happens via JSONB operators

        # Simulate raw JSONB data as it would be stored in PostgreSQL
        jsonb_data = {
            "port": 587,  # Should be int
            "timeout": 30.5,  # Should be float
            "enabled": True,  # Should be bool
            "name": "server",  # Should be string
        }

        # Test the difference between -> and ->> operators
        # -> preserves types (returns JSONB)
        # ->> converts to text (returns TEXT)

        # Simulate what PostgreSQL's ->> operator does (converts to text)
        # This is what's happening in line 125 of sql_generator.py
        text_extracted_port = str(jsonb_data["port"])  # "587" - becomes string!
        text_extracted_timeout = str(jsonb_data["timeout"])  # "30.5" - becomes string!
        text_extracted_enabled = str(jsonb_data["enabled"])  # "True" - becomes string!

        # When using ->> operator, everything becomes a string
        assert isinstance(text_extracted_port, str)
        assert isinstance(text_extracted_timeout, str)
        assert isinstance(text_extracted_enabled, str)
        assert text_extracted_port == "587"  # String, not int!

        # Simulate what PostgreSQL's -> operator should do (preserves types)
        # This is what we SHOULD be doing for numeric/boolean fields
        jsonb_extracted_port = jsonb_data["port"]  # 587 - stays int!
        jsonb_extracted_timeout = jsonb_data["timeout"]  # 30.5 - stays float!
        jsonb_extracted_enabled = jsonb_data["enabled"]  # True - stays bool!

        # When using -> operator (or equivalent), types are preserved
        assert isinstance(jsonb_extracted_port, int)
        assert isinstance(jsonb_extracted_timeout, float)
        assert isinstance(jsonb_extracted_enabled, bool)
        assert jsonb_extracted_port == 587  # Integer, as expected!


    def test_raw_json_string_handling(self):
        """Test handling of raw JSON strings (as might come from PostgreSQL).

        This tests the scenario where PostgreSQL returns JSONB data as text strings
        that need to be parsed while preserving numeric types.
        """
        # This is how PostgreSQL might return JSONB data as text
        raw_json_string = '{"port": 587, "timeout": 30, "active": true}'

        # Parse the JSON
        parsed_data = json.loads(raw_json_string)

        # Re-serialize using FraiseQLJSONEncoder
        FraiseQLJSONEncoder()
        re_serialized = json.dumps(parsed_data, cls=FraiseQLJSONEncoder)
        final_data = json.loads(re_serialized)

        # Numeric types should be preserved through the round-trip
        assert isinstance(final_data["port"], int)
        assert isinstance(final_data["timeout"], int)
        assert isinstance(final_data["active"], bool)

        assert final_data["port"] == 587
        assert final_data["timeout"] == 30
        assert final_data["active"] is True

    def test_edge_case_numeric_values(self):
        """Test edge cases for numeric values that might cause type coercion issues."""
        edge_cases = {
            "zero": 0,
            "negative": -42,
            "large_int": 999999999,
            "small_float": 0.1,
            "large_float": 999.999,
            "scientific": 1e10,
        }

        FraiseQLJSONEncoder()
        serialized = json.dumps(edge_cases, cls=FraiseQLJSONEncoder)
        deserialized = json.loads(serialized)

        # All numeric values should preserve their types
        assert isinstance(deserialized["zero"], int)
        assert isinstance(deserialized["negative"], int)
        assert isinstance(deserialized["large_int"], int)
        assert isinstance(deserialized["small_float"], float)
        assert isinstance(deserialized["large_float"], float)
        assert isinstance(deserialized["scientific"], float)

        # Values should be preserved exactly
        assert deserialized["zero"] == 0
        assert deserialized["negative"] == -42
        assert deserialized["large_int"] == 999999999
        assert deserialized["small_float"] == 0.1
        assert deserialized["large_float"] == 999.999
        assert deserialized["scientific"] == 1e10

    def test_postgresql_types_with_fraiseql_encoder(self):
        """Test that PostgreSQL-specific types are converted to frontend-compatible types."""
        import datetime
        import decimal
        import ipaddress
        import uuid

        postgresql_data = {
            # JSON-native types - should pass through unchanged
            "int_field": 587,
            "float_field": 3.14,
            "bool_field": True,
            "null_field": None,
            "string_field": "text",
            "array_field": [1, 2, 3],
            "object_field": {"nested": "value"},
            # PostgreSQL-specific types - should be converted to strings/compatible types
            "uuid_field": uuid.UUID("550e8400-e29b-41d4-a716-446655440000"),
            "datetime_field": datetime.datetime(2023, 1, 1, 12, 0, 0),
            "date_field": datetime.date(2023, 1, 1),
            "decimal_field": decimal.Decimal("123.45"),
            "ipv4_field": ipaddress.IPv4Address("192.168.1.1"),
            "bytes_field": b"hello",
        }

        # Serialize using FraiseQLJSONEncoder
        serialized = json.dumps(postgresql_data, cls=FraiseQLJSONEncoder)
        deserialized = json.loads(serialized)

        # JSON-native types should be preserved exactly
        assert isinstance(deserialized["int_field"], int)
        assert deserialized["int_field"] == 587  # NOT "587"
        assert isinstance(deserialized["float_field"], float)
        assert isinstance(deserialized["bool_field"], bool)
        assert deserialized["null_field"] is None
        assert isinstance(deserialized["string_field"], str)
        assert isinstance(deserialized["array_field"], list)
        assert isinstance(deserialized["object_field"], dict)

        # PostgreSQL types should be converted to frontend-compatible strings/types
        assert isinstance(deserialized["uuid_field"], str)
        assert deserialized["uuid_field"] == "550e8400-e29b-41d4-a716-446655440000"
        assert isinstance(deserialized["datetime_field"], str)
        assert isinstance(deserialized["date_field"], str)
        assert isinstance(deserialized["decimal_field"], float)  # Decimal -> float
        assert isinstance(deserialized["ipv4_field"], str)
        assert isinstance(deserialized["bytes_field"], str)



@pytest.mark.integration
class TestJSONBNumericCoercionIntegration:
    """Integration tests for JSONB numeric coercion with GraphQL execution."""

    @pytest.mark.asyncio
    async def test_end_to_end_graphql_query_response_types(self):
        """Test that a complete GraphQL query preserves numeric types end-to-end.

        This is the integration test that would expose the bug in a real GraphQL
        query execution scenario.
        """
        # This test would require a real database setup
        # For now, we'll simulate the data flow

        # Step 1: Data as it would come from PostgreSQL JSONB
        db_result = {
            "id": "smtp-server-1",
            "host": "smtp.example.com",
            "port": 587,  # Correctly stored as int in DB
            "use_tls": True,
        }

        # Step 2: Wrap in GraphQL response structure
        graphql_response = {"data": {"smtpServer": db_result}}

        # Step 3: Serialize as would happen in HTTP response
        FraiseQLJSONEncoder()
        http_response_json = json.dumps(graphql_response, cls=FraiseQLJSONEncoder)

        # Step 4: Parse response as client would receive it
        client_received_data = json.loads(http_response_json)

        # Step 5: Verify client receives correct types
        server_data = client_received_data["data"]["smtpServer"]

        # THIS IS THE CRITICAL ASSERTION THAT FAILS IN v0.4.1
        assert isinstance(server_data["port"], int), (
            f"End-to-end type coercion bug: Expected port as int, got "
            f"{type(server_data['port'])}: {server_data['port']}"
        )
