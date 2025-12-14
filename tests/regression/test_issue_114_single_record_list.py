"""Regression test for Issue #114: db.find() returns dict instead of list for single record.

Issue: https://github.com/fraiseql/fraiseql/issues/114

When db.find() with a where filter matches exactly one record, it was incorrectly
returning a single dict object instead of a list containing one dict.

Expected: db.find() should ALWAYS return a list, regardless of match count:
- 0 matches → []
- 1 match → [{...}]
- N matches → [{...}, {...}, ...]

Actual (before fix):
- 0 matches → []
- 1 match → {...} ❌
- N matches → [{...}, {...}, ...]
"""

import json

import fraiseql._fraiseql_rs as fraiseql_rs
import pytest

pytestmark = pytest.mark.rust


class TestIssue114SingleRecordList:
    """Test that db.find() always returns list, even for single records."""

    def test_rust_pipeline_single_record_with_is_list_true(self) -> None:
        """Rust pipeline: single record with is_list=True should return array."""
        result = fraiseql_rs.build_graphql_response(
            json_strings=['{"id": "1", "name": "router-01"}'],
            field_name="routers",
            type_name="Router",
            is_list=True,
        )

        data = json.loads(result.decode("utf-8"))

        assert isinstance(data["data"]["routers"], list), "Should return list, not dict"
        assert len(data["data"]["routers"]) == 1, "Should have exactly one item"
        assert data["data"]["routers"][0]["name"] == "router-01"

    def test_rust_pipeline_single_record_with_is_list_false(self) -> None:
        """Rust pipeline: single record with is_list=False should return object."""
        result = fraiseql_rs.build_graphql_response(
            json_strings=['{"id": "1", "name": "router-01"}'],
            field_name="router",
            type_name="Router",
            is_list=False,
        )

        data = json.loads(result.decode("utf-8"))

        assert isinstance(data["data"]["router"], dict), "Should return dict, not list"
        assert data["data"]["router"]["name"] == "router-01"

    def test_rust_pipeline_multiple_records_with_is_list_true(self) -> None:
        """Rust pipeline: multiple records with is_list=True should return array."""
        result = fraiseql_rs.build_graphql_response(
            json_strings=[
                '{"id": "1", "name": "router-01"}',
                '{"id": "2", "name": "router-02"}',
            ],
            field_name="routers",
            type_name="Router",
            is_list=True,
        )

        data = json.loads(result.decode("utf-8"))

        assert isinstance(data["data"]["routers"], list), "Should return list"
        assert len(data["data"]["routers"]) == 2, "Should have two items"

    def test_rust_pipeline_zero_records_with_is_list_true(self) -> None:
        """Rust pipeline: zero records with is_list=True should return empty array."""
        result = fraiseql_rs.build_graphql_response(
            json_strings=[],
            field_name="routers",
            type_name=None,
            is_list=True,
        )

        data = json.loads(result.decode("utf-8"))

        assert isinstance(data["data"]["routers"], list), "Should return list"
        assert len(data["data"]["routers"]) == 0, "Should be empty"
