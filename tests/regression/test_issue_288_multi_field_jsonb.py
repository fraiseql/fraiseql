"""Regression tests for issue #288.

Bug: Multi-field query with JSONB types returns empty nested fields.

When a GraphQL query has 2+ root-level fields and at least one resolver uses
a JSONB type (decorated with @fraiseql.type(jsonb_column=...)), the nested
JSONB fields in the response are empty -- only __typename is returned.

Single root-field queries (even with aliases) work correctly.

Root cause: execute_multi_field_query passes field_selections_json (built from
top-level field names only) to build_multi_field_response. Rust's
transform_with_selections then filters nested sub-fields because paths like
"nested.value" are absent from selected_paths (which only contains "nested").

Fix: Pass None for field_selections_json since items are already
field-projected, JSONB-extracted, camelCased, and typed by the first
Rust pass inside db.find().
"""

import json

import fraiseql._fraiseql_rs as fraiseql_rs
import pytest
from graphql import (
    GraphQLField,
    GraphQLList,
    GraphQLNonNull,
    GraphQLObjectType,
    GraphQLSchema,
    GraphQLString,
)

from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.fastapi.routers import execute_multi_field_query


@pytest.fixture(scope="module", autouse=True)
def init_schema_registry_jsonb() -> None:
    """Initialize schema registry with JSONB-style nested types."""
    fraiseql_rs.reset_schema_registry_for_testing()

    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "Nested": {
                "fields": {
                    "value": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                }
            },
            "MyItem": {
                "fields": {
                    "id": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                    "nested": {
                        "type_name": "Nested",
                        "is_nested_object": True,
                        "is_list": False,
                    },
                }
            },
        },
    }

    fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))


@pytest.mark.asyncio
async def test_multi_field_jsonb_nested_fields_populated() -> None:
    """Multi-field query with JSONB types must return populated nested fields.

    Regression test for issue #288.

    Query:
        query GetItems($where1: String, $where2: String) {
            items1: myItems(where: $where1) { nested { value } }
            items2: myItems(where: $where2) { nested { value } }
        }

    Before fix: nested returns {"__typename": "Nested"} (value missing).
    After fix:  nested returns {"__typename": "Nested", "value": "hello"}.
    """
    # Simulate what db.find() returns in multi-field mode (include_graphql_wrapper=False):
    # items are already JSONB-extracted, camelCased, and __typename-injected.
    processed_items = [
        {
            "__typename": "MyItem",
            "id": "abc",
            "nested": {"__typename": "Nested", "value": "hello"},
        }
    ]

    async def my_items_resolver(info, where=None):
        return processed_items

    nested_type = GraphQLObjectType(
        "Nested",
        {
            "value": GraphQLField(GraphQLString),
        },
    )
    my_item_type = GraphQLObjectType(
        "MyItem",
        {
            "id": GraphQLField(GraphQLString),
            "nested": GraphQLField(nested_type),
        },
    )
    query_type = GraphQLObjectType(
        "Query",
        {
            "myItems": GraphQLField(
                GraphQLList(GraphQLNonNull(my_item_type)),
                resolve=my_items_resolver,
            ),
        },
    )
    schema = GraphQLSchema(query=query_type)

    query_string = """
    query GetItems($where1: String, $where2: String) {
        items1: myItems(where: $where1) {
            nested { value }
        }
        items2: myItems(where: $where2) {
            nested { value }
        }
    }
    """

    result = await execute_multi_field_query(schema, query_string, {}, {})

    assert isinstance(result, RustResponseBytes)
    result_json = json.loads(bytes(result))

    assert "data" in result_json, f"Missing data key: {result_json}"
    assert "items1" in result_json["data"], f"Missing items1: {result_json['data']}"
    assert "items2" in result_json["data"], f"Missing items2: {result_json['data']}"

    # Both aliased fields must have the nested value populated
    for alias in ("items1", "items2"):
        items = result_json["data"][alias]
        assert len(items) == 1, f"{alias} should have 1 item"
        nested = items[0].get("nested", {})
        assert nested.get("value") == "hello", (
            f"Issue #288: {alias}[0].nested.value is empty. "
            f"Got nested={nested!r}. "
            "Expected 'hello'. This indicates double JSONB processing in "
            "execute_multi_field_query."
        )


@pytest.mark.asyncio
async def test_single_field_jsonb_nested_fields_unaffected() -> None:
    """Single-field queries with JSONB types continue to work correctly.

    Regression guard: the fix must not break single-field queries.
    """
    processed_items = [
        {
            "__typename": "MyItem",
            "id": "abc",
            "nested": {"__typename": "Nested", "value": "hello"},
        }
    ]

    async def my_items_resolver(info, where=None):
        return processed_items

    nested_type = GraphQLObjectType(
        "Nested",
        {
            "value": GraphQLField(GraphQLString),
        },
    )
    my_item_type = GraphQLObjectType(
        "MyItem",
        {
            "id": GraphQLField(GraphQLString),
            "nested": GraphQLField(nested_type),
        },
    )
    query_type = GraphQLObjectType(
        "Query",
        {
            "myItems": GraphQLField(
                GraphQLList(GraphQLNonNull(my_item_type)),
                resolve=my_items_resolver,
            ),
        },
    )
    schema = GraphQLSchema(query=query_type)

    # Single-field query: goes through execute_multi_field_query only when
    # has_multiple_root_fields=True. Here we still use the executor directly
    # with a single-field query to verify it still works.
    query_string = """
    {
        myItems {
            nested { value }
        }
    }
    """

    result = await execute_multi_field_query(schema, query_string, {}, {})

    assert isinstance(result, RustResponseBytes)
    result_json = json.loads(bytes(result))

    assert "data" in result_json
    items = result_json["data"]["myItems"]
    assert len(items) == 1
    nested = items[0].get("nested", {})
    assert nested.get("value") == "hello"
