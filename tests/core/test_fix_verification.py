"""Verification tests for the JSONB numeric type coercion fix.

This test suite verifies that the fix correctly handles:
1. JSON-native types (int, float, bool) are preserved
2. PostgreSQL-specific types are converted to frontend-compatible formats
3. Frontend TypeScript compatibility is maintained
"""

import datetime
import decimal
import ipaddress
import json
import uuid

from fraiseql.core.ast_parser import FieldPath
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder
from fraiseql.sql.sql_generator import build_sql_query


class TestJSONBFixVerification:
    """Comprehensive verification of the JSONB type coercion fix."""

    def test_sql_generator_uses_type_preserving_operator(self):
        """Verify SQL generator now uses -> operator for type preservation."""
        field_paths = [
            FieldPath(alias="port", path=["config", "port"]),
            FieldPath(alias="timeout", path=["settings", "timeout"]),
            FieldPath(alias="enabled", path=["flags", "enabled"]),
        ]

        query = build_sql_query("servers", field_paths, json_output=True)
        sql_str = query.as_string(None)

        # Should use -> operator (type-preserving) instead of ->> (text-converting)
        assert "data->'config'->'port'" in sql_str
        assert "data->'settings'->'timeout'" in sql_str
        assert "data->'flags'->'enabled'" in sql_str

        # Should NOT contain ->> operators (the old broken behavior)
        assert "->>'port'" not in sql_str
        assert "->>'timeout'" not in sql_str
        assert "->>'enabled'" not in sql_str

        print("✅ SQL generator now uses type-preserving -> operator")

    def test_fraiseql_encoder_preserves_json_native_types(self):
        """Verify FraiseQLJSONEncoder preserves JSON-native types while converting PostgreSQL types."""
        mixed_data = {
            # JSON-native types - should be preserved exactly
            "port": 587,  # int -> int
            "timeout": 30.5,  # float -> float
            "enabled": True,  # bool -> bool
            "disabled": False,  # bool -> bool
            "name": "server",  # string -> string
            "tags": ["web", "api"],  # array -> array
            "config": {"ssl": True},  # object -> object
            "nullable": None,  # null -> null
            # PostgreSQL-specific types - should be converted to frontend-compatible
            "uuid_field": uuid.UUID("550e8400-e29b-41d4-a716-446655440000"),
            "created_at": datetime.datetime(2023, 1, 1, 12, 0, 0),
            "price": decimal.Decimal("19.99"),
            "ip_address": ipaddress.IPv4Address("192.168.1.1"),
        }

        # Serialize using FraiseQLJSONEncoder
        json_string = json.dumps(mixed_data, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_string)

        # JSON-native types should be preserved exactly (CRITICAL for frontend)
        assert isinstance(parsed["port"], int)
        assert parsed["port"] == 587  # NOT "587"

        assert isinstance(parsed["timeout"], float)
        assert parsed["timeout"] == 30.5  # NOT "30.5"

        assert isinstance(parsed["enabled"], bool)
        assert parsed["enabled"] is True  # NOT "True"

        assert isinstance(parsed["disabled"], bool)
        assert parsed["disabled"] is False  # NOT "False"

        assert isinstance(parsed["name"], str)
        assert isinstance(parsed["tags"], list)
        assert isinstance(parsed["config"], dict)
        assert parsed["nullable"] is None

        # PostgreSQL types should be frontend-compatible
        assert isinstance(parsed["uuid_field"], str)
        assert isinstance(parsed["created_at"], str)  # ISO string
        assert isinstance(parsed["price"], float)  # Decimal -> float
        assert isinstance(parsed["ip_address"], str)

        print(
            "✅ FraiseQLJSONEncoder correctly preserves JSON types while converting PostgreSQL types"
        )

    def test_end_to_end_graphql_response_simulation(self):
        """Simulate a complete GraphQL response to verify end-to-end type preservation."""
        # Simulate data as it would come from PostgreSQL JSONB with mixed types
        server_data = {
            "__typename": "Server",
            "id": uuid.UUID("550e8400-e29b-41d4-a716-446655440000"),  # PostgreSQL UUID
            "name": "Production Server",  # string
            "port": 443,  # int - CRITICAL: should stay int
            "ssl_enabled": True,  # bool - CRITICAL: should stay bool
            "timeout_seconds": 30.0,  # float - CRITICAL: should stay float
            "connection_limit": 1000,  # int - CRITICAL: should stay int
            "uptime_percent": 99.9,  # float - CRITICAL: should stay float
            "maintenance_mode": False,  # bool - CRITICAL: should stay bool
            "created_at": datetime.datetime(2023, 1, 1, 12, 0, 0),  # PostgreSQL datetime
            "last_restart": None,  # null
            "tags": ["production", "web"],  # array
            "config": {"ssl": True, "port": 443},  # object with mixed types
        }

        # Simulate full GraphQL response structure
        graphql_response = {"data": {"server": server_data}}

        # Serialize using FraiseQLJSONEncoder (as happens in production)
        response_json = json.dumps(graphql_response, cls=FraiseQLJSONEncoder)

        # Parse as frontend would receive it
        client_data = json.loads(response_json)
        server = client_data["data"]["server"]

        # CRITICAL ASSERTIONS: JSON-native types must be preserved for TypeScript
        assert isinstance(server["port"], int), f"port should be int, got {type(server['port'])}"
        assert server["port"] == 443, "port value should be preserved"

        assert isinstance(server["ssl_enabled"], bool), (
            f"ssl_enabled should be bool, got {type(server['ssl_enabled'])}"
        )
        assert server["ssl_enabled"] is True, "boolean value should be preserved"

        assert isinstance(server["timeout_seconds"], float), (
            f"timeout_seconds should be float, got {type(server['timeout_seconds'])}"
        )
        assert server["timeout_seconds"] == 30.0, "float value should be preserved"

        assert isinstance(server["connection_limit"], int), (
            f"connection_limit should be int, got {type(server['connection_limit'])}"
        )
        assert isinstance(server["uptime_percent"], float), (
            f"uptime_percent should be float, got {type(server['uptime_percent'])}"
        )
        assert isinstance(server["maintenance_mode"], bool), (
            f"maintenance_mode should be bool, got {type(server['maintenance_mode'])}"
        )

        # Nested object types should also be preserved
        assert isinstance(server["config"]["ssl"], bool)
        assert isinstance(server["config"]["port"], int)

        # PostgreSQL types should be frontend-compatible
        assert isinstance(server["id"], str), "UUID should be converted to string"
        assert isinstance(server["created_at"], str), "datetime should be converted to ISO string"

        print("✅ End-to-end GraphQL response preserves JSON types correctly")
        print(f"   Server port: {server['port']} ({type(server['port'])})")
        print(f"   SSL enabled: {server['ssl_enabled']} ({type(server['ssl_enabled'])})")
        print(f"   Timeout: {server['timeout_seconds']} ({type(server['timeout_seconds'])})")

    def test_typescript_compatibility(self):
        """Verify that the response types are compatible with TypeScript expectations."""
        # Types that TypeScript can handle natively
        typescript_compatible_response = {
            "data": {
                "smtpServer": {
                    "__typename": "SmtpServer",
                    "id": "smtp-1",  # string ✅
                    "host": "smtp.example.com",  # string ✅
                    "port": 587,  # number ✅ (NOT "587" ❌)
                    "use_tls": True,  # boolean ✅ (NOT "True" ❌)
                    "timeout": 30.5,  # number ✅ (NOT "30.5" ❌)
                    "connection_count": 0,  # number ✅ (NOT "0" ❌)
                    "settings": {  # object ✅
                        "retry_attempts": 3,  # number ✅
                        "ssl_verify": False,  # boolean ✅
                    },
                    "supported_ports": [25, 465, 587],  # Array<number> ✅
                    "created_at": "2023-01-01T12:00:00",  # string (ISO) ✅
                    "last_used": None,  # null ✅
                }
            }
        }

        # Serialize and verify types
        response_json = json.dumps(typescript_compatible_response, cls=FraiseQLJSONEncoder)
        parsed = json.loads(response_json)

        server = parsed["data"]["smtpServer"]

        # These types work in TypeScript without any conversion
        typescript_types = {
            "id": str,
            "host": str,
            "port": int,  # CRITICAL: Must be int, not str
            "use_tls": bool,  # CRITICAL: Must be bool, not str
            "timeout": float,  # CRITICAL: Must be float, not str
            "connection_count": int,  # CRITICAL: Must be int, not str
            "settings": dict,
            "supported_ports": list,
            "created_at": str,  # ISO string format
        }

        for field, expected_type in typescript_types.items():
            actual_type = type(server[field])
            assert isinstance(server[field], expected_type), (
                f"TypeScript compatibility broken: {field} should be {expected_type.__name__}, "
                f"got {actual_type.__name__}: {server[field]}"
            )

        # Verify nested object types
        settings = server["settings"]
        assert isinstance(settings["retry_attempts"], int)
        assert isinstance(settings["ssl_verify"], bool)

        # Verify array element types
        ports = server["supported_ports"]
        assert all(isinstance(port, int) for port in ports)

        print("✅ Response is fully TypeScript-compatible")
        print("   All numeric fields are actual numbers, not strings")
        print("   All boolean fields are actual booleans, not strings")


if __name__ == "__main__":
    # Run verification manually
    test = TestJSONBFixVerification()
    test.test_sql_generator_uses_type_preserving_operator()
    test.test_fraiseql_encoder_preserves_json_native_types()
    test.test_end_to_end_graphql_response_simulation()
    test.test_typescript_compatibility()
    print("\n🎉 All verification tests passed! The JSONB numeric type coercion bug has been fixed.")
