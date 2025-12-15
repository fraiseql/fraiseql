"""Unit tests for camelCase conversion with problematic patterns.

Tests the Rust functions exported to Python for camelCase conversion
of nested JSONB structures.
"""

import json

import pytest


class TestCamelCaseConversionPatterns:
    """Test camelCase conversion for problematic field patterns."""

    def test_underscore_pattern_to_camelcase(self) -> None:
        """Standard underscore patterns should convert correctly."""
        from fraiseql._fraiseql_rs import to_camel_case

        assert to_camel_case("smtp_server") == "smtpServer"
        assert to_camel_case("print_servers") == "printServers"
        assert to_camel_case("ip_address") == "ipAddress"

    def test_underscore_number_pattern_to_camelcase(self) -> None:
        """Underscore before number should produce correct output."""
        from fraiseql._fraiseql_rs import to_camel_case

        # dns_1 → dns1 (number not capitalized)
        assert to_camel_case("dns_1") == "dns1"
        assert to_camel_case("dns_2") == "dns2"
        assert to_camel_case("backup_1_id") == "backup1Id"
        assert to_camel_case("server_10_name") == "server10Name"

    def test_single_word_unchanged(self) -> None:
        """Single words should remain unchanged."""
        from fraiseql._fraiseql_rs import to_camel_case

        assert to_camel_case("gateway") == "gateway"
        assert to_camel_case("router") == "router"
        assert to_camel_case("id") == "id"

    def test_already_camelcase_unchanged(self) -> None:
        """Already camelCase strings should remain unchanged."""
        from fraiseql._fraiseql_rs import to_camel_case

        assert to_camel_case("smtpServer") == "smtpServer"
        assert to_camel_case("ipAddress") == "ipAddress"

    def test_transform_json_nested_dict(self) -> None:
        """transform_json should convert all nested keys to camelCase."""
        from fraiseql._fraiseql_rs import transform_json

        input_json = json.dumps(
            {
                "id": "123",
                "smtp_server": {"ip_address": "10.0.0.1", "port": 25},
                "dns_1": {"ip_address": "8.8.8.8"},
                "print_servers": [{"host_name": "printer1"}],
            }
        )

        result = transform_json(input_json)
        parsed = json.loads(result)

        # Top-level keys should be camelCase
        assert "smtpServer" in parsed, f"Got keys: {list(parsed.keys())}"
        assert "dns1" in parsed, f"Got keys: {list(parsed.keys())}"
        assert "printServers" in parsed, f"Got keys: {list(parsed.keys())}"

        # Nested keys should be camelCase
        assert "ipAddress" in parsed["smtpServer"]
        assert "ipAddress" in parsed["dns1"]
        assert "hostName" in parsed["printServers"][0]


class TestBuildGraphQLResponse:
    """Test build_graphql_response for nested JSONB handling."""

    def test_nested_object_keys_converted(self) -> None:
        """build_graphql_response should convert nested object keys."""
        from fraiseql._fraiseql_rs import build_graphql_response

        json_string = json.dumps(
            {
                "id": "123",
                "identifier": "test",
                "smtp_server": {"id": "456", "ip_address": "10.0.0.1"},
                "dns_1": {"id": "789", "ip_address": "8.8.8.8"},
            }
        )

        response_bytes = build_graphql_response(
            json_strings=[json_string],
            field_name="networkConfiguration",
            type_name="NetworkConfiguration",
            is_list=False,
        )

        response = json.loads(response_bytes)
        data = response["data"]["networkConfiguration"]

        assert "smtpServer" in data, f"Got keys: {list(data.keys())}"
        assert "ipAddress" in data["smtpServer"]
        assert "dns1" in data, f"Got keys: {list(data.keys())}"
        assert "ipAddress" in data["dns1"]

    def test_array_item_keys_converted(self) -> None:
        """build_graphql_response should convert keys in array items."""
        from fraiseql._fraiseql_rs import build_graphql_response

        json_string = json.dumps(
            {
                "id": "123",
                "print_servers": [
                    {"host_name": "printer1", "ip_address": "10.0.0.1"},
                    {"host_name": "printer2", "ip_address": "10.0.0.2"},
                ],
            }
        )

        response_bytes = build_graphql_response(
            json_strings=[json_string],
            field_name="config",
            type_name="Config",
            is_list=False,
        )

        response = json.loads(response_bytes)
        data = response["data"]["config"]

        assert "printServers" in data, f"Got keys: {list(data.keys())}"
        assert "hostName" in data["printServers"][0]
        assert "ipAddress" in data["printServers"][0]

    def test_deeply_nested_keys_converted(self) -> None:
        """Deeply nested structures should have all keys converted."""
        from fraiseql._fraiseql_rs import build_graphql_response

        json_string = json.dumps(
            {
                "id": "123",
                "network_config": {
                    "primary_dns": {
                        "ip_address": "8.8.8.8",
                        "backup_servers": [{"server_name": "backup1"}],
                    }
                },
            }
        )

        response_bytes = build_graphql_response(
            json_strings=[json_string],
            field_name="data",
            type_name="Data",
            is_list=False,
        )

        response = json.loads(response_bytes)
        data = response["data"]["data"]

        assert "networkConfig" in data
        assert "primaryDns" in data["networkConfig"]
        assert "ipAddress" in data["networkConfig"]["primaryDns"]
        assert "backupServers" in data["networkConfig"]["primaryDns"]
        assert "serverName" in data["networkConfig"]["primaryDns"]["backupServers"][0]


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
