import pytest

"""Complete example demonstrating CamelForge integration.

This test shows the exact flow described in the original feature request:
GraphQL queries with low field counts use CamelForge for database-native
camelCase transformation, while high field counts fall back to standard processing.
"""

from fraiseql.core.ast_parser import FieldPath
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.sql.sql_generator import build_sql_query


@pytest.mark.camelforge
class TestCamelForgeCompleteExample:
    """Complete example of CamelForge integration matching the feature request."""

    def test_holy_grail_architecture_low_field_count(self):
        """Test the 'Holy Grail' architecture for low field count queries.

        This matches the exact desired behavior from the feature request:
        GraphQL Query: { dnsServers { id, identifier, ipAddress } }  # 3 fields
        →  FraiseQL detects: "Low field count, can use selective CamelForge"
        →  FraiseQL generates: turbo.fn_camelforge(jsonb_build_object(...), 'dns_server')
        →  CamelForge returns: {"id": "uuid", "identifier": "dns-01", "ipAddress": "192.168.1.1"}
        """
        # Simulate GraphQL query: { dnsServers { id, identifier, ipAddress } }
        field_paths = [
            FieldPath(alias="id", path=["id"]),  # id (no transformation)
            FieldPath(alias="identifier", path=["identifier"]),  # identifier (no transformation)
            FieldPath(alias="ipAddress", path=["ip_address"]),  # ipAddress → ip_address
        ]

        # Configure CamelForge settings as described in feature request
        # Note: CamelForge is always enabled in v0.11.0+
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_function="turbo.fn_camelforge",
            camelforge_field_threshold=32000,  # PostgreSQL parameter limit
            jsonb_field_limit_threshold=20,  # Field threshold
        )

        # Generate SQL with CamelForge integration (always enabled in v0.11.0+)
        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=config.jsonb_field_limit_threshold,
            camelforge_enabled=True,  # Always enabled in v0.11.0+
            camelforge_function=config.camelforge_function,
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Verify the exact SQL structure from the feature request
        assert "turbo.fn_camelforge(" in sql_str
        assert "jsonb_build_object(" in sql_str
        assert "'dns_server'" in sql_str

        # Verify field mapping: GraphQL camelCase → database snake_case
        assert "data->>'ip_address'" in sql_str  # Not ipAddress
        assert "data->>'identifier'" in sql_str
        assert "data->>'id'" in sql_str

        # Verify GraphQL field names are preserved in jsonb_build_object
        assert "'ipAddress'" in sql_str  # GraphQL field name
        assert "'identifier'" in sql_str
        assert "'id'" in sql_str


    def test_holy_grail_architecture_high_field_count(self):
        """Test graceful degradation for high field count queries.

        This matches the fallback behavior from the feature request:
        GraphQL Query: { dnsServers { id, identifier, ipAddress, ...50 more fields } }
        →  FraiseQL detects: "High field count, PostgreSQL parameter limit exceeded"
        →  FraiseQL generates: SELECT data FROM v_dns_server WHERE tenant_id = $1
        →  Standard GraphQL processing with Python field filtering
        """
        # Simulate GraphQL query with many fields (above threshold)
        field_paths = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(25)]

        # Configure with field threshold (CamelForge always enabled in v0.11.0+)
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_function="turbo.fn_camelforge",
            jsonb_field_limit_threshold=20,  # 25 fields > 20 threshold
        )

        # Generate SQL - should fall back to full data column due to field count
        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=config.jsonb_field_limit_threshold,
            camelforge_enabled=True,  # Always enabled in v0.11.0+
            camelforge_function=config.camelforge_function,
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # Verify fallback to standard behavior (no CamelForge)
        assert "turbo.fn_camelforge(" not in sql_str
        assert "jsonb_build_object(" not in sql_str
        assert "SELECT data AS result" in sql_str


    def test_performance_characteristics(self):
        """Test performance characteristics mentioned in the feature request.

        Benefits claimed:
        - Sub-millisecond responses via database-native transformation
        - Zero Python object instantiation overhead
        - Automatic camelCase conversion without manual configuration
        - Perfect TurboRouter integration for cached queries
        """
        # Small query (should use CamelForge)
        small_fields = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="createdAt", path=["created_at"]),
            FieldPath(alias="ipAddress", path=["ip_address"]),
        ]

        small_query = build_sql_query(
            table="v_dns_server",
            field_paths=small_fields,
            json_output=True,
            raw_json_output=True,  # For maximum performance
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        small_sql = small_query.as_string(None)

        # Should use CamelForge with raw JSON output for maximum performance
        assert "turbo.fn_camelforge(" in small_sql
        assert "::text AS result" in small_sql  # Raw JSON casting

        # Large query (should fall back)
        large_fields = [FieldPath(alias=f"field{i}", path=[f"field{i}"]) for i in range(100)]

        large_query = build_sql_query(
            table="v_dns_server",
            field_paths=large_fields,
            json_output=True,
            raw_json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        large_sql = large_query.as_string(None)

        # Should fall back to full data column
        assert "turbo.fn_camelforge(" not in large_sql
        assert "SELECT data::text AS result" in large_sql


    def test_backward_compatibility(self):
        """Test backward compatibility guarantees from the feature request.

        v0.11.0 Changes:
        - CamelForge is now always enabled (removed camelforge_enabled flag)
        - Existing queries continue working with automatic CamelForge optimization
        - Zero Breaking Changes: Queries produce correct results, just faster
        """
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="name", path=["name"]),
        ]

        # Default configuration (CamelForge always enabled in v0.11.0+)
        default_config = FraiseQLConfig(database_url="postgresql://test@localhost/test")
        # Verify config has CamelForge settings
        assert default_config.camelforge_function == "turbo.fn_camelforge"
        assert default_config.camelforge_field_threshold == 20

        # Test that CamelForge is used when enabled and entity type is provided
        enabled_query = build_sql_query(
            table="v_entity",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,  # Explicitly enabled
            camelforge_function="turbo.fn_camelforge",
            entity_type="entity",
        )

        enabled_sql = enabled_query.as_string(None)
        assert "turbo.fn_camelforge(" in enabled_sql

        # Test that CamelForge can be disabled for specific queries if needed
        disabled_query = build_sql_query(
            table="v_entity",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=False,  # Explicitly disabled for this specific query
        )

        disabled_sql = disabled_query.as_string(None)
        assert "turbo.fn_camelforge(" not in disabled_sql
        assert "jsonb_build_object(" in disabled_sql


    def test_success_criteria_validation(self):
        """Validate all success criteria from the feature request.

        Success Criteria:
        1. ✅ Low field count queries use CamelForge-wrapped SQL
        2. ✅ High field count queries use standard processing
        3. ✅ Automatic field mapping from camelCase to snake_case
        4. ✅ JSON passthrough when CamelForge is used
        5. ✅ TurboRouter compatibility with CamelForge queries
        6. ✅ Response time < 1ms for cached CamelForge queries (not testable here)
        """
        # 1. Low field count uses CamelForge
        low_fields = [FieldPath(alias="ipAddress", path=["ip_address"])]
        low_query = build_sql_query(
            table="v_dns_server",
            field_paths=low_fields,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )
        assert "turbo.fn_camelforge(" in low_query.as_string(None)  # ✅ Criterion 1

        # 2. High field count uses standard processing
        high_fields = [FieldPath(alias=f"f{i}", path=[f"f{i}"]) for i in range(25)]
        high_query = build_sql_query(
            table="v_dns_server",
            field_paths=high_fields,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )
        assert "turbo.fn_camelforge(" not in high_query.as_string(None)  # ✅ Criterion 2

        # 3. Automatic field mapping camelCase → snake_case
        mapping_fields = [
            FieldPath(alias="createdAt", path=["created_at"]),  # camelCase → snake_case
            FieldPath(alias="ipAddress", path=["ip_address"]),  # camelCase → snake_case
            FieldPath(alias="nTotalItems", path=["n_total_items"]),  # Number prefix handling
        ]
        mapping_query = build_sql_query(
            table="v_test",
            field_paths=mapping_fields,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="test",
        )
        mapping_sql = mapping_query.as_string(None)
        assert "data->>'created_at'" in mapping_sql  # Database uses snake_case
        assert "data->>'ip_address'" in mapping_sql  # Database uses snake_case
        assert "data->>'n_total_items'" in mapping_sql  # Database uses snake_case
        assert "'createdAt'" in mapping_sql  # GraphQL preserves camelCase
        assert "'ipAddress'" in mapping_sql  # GraphQL preserves camelCase
        assert "'nTotalItems'" in mapping_sql  # GraphQL preserves camelCase
        # ✅ Criterion 3

        # 4. JSON passthrough with raw_json_output
        passthrough_query = build_sql_query(
            table="v_dns_server",
            field_paths=low_fields,
            json_output=True,
            raw_json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )
        assert "::text AS result" in passthrough_query.as_string(None)  # ✅ Criterion 4

        # 5. TurboRouter compatibility (CamelForge works with any function name)
        turbo_query = build_sql_query(
            table="v_dns_server",
            field_paths=low_fields,
            json_output=True,
            field_limit_threshold=20,
            camelforge_enabled=True,
            camelforge_function="turbo.fn_build_dns_server_response",  # TurboRouter function
            entity_type="dns_server",
        )
        assert "turbo.fn_build_dns_server_response(" in turbo_query.as_string(
            None
        )  # ✅ Criterion 5


    def test_example_from_feature_request(self):
        """Test the exact example from the original feature request.

        Current Failing Test:
        query GetDnsServers {
            dnsServers {
                id
                identifier
                ipAddress  # This should work with CamelForge
            }
        }

        Expected Result: {"dnsServers": [{"id": "...", "identifier": "...", "ipAddress": "192.168.1.1"}]}
        """
        # Simulate the exact GraphQL query from the feature request
        field_paths = [
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="identifier", path=["identifier"]),
            FieldPath(alias="ipAddress", path=["ip_address"]),  # The problematic field
        ]

        # Use the exact configuration suggested in the feature request
        query = build_sql_query(
            table="v_dns_server",
            field_paths=field_paths,
            json_output=True,
            field_limit_threshold=32000,  # PostgreSQL parameter limit from feature request
            camelforge_enabled=True,
            camelforge_function="turbo.fn_camelforge",
            entity_type="dns_server",
        )

        sql_str = query.as_string(None)

        # This should now generate the exact SQL structure described in the feature request
        expected_structure = [
            "turbo.fn_camelforge(",  # CamelForge function call
            "jsonb_build_object(",  # Selective field extraction
            "'id', data->>'id'",  # ID field mapping
            "'identifier', data->>'identifier'",  # Identifier field mapping
            "'ipAddress', data->>'ip_address'",  # camelCase → snake_case mapping
            "'dns_server'",  # Entity type parameter
        ]

        for expected in expected_structure:
            assert expected in sql_str, f"Missing expected SQL fragment: {expected}"


        # This SQL would now return: {"id": "uuid", "identifier": "dns-01", "ipAddress": "192.168.1.1"}
        # instead of the previous error: 'DnsServer' object has no attribute 'keys'
