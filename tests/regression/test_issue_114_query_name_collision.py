"""Regression test for Issue #114 additional comment: query name collision.

Issue: https://github.com/evoludigit/fraiseql/issues/114#issuecomment

After fixing the single-record list bug, a new issue was discovered where
`routersCount` query is being resolved as `routers` query, returning a full
list instead of an integer count.

Expected: Query name matching should correctly distinguish between:
- routers (list[Router])
- routers_count / routersCount (int)

Actual (bug):
- routersCount resolves to routers resolver ❌
- Returns list[Router] instead of int
"""

import pytest
from graphql import GraphQLInt, GraphQLList, GraphQLString

from fraiseql import fraise_type, query
from fraiseql.gql.schema_builder import build_fraiseql_schema


@pytest.fixture
def clean_registry() -> None:
    """Clean the schema registry before and after each test."""
    from fraiseql.gql.builders.registry import SchemaRegistry
    from fraiseql.mutations.decorators import clear_mutation_registries

    registry = SchemaRegistry.get_instance()
    registry.clear()
    clear_mutation_registries()

    yield

    # Clear after test
    registry.clear()
    clear_mutation_registries()


class TestIssue114QueryNameCollision:
    """Test that query name matching correctly distinguishes similar names.

    This test verifies that the GraphQL schema builder correctly creates
    distinct fields for queries with similar names (e.g., routers vs routersCount).

    The reported bug was that routersCount was incorrectly mapped to the routers
    resolver instead of the routers_count resolver.
    """

    def test_schema_has_distinct_fields_for_similar_query_names(self, clean_registry) -> None:
        """Schema should have separate fields for routers and routersCount."""

        # Define Router type
        @fraise_type
        class Router:
            id: str
            hostname: str

        # Mock resolvers
        @query
        async def routers(info) -> list[Router]:
            """Returns list of routers."""
            return []

        @query
        async def routers_count(info) -> int:
            """Returns count of routers."""
            return 0

        # Build schema
        schema = build_fraiseql_schema(
            query_types=[Router, routers, routers_count],
            mutation_resolvers=[],
        )

        # Verify schema structure
        query_type = schema.query_type
        assert query_type is not None, "Query type should exist"

        fields = query_type.fields

        # Both fields should exist in the schema
        assert "routers" in fields, "routers field should exist"
        assert "routersCount" in fields, "routersCount field should exist"

        # routers should return list of Router
        routers_field = fields["routers"]
        assert isinstance(routers_field.type, GraphQLList), (
            f"routers should return list, got {type(routers_field.type)}"
        )

        # routersCount should return int
        routers_count_field = fields["routersCount"]
        assert routers_count_field.type == GraphQLInt, (
            f"routersCount should return Int, got {routers_count_field.type}"
        )

    def test_schema_has_distinct_fields_for_prefix_variations(self, clean_registry) -> None:
        """Schema should distinguish between device, devices, devicesCount, deviceStatus."""

        @query
        async def device(info) -> str:
            return ""

        @query
        async def devices(info) -> list[str]:
            return []

        @query
        async def devices_count(info) -> int:
            return 0

        @query
        async def device_status(info) -> str:
            return ""

        schema = build_fraiseql_schema(
            query_types=[device, devices, devices_count, device_status],
            mutation_resolvers=[],
        )

        query_type = schema.query_type
        fields = query_type.fields

        # All four fields should exist and be distinct
        assert "device" in fields, "device field should exist"
        assert "devices" in fields, "devices field should exist"
        assert "devicesCount" in fields, "devicesCount field should exist"
        assert "deviceStatus" in fields, "deviceStatus field should exist"

        # Verify types
        assert fields["device"].type == GraphQLString
        assert isinstance(fields["devices"].type, GraphQLList)
        assert fields["devicesCount"].type == GraphQLInt
        assert fields["deviceStatus"].type == GraphQLString

    def test_snake_case_to_camel_case_naming_is_unique(self, clean_registry) -> None:
        """snake_case names should convert to unique camelCase GraphQL field names."""

        @query
        async def user_profile(info) -> str:
            return ""

        @query
        async def user_profile_count(info) -> int:
            return 0

        schema = build_fraiseql_schema(
            query_types=[user_profile, user_profile_count],
            mutation_resolvers=[],
        )

        query_type = schema.query_type
        fields = query_type.fields

        # Should have both fields with proper camelCase names
        assert "userProfile" in fields, "userProfile field should exist"
        assert "userProfileCount" in fields, "userProfileCount field should exist"

        # Should have correct types
        assert fields["userProfile"].type == GraphQLString
        assert fields["userProfileCount"].type == GraphQLInt
