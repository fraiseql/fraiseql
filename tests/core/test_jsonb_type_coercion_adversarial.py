"""Adversarial test suite for JSONB type coercion fix.

This comprehensive test challenges the fix with complex PostgreSQL types,
edge cases, nested structures, and frontend compatibility scenarios.
Similar to the order by adversarial test pattern.
"""

import datetime
import decimal
import ipaddress
import json
import uuid
from dataclasses import dataclass
from enum import Enum
from typing import Any, Dict

import pytest

from fraiseql.core.ast_parser import FieldPath
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder
from fraiseql.sql.sql_generator import build_sql_query


class ServerStatus(Enum):
    """Test enum for complex serialization."""

    ACTIVE = "active"
    INACTIVE = "inactive"
    MAINTENANCE = "maintenance"


@dataclass
class ComplexDataClass:
    """Test dataclass for complex serialization."""

    name: str
    value: int


class TestJSONBTypeCoercionAdversarial:
    """Adversarial tests for JSONB type coercion with complex PostgreSQL types."""

    def test_postgresql_types_kitchen_sink(self):
        """Test every PostgreSQL type FraiseQL might encounter."""
        # This is the "kitchen sink" test - everything PostgreSQL can throw at us
        kitchen_sink_data = {
            # JSON-native types (should pass through unchanged)
            "json_int": 42,
            "json_float": 3.14159,
            "json_bool_true": True,
            "json_bool_false": False,
            "json_string": "hello",
            "json_null": None,
            "json_array": [1, 2, 3, "mixed", True, None],
            "json_object": {"nested": {"deeply": {"value": 123}}},
            # PostgreSQL scalar types (should be converted to frontend-safe formats)
            "uuid_type": uuid.UUID("550e8400-e29b-41d4-a716-446655440000"),
            "datetime_type": datetime.datetime(2023, 12, 25, 15, 30, 45, 123456),
            "date_type": datetime.date(2023, 12, 25),
            "time_type": datetime.time(15, 30, 45),
            "decimal_type": decimal.Decimal("999999.123456789"),
            "decimal_negative": decimal.Decimal("-123.45"),
            "decimal_zero": decimal.Decimal("0"),
            "ipv4_type": ipaddress.IPv4Address("192.168.1.100"),
            "ipv6_type": ipaddress.IPv6Address("2001:0db8:85a3:0000:0000:8a2e:0370:7334"),
            "bytes_type": b"binary_data_\x00\x01\x02",
            # Python complex types (should be handled gracefully)
            "enum_type": ServerStatus.MAINTENANCE,
            "dataclass_type": ComplexDataClass("test", 456),
            "set_type": {1, 2, 3, 4},  # Sets aren't JSON-serializable
            "tuple_type": (1, 2, "three"),
            # Edge case numerics that might break serialization
            "zero": 0,
            "negative_int": -999999,
            "large_int": 9223372036854775807,  # Max int64
            "small_float": 0.000000001,
            "large_float": 1e308,
            "scientific_notation": 1.23e-10,
            "infinity": float("inf"),
            "negative_infinity": float("-inf"),
            "not_a_number": float("nan"),
            # Nested complex structures
            "complex_nested": {
                "level1": {
                    "uuid_in_nested": uuid.UUID("123e4567-e89b-12d3-a456-426614174000"),
                    "array_of_decimals": [
                        decimal.Decimal("1.1"),
                        decimal.Decimal("2.2"),
                        decimal.Decimal("3.3"),
                    ],
                    "mixed_array": [
                        {"id": uuid.UUID("111e1111-e11b-11d1-a111-111111111111"), "count": 100},
                        {"id": uuid.UUID("222e2222-e22b-22d2-a222-222222222222"), "count": 200},
                    ],
                }
            },
            # Problematic strings that might break JSON
            "empty_string": "",
            "unicode_string": "Hello 🌍 world! 中文 العربية ñoño",
            "json_like_string": '{"fake": "json", "number": 42}',
            "escape_chars": 'Line1\nLine2\tTabbed\r\n"Quoted"\\Backslash',
            # Large data structures
            "large_array": list(range(1000)),
            "deep_nesting": self._create_deep_nested_object(20),
        }

        # Serialize using FraiseQLJSONEncoder
        try:
            serialized = json.dumps(kitchen_sink_data, cls=FraiseQLJSONEncoder)
            deserialized = json.loads(serialized)
        except (TypeError, ValueError, OverflowError) as e:
            pytest.fail(f"FraiseQLJSONEncoder failed to handle complex types: {e}")

        # Test JSON-native type preservation (CRITICAL)
        assert isinstance(deserialized["json_int"], int)
        assert deserialized["json_int"] == 42
        assert isinstance(deserialized["json_float"], float)
        assert isinstance(deserialized["json_bool_true"], bool)
        assert deserialized["json_bool_true"] is True
        assert isinstance(deserialized["json_bool_false"], bool)
        assert deserialized["json_bool_false"] is False

        # Test PostgreSQL type conversion to frontend-safe formats
        assert isinstance(deserialized["uuid_type"], str)
        assert deserialized["uuid_type"] == "550e8400-e29b-41d4-a716-446655440000"
        assert isinstance(deserialized["datetime_type"], str)  # ISO format
        assert isinstance(deserialized["decimal_type"], float)  # Decimal -> float
        assert isinstance(deserialized["ipv4_type"], str)
        assert deserialized["ipv4_type"] == "192.168.1.100"

        # Test edge case numerics
        assert isinstance(deserialized["zero"], int)
        assert deserialized["zero"] == 0
        assert isinstance(deserialized["negative_int"], int)
        assert isinstance(deserialized["large_int"], int)

        # Test special float values
        if deserialized["infinity"] is not None:  # Some JSON encoders handle this differently
            assert deserialized["infinity"] in [None, "Infinity", float("inf")]

        # Test nested structure preservation
        nested = deserialized["complex_nested"]["level1"]
        assert isinstance(nested["uuid_in_nested"], str)
        assert isinstance(nested["array_of_decimals"][0], float)  # Decimal -> float in arrays

        print("✅ Kitchen sink test passed - all PostgreSQL types handled correctly")

    def test_sql_generator_with_complex_paths(self):
        """Test SQL generation with deeply nested and complex field paths."""
        complex_field_paths = [
            # Simple JSON-native fields
            FieldPath(alias="serverId", path=["id"]),
            FieldPath(alias="port", path=["config", "port"]),
            FieldPath(alias="enabled", path=["flags", "enabled"]),
            # Deep nesting
            FieldPath(alias="deepValue", path=["level1", "level2", "level3", "level4", "value"]),
            # Array access
            FieldPath(alias="firstTag", path=["tags", "0"]),
            FieldPath(alias="nestedArrayValue", path=["data", "items", "1", "name"]),
            # Complex mixed paths
            FieldPath(alias="userFirstName", path=["user", "profile", "personal", "firstName"]),
            FieldPath(
                alias="settingsTimeout", path=["config", "database", "connection", "timeout"]
            ),
            FieldPath(
                alias="metricValue", path=["monitoring", "metrics", "cpu", "usage", "percent"]
            ),
            # Edge case field names
            FieldPath(alias="fieldWithDashes", path=["field-with-dashes"]),
            FieldPath(alias="fieldWithNumbers", path=["field123", "sub456"]),
            FieldPath(alias="camelCaseField", path=["camelCase", "nestedCamelCase"]),
            FieldPath(alias="snake_case_field", path=["snake_case", "nested_snake_case"]),
        ]

        query = build_sql_query(
            "complex_table",
            complex_field_paths,
            json_output=True,
            typename="ComplexType",
            raw_json_output=True,
        )

        sql_str = query.as_string(None)

        # Verify type-aware operator selection
        expected_operators = {
            "serverId": "->>",  # id is string
            "port": "->",  # port is numeric
            "enabled": "->",  # enabled is boolean
            "deepValue": "->",  # value could be numeric
            "firstTag": "->>",  # array element, likely string
            "nestedArrayValue": "->>",  # name is string
            "userFirstName": "->>",  # firstName is string
            "settingsTimeout": "->",  # timeout is numeric
            "metricValue": "->>",  # percent as string (could be string representation)
            "fieldWithDashes": "->>",  # generic field, default to string
            "fieldWithNumbers": "->>",  # sub456 likely string
            "camelCaseField": "->>",  # nestedCamelCase likely string
            "snake_case_field": "->>",  # nested_snake_case likely string
        }

        for field_path in complex_field_paths:
            alias = field_path.alias
            expected_operator = expected_operators.get(alias, "->")

            # Construct expected SQL path with correct operator for final field
            path_parts = []
            for i, part in enumerate(field_path.path):
                if i == 0 and len(field_path.path) == 1:  # Single element path
                    path_parts.append(f"data{expected_operator}'{part}'")
                elif i == 0:  # First element of multi-element path
                    path_parts.append(f"data->'{part}'")
                elif i == len(field_path.path) - 1:  # Last part uses type-aware operator
                    path_parts.append(f"{expected_operator}'{part}'")
                else:  # Intermediate parts use -> for navigation
                    path_parts.append(f"->'{part}'")
            expected_sql = "".join(path_parts)

            assert expected_sql in sql_str, (
                f"Field path {field_path.path} (alias: {alias}) should generate {expected_sql} "
                f"but not found in: {sql_str}"
            )

        # Verify typename is included
        assert "'__typename', 'ComplexType'" in sql_str

        # Verify raw JSON output formatting
        assert "::text AS result" in sql_str

        print("✅ Complex SQL path generation uses type-preserving operators")

    def test_graphql_response_structure_adversarial(self):
        """Test complex GraphQL response structures that might break serialization."""
        # Simulate complex nested GraphQL response with mixed types
        adversarial_response = {
            "data": {
                "serverCluster": {
                    "__typename": "ServerCluster",
                    "id": uuid.UUID("01234567-1234-1234-1234-123456789012"),
                    "name": "Production Cluster",
                    "servers": [
                        {
                            "__typename": "Server",
                            "id": uuid.UUID("12345678-1234-1234-1234-123456789012"),
                            "hostname": "prod-web-01.example.com",
                            "port": 443,  # Must stay int
                            "ssl_enabled": True,  # Must stay bool
                            "cpu_usage": 85.7,  # Must stay float
                            "memory_total": 16777216,  # Large int
                            "uptime_seconds": 2592000,  # 30 days in seconds
                            "is_healthy": True,
                            "last_check": datetime.datetime(2023, 12, 25, 10, 30, 0),
                            "config": {
                                "database": {
                                    "host": "db.internal",
                                    "port": 5432,  # Nested int
                                    "ssl": True,  # Nested bool
                                    "timeout": 30.0,  # Nested float
                                    "pool_size": 20,  # Another nested int
                                    "connection_string": "postgresql://user:pass@db:5432/prod",
                                },
                                "cache": {
                                    "enabled": True,
                                    "ttl_seconds": 3600,
                                    "max_memory_mb": 512,
                                    "hit_rate": 0.95,
                                },
                            },
                            "metrics": {
                                "requests_per_second": 1250.5,
                                "error_rate": 0.001,
                                "response_times": [12, 15, 18, 22, 25],  # Array of ints
                                "percentiles": {"p50": 15.0, "p95": 45.2, "p99": 120.8},
                            },
                        },
                        {
                            "__typename": "Server",
                            "id": uuid.UUID("87654321-1234-1234-1234-123456789012"),
                            "hostname": "prod-web-02.example.com",
                            "port": 443,
                            "ssl_enabled": True,
                            "cpu_usage": 78.3,
                            "memory_total": 16777216,
                            "uptime_seconds": 1728000,  # 20 days
                            "is_healthy": False,  # Different bool value
                            "last_check": datetime.datetime(2023, 12, 25, 10, 35, 0),
                            "config": {
                                "database": {
                                    "host": "db.internal",
                                    "port": 5432,
                                    "ssl": False,  # Different bool value
                                    "timeout": 45.0,  # Different float
                                    "pool_size": 15,
                                    "connection_string": "postgresql://user2:pass2@db:5432/prod",
                                }
                            },
                            "metrics": {
                                "requests_per_second": 980.2,
                                "error_rate": 0.002,
                                "response_times": [14, 16, 19, 24, 28],
                                "percentiles": {"p50": 16.5, "p95": 48.1, "p99": 125.3},
                            },
                        },
                    ],
                    "load_balancer": {
                        "__typename": "LoadBalancer",
                        "enabled": True,
                        "algorithm": "round_robin",
                        "health_check_interval": 30,  # seconds as int
                        "timeout": 5.0,  # timeout as float
                        "retry_attempts": 3,
                        "sticky_sessions": False,
                        "weights": [0.6, 0.4],  # Array of floats
                        "statistics": {
                            "total_requests": 2500000,  # Large int
                            "successful_requests": 2497500,  # Large int
                            "failed_requests": 2500,  # Int
                            "success_rate": 0.999,  # Float
                            "average_response_time": 18.7,  # Float
                        },
                    },
                    "created_at": datetime.datetime(2023, 1, 1, 0, 0, 0),
                    "updated_at": datetime.datetime(2023, 12, 25, 10, 30, 0),
                    "version": "2.1.3",
                    "maintenance_window": {
                        "enabled": False,
                        "start_hour": 2,  # Int for hour
                        "duration_minutes": 120,  # Int for duration
                        "timezone": "UTC",
                        "days": [0, 6],  # Array of ints (Sunday, Saturday)
                    },
                }
            },
            "errors": None,
            "extensions": {
                "query_complexity": 45,  # Int
                "execution_time_ms": 127.5,  # Float
                "cached": True,  # Bool
                "cache_hit_rate": 0.85,  # Float
            },
        }

        # Serialize the complex structure
        try:
            json_response = json.dumps(adversarial_response, cls=FraiseQLJSONEncoder)
            parsed = json.loads(json_response)
        except Exception as e:
            pytest.fail(f"Failed to serialize complex GraphQL response: {e}")

        # Deep verification of type preservation
        cluster = parsed["data"]["serverCluster"]

        # Root level types
        assert isinstance(cluster["id"], str)  # UUID -> string
        assert isinstance(cluster["name"], str)
        assert isinstance(cluster["created_at"], str)  # datetime -> ISO string

        # Server array - verify each server preserves types
        for i, server in enumerate(cluster["servers"]):
            assert isinstance(server["port"], int), (
                f"Server {i} port should be int, got {type(server['port'])}"
            )
            assert isinstance(server["ssl_enabled"], bool), f"Server {i} ssl_enabled should be bool"
            assert isinstance(server["cpu_usage"], float), f"Server {i} cpu_usage should be float"
            assert isinstance(server["memory_total"], int), f"Server {i} memory_total should be int"
            assert isinstance(server["uptime_seconds"], int), (
                f"Server {i} uptime_seconds should be int"
            )
            assert isinstance(server["is_healthy"], bool), f"Server {i} is_healthy should be bool"

            # Nested config object types
            db_config = server["config"]["database"]
            assert isinstance(db_config["port"], int), f"Server {i} DB port should be int"
            assert isinstance(db_config["ssl"], bool), f"Server {i} DB ssl should be bool"
            assert isinstance(db_config["timeout"], float), f"Server {i} DB timeout should be float"
            assert isinstance(db_config["pool_size"], int), f"Server {i} DB pool_size should be int"

            # Metrics with arrays and nested objects
            metrics = server["metrics"]
            assert isinstance(metrics["requests_per_second"], float)
            assert isinstance(metrics["error_rate"], float)
            assert all(isinstance(rt, int) for rt in metrics["response_times"]), (
                "Response times should be array of ints"
            )

            percentiles = metrics["percentiles"]
            assert isinstance(percentiles["p50"], float)
            assert isinstance(percentiles["p95"], float)
            assert isinstance(percentiles["p99"], float)

        # Load balancer nested types
        lb = cluster["load_balancer"]
        assert isinstance(lb["enabled"], bool)
        assert isinstance(lb["health_check_interval"], int)
        assert isinstance(lb["timeout"], float)
        assert isinstance(lb["retry_attempts"], int)
        assert isinstance(lb["sticky_sessions"], bool)
        assert all(isinstance(w, float) for w in lb["weights"]), "Weights should be array of floats"

        # Load balancer statistics
        stats = lb["statistics"]
        assert isinstance(stats["total_requests"], int)
        assert isinstance(stats["successful_requests"], int)
        assert isinstance(stats["failed_requests"], int)
        assert isinstance(stats["success_rate"], float)
        assert isinstance(stats["average_response_time"], float)

        # Maintenance window
        mw = cluster["maintenance_window"]
        assert isinstance(mw["enabled"], bool)
        assert isinstance(mw["start_hour"], int)
        assert isinstance(mw["duration_minutes"], int)
        assert all(isinstance(day, int) for day in mw["days"]), "Days should be array of ints"

        # Extensions
        ext = parsed["extensions"]
        assert isinstance(ext["query_complexity"], int)
        assert isinstance(ext["execution_time_ms"], float)
        assert isinstance(ext["cached"], bool)
        assert isinstance(ext["cache_hit_rate"], float)

        print("✅ Complex nested GraphQL response preserves all types correctly")
        print(f"   Verified {len(cluster['servers'])} servers with nested configs and metrics")
        print("   All numeric fields preserved as numbers, not strings")

    def test_edge_case_json_structures(self):
        """Test edge cases that might break JSON serialization."""
        edge_cases = {
            # Empty structures
            "empty_dict": {},
            "empty_list": [],
            "empty_string": "",
            # Null/None variations
            "explicit_null": None,
            "null_in_array": [1, None, 3],
            "null_in_object": {"a": 1, "b": None, "c": 3},
            # Circular reference simulation (should be handled)
            "self_reference": {"name": "parent"},
            # Very large numbers
            "max_safe_integer": 9007199254740991,  # JavaScript MAX_SAFE_INTEGER
            "beyond_safe_integer": 9007199254740992,  # Beyond JavaScript safe integer
            "negative_large": -9007199254740991,
            # Precision edge cases
            "high_precision_float": 0.1234567890123456789,
            "tiny_float": 1e-100,
            "huge_float": 1e100,
            # String edge cases that might break JSON
            "control_chars": "\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f",
            "high_unicode": "𝕌𝕟𝕚𝕔𝕠𝕕𝕖 𝔹𝕠𝕕 \U0001f4a9",
            "json_injection": '{"malicious": "value", "number": 42}',
            "sql_injection": "'; DROP TABLE users; --",
            "script_injection": "<script>alert('xss')</script>",
            # Complex nested arrays
            "mixed_array": [
                1,
                2.5,
                True,
                False,
                None,
                "string",
                [1, 2, [3, 4, [5]]],  # Nested arrays
                {"nested": {"deep": {"value": 42}}},  # Object in array
            ],
            # Complex nested objects
            "mixed_object": {
                "int": 42,
                "float": 3.14,
                "bool": True,
                "null": None,
                "array": [1, 2, 3],
                "nested": {
                    "level2": {
                        "level3": {"int": 99, "float": 2.71, "bool": False, "array": [4, 5, 6]}
                    }
                },
            },
        }

        # Add self-reference to test circular handling - but skip for now
        # (This would need special circular reference handling in the encoder)
        # edge_cases["self_reference"]["self"] = edge_cases["self_reference"]

        # Serialize and test
        try:
            json_result = json.dumps(edge_cases, cls=FraiseQLJSONEncoder)
            parsed = json.loads(json_result)
        except Exception as e:
            pytest.fail(f"Failed to handle edge case structures: {e}")

        # Verify basic structure preservation
        assert isinstance(parsed["empty_dict"], dict)
        assert len(parsed["empty_dict"]) == 0
        assert isinstance(parsed["empty_list"], list)
        assert len(parsed["empty_list"]) == 0

        # Verify null handling
        assert parsed["explicit_null"] is None
        assert parsed["null_in_array"][1] is None
        assert parsed["null_in_object"]["b"] is None

        # Verify large number handling
        assert isinstance(parsed["max_safe_integer"], int)
        assert isinstance(parsed["beyond_safe_integer"], int)

        # Verify precision
        assert isinstance(parsed["high_precision_float"], float)
        assert isinstance(parsed["tiny_float"], float)
        assert isinstance(parsed["huge_float"], float)

        # Verify complex nested structures preserve types
        mixed_obj = parsed["mixed_object"]
        assert isinstance(mixed_obj["int"], int)
        assert isinstance(mixed_obj["float"], float)
        assert isinstance(mixed_obj["bool"], bool)
        assert mixed_obj["null"] is None
        assert isinstance(mixed_obj["array"], list)

        # Deep nesting
        deep = mixed_obj["nested"]["level2"]["level3"]
        assert isinstance(deep["int"], int)
        assert isinstance(deep["float"], float)
        assert isinstance(deep["bool"], bool)
        assert isinstance(deep["array"], list)
        assert all(isinstance(x, int) for x in deep["array"])

        print("✅ All edge case JSON structures handled correctly")

    def test_typescript_interface_compatibility(self):
        """Test that serialized data matches TypeScript interface expectations."""
        # Define data that matches a typical TypeScript interface
        typescript_interface_data = {
            "server": {
                "__typename": "Server",
                "id": "server-123",  # string
                "name": "Production Server",  # string
                "port": 443,  # number (CRITICAL)
                "isActive": True,  # boolean (CRITICAL)
                "cpuUsage": 85.7,  # number (CRITICAL)
                "memoryUsageBytes": 8589934592,  # number (CRITICAL)
                "connectionCount": 150,  # number (CRITICAL)
                "isHealthy": True,  # boolean (CRITICAL)
                "hasSSL": False,  # boolean (CRITICAL)
                "uptime": 2592000.5,  # number (CRITICAL)
                "tags": ["production", "web"],  # string[]
                "createdAt": "2023-01-01T00:00:00Z",  # string (ISO date)
                "updatedAt": None,  # string | null
                "config": {  # object
                    "maxConnections": 1000,  # number (CRITICAL)
                    "timeout": 30.0,  # number (CRITICAL)
                    "enableLogging": True,  # boolean (CRITICAL)
                    "retryAttempts": 3,  # number (CRITICAL)
                    "ssl": {
                        "enabled": True,  # boolean (CRITICAL)
                        "port": 443,  # number (CRITICAL)
                        "certExpiry": "2024-12-31T23:59:59Z",  # string
                    },
                },
                "metrics": {  # object
                    "requestsPerSecond": 1250.75,  # number (CRITICAL)
                    "errorRate": 0.001,  # number (CRITICAL)
                    "responseTimeMs": [12, 15, 18],  # number[] (CRITICAL)
                    "counters": {
                        "total": 1000000,  # number (CRITICAL)
                        "success": 999000,  # number (CRITICAL)
                        "errors": 1000,  # number (CRITICAL)
                    },
                },
            }
        }

        # Serialize using FraiseQLJSONEncoder
        serialized = json.dumps(typescript_interface_data, cls=FraiseQLJSONEncoder)
        deserialized = json.loads(serialized)

        server = deserialized["server"]

        # Test TypeScript interface compatibility - these must be exact types
        typescript_expectations = [
            # Root level fields
            ("id", str),
            ("name", str),
            ("port", int),  # CRITICAL: number in TS
            ("isActive", bool),  # CRITICAL: boolean in TS
            ("cpuUsage", float),  # CRITICAL: number in TS
            ("memoryUsageBytes", int),  # CRITICAL: number in TS
            ("connectionCount", int),  # CRITICAL: number in TS
            ("isHealthy", bool),  # CRITICAL: boolean in TS
            ("hasSSL", bool),  # CRITICAL: boolean in TS
            ("uptime", float),  # CRITICAL: number in TS
            ("tags", list),  # CRITICAL: string[] in TS
            ("createdAt", str),  # CRITICAL: string in TS
            # Nested config object
            ("config.maxConnections", int),  # CRITICAL: number in TS
            ("config.timeout", float),  # CRITICAL: number in TS
            ("config.enableLogging", bool),  # CRITICAL: boolean in TS
            ("config.retryAttempts", int),  # CRITICAL: number in TS
            # Deeply nested SSL config
            ("config.ssl.enabled", bool),  # CRITICAL: boolean in TS
            ("config.ssl.port", int),  # CRITICAL: number in TS
            ("config.ssl.certExpiry", str),  # CRITICAL: string in TS
            # Metrics object
            ("metrics.requestsPerSecond", float),  # CRITICAL: number in TS
            ("metrics.errorRate", float),  # CRITICAL: number in TS
            ("metrics.responseTimeMs", list),  # CRITICAL: number[] in TS
            # Nested counters
            ("metrics.counters.total", int),  # CRITICAL: number in TS
            ("metrics.counters.success", int),  # CRITICAL: number in TS
            ("metrics.counters.errors", int),  # CRITICAL: number in TS
        ]

        # Verify each TypeScript expectation
        for field_path, expected_type in typescript_expectations:
            # Navigate to nested field
            current = server
            parts = field_path.split(".")
            for part in parts:
                current = current[part]

            actual_type = type(current)
            assert isinstance(current, expected_type), (
                f"TypeScript compatibility violation: {field_path} "
                f"expected {expected_type.__name__}, got {actual_type.__name__}: {current}"
            )

        # Verify array element types (critical for TypeScript arrays)
        assert all(isinstance(tag, str) for tag in server["tags"]), (
            "tags array must contain only strings"
        )
        assert all(isinstance(rt, int) for rt in server["metrics"]["responseTimeMs"]), (
            "responseTimeMs must be number[]"
        )

        # Verify null handling (TypeScript string | null)
        assert server["updatedAt"] is None, "null values must remain null, not become strings"

        print("✅ All data matches TypeScript interface expectations")
        print("   - All numeric fields are JavaScript numbers, not strings")
        print("   - All boolean fields are JavaScript booleans, not strings")
        print("   - All arrays contain correctly typed elements")
        print("   - Null values remain null")

    def _create_deep_nested_object(self, depth: int) -> Dict[str, Any]:
        """Create a deeply nested object for testing."""
        if depth <= 0:
            return {
                "value": 42,
                "active": True,
                "rate": 3.14,
                "id": uuid.UUID("00000000-1234-1234-1234-123456789012"),
            }

        return {
            f"level_{depth}": self._create_deep_nested_object(depth - 1),
            "metadata": {
                "depth": depth,
                "timestamp": datetime.datetime.now(),
                "weight": float(depth) * 1.5,
            },
        }


# Run adversarial tests manually if executed directly
if __name__ == "__main__":
    test = TestJSONBTypeCoercionAdversarial()
    print("🧪 Running adversarial JSONB type coercion tests...")

    try:
        test.test_postgresql_types_kitchen_sink()
        test.test_sql_generator_with_complex_paths()
        test.test_graphql_response_structure_adversarial()
        test.test_edge_case_json_structures()
        test.test_typescript_interface_compatibility()

        print("\n🎉 ALL ADVERSARIAL TESTS PASSED!")
        print("The JSONB type coercion fix handles all complex PostgreSQL types correctly.")

    except Exception as e:
        print(f"\n❌ ADVERSARIAL TEST FAILED: {e}")
        raise
