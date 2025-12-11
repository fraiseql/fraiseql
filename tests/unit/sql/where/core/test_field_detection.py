"""Test field detection for IP addresses and vectors (TDD Red Cycle).

These tests focus on the core issue from the IP filtering guide and vector support:
detecting IP address fields, vector fields, and values correctly.
"""

import pytest

from fraiseql.sql.where.core.field_detection import FieldType, detect_field_type


class TestIPAddressFieldDetection:
    """Test IP address field detection functionality."""

    def test_detect_ip_from_field_name_snake_case(self) -> None:
        """Should detect IP fields from snake_case field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("ip_address", "192.168.1.1", None)
        assert result == FieldType.IP_ADDRESS

    def test_detect_ip_from_field_name_camel_case(self) -> None:
        """Should detect IP fields from camelCase field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("ipAddress", "192.168.1.1", None)
        assert result == FieldType.IP_ADDRESS

    def test_detect_ip_from_field_name_variations(self) -> None:
        """Should detect various IP field name patterns."""
        # Red cycle - this will fail initially
        ip_field_names = [
            "server_ip",
            "serverIp",
            "gateway_ip",
            "gatewayIp",
            "host_ip",
            "hostIp",
            "ip",
        ]

        for field_name in ip_field_names:
            result = detect_field_type(field_name, "10.0.0.1", None)
            assert result == FieldType.IP_ADDRESS, f"Failed for field: {field_name}"

    def test_detect_ip_from_value_ipv4(self) -> None:
        """Should detect IP addresses from IPv4 values."""
        # Red cycle - this will fail initially
        ipv4_values = [
            "192.168.1.1",
            "10.0.0.1",
            "172.16.0.1",
            "8.8.8.8",
            "127.0.0.1",
        ]

        for ip_value in ipv4_values:
            result = detect_field_type("some_field", ip_value, None)
            assert result == FieldType.IP_ADDRESS, f"Failed for IP: {ip_value}"

    def test_detect_ip_from_value_ipv6(self) -> None:
        """Should detect IP addresses from IPv6 values."""
        # Red cycle - this will fail initially
        ipv6_values = [
            "2001:db8::1",
            "fe80::1",
            "::1",
            "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
        ]

        for ip_value in ipv6_values:
            result = detect_field_type("some_field", ip_value, None)
            assert result == FieldType.IP_ADDRESS, f"Failed for IPv6: {ip_value}"

    def test_detect_ip_from_value_cidr(self) -> None:
        """Should detect CIDR networks as IP address type."""
        # Red cycle - this will fail initially
        cidr_values = [
            "192.168.1.0/24",
            "10.0.0.0/8",
            "172.16.0.0/12",
            "2001:db8::/32",
        ]

        for cidr_value in cidr_values:
            result = detect_field_type("network", cidr_value, None)
            assert result == FieldType.IP_ADDRESS, f"Failed for CIDR: {cidr_value}"

    def test_detect_non_ip_values(self) -> None:
        """Should NOT detect non-IP values as IP addresses."""
        # Red cycle - this will fail initially
        non_ip_values = [
            "hello",
            "example.com",
            "192.168.1.256",  # Invalid IP
            "not.an.ip.address",
            "12345",
            "",
        ]

        for value in non_ip_values:
            result = detect_field_type("some_field", value, None)
            assert result != FieldType.IP_ADDRESS, f"Incorrectly detected as IP: {value}"

    def test_detect_from_python_type(self) -> None:
        """Should detect IP fields from Python type hints."""
        # Red cycle - this will fail initially
        try:
            from fraiseql.types.scalars.ip_address import IpAddressField

            result = detect_field_type("field", "192.168.1.1", IpAddressField)
            assert result == FieldType.IP_ADDRESS
        except ImportError:
            pytest.skip("IpAddressField not available")

    def test_detect_ip_list_values(self) -> None:
        """Should detect IP addresses in list values."""
        # Red cycle - this will fail initially
        ip_list = ["192.168.1.1", "10.0.0.1", "172.16.0.1"]
        result = detect_field_type("ip_addresses", ip_list, None)
        assert result == FieldType.IP_ADDRESS


class TestVectorFieldDetection:
    """Test vector/embedding field detection functionality."""

    def test_detect_vector_from_field_name_embedding_suffix(self) -> None:
        """Should detect vector fields from embedding field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("embedding", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_detect_vector_from_field_name_text_embedding(self) -> None:
        """Should detect vector fields from text_embedding field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("text_embedding", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_detect_vector_from_field_name_vector_suffix(self) -> None:
        """Should detect vector fields from _vector field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("_vector", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_detect_vector_from_field_name_embedding_vector(self) -> None:
        """Should detect vector fields from embedding_vector field names."""
        # Red cycle - this will fail initially
        result = detect_field_type("embedding_vector", [0.1, 0.2, 0.3], None)
        assert result == FieldType.VECTOR

    def test_vector_vs_array_disambiguation(self) -> None:
        """Should detect vector fields vs regular array fields by name pattern."""
        # Red cycle - this will fail initially
        # Vector field (should be VECTOR)
        vector_result = detect_field_type("embedding", [0.1, 0.2, 0.3], None)
        assert vector_result == FieldType.VECTOR

        # Regular array field (should be ARRAY)
        array_result = detect_field_type("tags", ["tag1", "tag2"], None)
        assert array_result == FieldType.ARRAY

        # Another regular array field (should be ARRAY)
        scores_result = detect_field_type("scores", [1.0, 2.0, 3.0], None)
        assert scores_result == FieldType.ARRAY

    def test_vector_field_type_enum_exists(self) -> None:
        """Should have VECTOR field type in FieldType enum."""
        # Red cycle - this will fail initially
        # This test verifies the enum exists and can be accessed
        assert hasattr(FieldType, "VECTOR")
        assert FieldType.VECTOR.value == "vector"
