"""End-to-end integration test for IP filtering (TDD Red Cycle).

This test reproduces the exact production issue described in the IP filtering guide:
GraphQL queries with IP address equality should return results when data exists.
"""

import pytest

from fraiseql.sql.where import build_where_clause, detect_field_type

pytestmark = pytest.mark.database


class TestEndToEndIPFiltering:
    """Integration tests for complete IP filtering workflow."""

    def test_graphql_ip_equality_reproduces_production_bug(self) -> None:
        """Should reproduce the exact production bug for IP equality filtering.

        This test simulates the GraphQL query:
        dnsServers(where: { ipAddress: { eq: "21.43.63.2" } })

        The bug was that this returned 0 results despite the IP existing.
        """
        # Red cycle - this will fail initially because we haven't implemented the clean architecture yet

        # Simulate GraphQL where input
        graphql_where = {"ipAddress": {"eq": "21.43.63.2"}}

        # This should detect the field as IP address type
        field_type = detect_field_type("ipAddress", "21.43.63.2", None)
        assert field_type.is_ip_address()

        # This should build proper SQL with inet casting
        sql_result = build_where_clause(graphql_where)
        sql_str = sql_result.as_string(None)

        # Critical assertions - should generate proper inet casting
        assert "::inet" in sql_str, "Missing inet casting - this was the production bug!"
        assert "data ->> 'ip_address'" in sql_str, "Should convert camelCase to snake_case"
        assert "::inet = '21.43.63.2'::inet" in sql_str, "Should cast value to inet"

        # Should NOT use the problematic host() function
        assert "host(" not in sql_str, "Should not use host() function that strips CIDR"

    def test_ip_filtering_with_list_values(self) -> None:
        """Should handle IP filtering with IN/NOT IN operators."""
        # Red cycle - this will fail initially

        graphql_where = {"serverIp": {"in": ["192.168.1.1", "10.0.0.1", "172.16.0.1"]}}

        sql_result = build_where_clause(graphql_where)
        sql_str = sql_result.as_string(None)

        # Should handle list of IPs with proper inet casting
        assert "data ->> 'server_ip'" in sql_str
        assert "IN (" in sql_str
        assert "'192.168.1.1'::inet" in sql_str
        assert "'10.0.0.1'::inet" in sql_str
        assert "'172.16.0.1'::inet" in sql_str

    def test_network_specific_operators(self) -> None:
        """Should handle network-specific operators like inSubnet."""
        # Red cycle - this will fail initially

        graphql_where = {"ipAddress": {"inSubnet": "192.168.1.0/24"}}

        sql_result = build_where_clause(graphql_where)
        sql_str = sql_result.as_string(None)

        # Should use PostgreSQL subnet containment operator
        assert "<<=" in sql_str
        assert "'192.168.1.0/24'::inet" in sql_str

    def test_mixed_field_types_in_where_clause(self) -> None:
        """Should handle mixed field types correctly in same where clause."""
        # Red cycle - this will fail initially

        graphql_where = {
            "ipAddress": {"eq": "192.168.1.1"},  # IP field
            "name": {"contains": "server"},  # String field
            "port": {"eq": 80},  # Integer field
        }

        sql_result = build_where_clause(graphql_where)
        sql_str = sql_result.as_string(None)

        # IP field should use inet casting
        assert "data ->> 'ip_address'" in sql_str
        assert "::inet = '192.168.1.1'::inet" in sql_str

        # String field should use LIKE
        assert "data ->> 'name' LIKE '%server%'" in sql_str

        # Integer field should use numeric casting
        assert "(data ->> 'port')::numeric = 80" in sql_str

    def test_ipv6_filtering(self) -> None:
        """Should handle IPv6 addresses correctly."""
        # Red cycle - this will fail initially

        graphql_where = {"ipv6Address": {"eq": "2001:db8::1"}}

        sql_result = build_where_clause(graphql_where)
        sql_str = sql_result.as_string(None)

        # Should detect IPv6 and use inet casting
        assert "data ->> 'ipv6_address'" in sql_str
        assert "::inet = '2001:db8::1'::inet" in sql_str

    def test_cidr_network_filtering(self) -> None:
        """Should handle CIDR network addresses correctly."""
        # Red cycle - this will fail initially

        graphql_where = {"network": {"eq": "10.0.0.0/8"}}

        sql_result = build_where_clause(graphql_where)
        sql_str = sql_result.as_string(None)

        # Should detect CIDR and use inet casting
        assert "data ->> 'network'" in sql_str
        assert "::inet = '10.0.0.0/8'::inet" in sql_str

    def test_field_name_conversion_snake_to_camel(self) -> None:
        """Should convert GraphQL camelCase field names to database snake_case."""
        # Red cycle - this will fail initially

        camel_to_snake_cases = [
            ("ipAddress", "ip_address"),
            ("serverIp", "server_ip"),
            ("gatewayIp", "gateway_ip"),
            ("hostIp", "host_ip"),
        ]

        for camel_case, snake_case in camel_to_snake_cases:
            graphql_where = {camel_case: {"eq": "192.168.1.1"}}
            sql_result = build_where_clause(graphql_where)
            sql_str = sql_result.as_string(None)

            # Should use snake_case in SQL
            assert f"(data ->> '{snake_case}')" in sql_str
            assert camel_case not in sql_str  # Should NOT use camelCase in SQL
