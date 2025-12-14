"""Golden file tests for WHERE clause SQL generation.

These tests verify that the refactor doesn't change SQL output for common queries.
Each test records the expected SQL and parameters, then verifies they remain identical.
"""

import uuid

import pytest

from fraiseql.db import FraiseQLRepository

# Golden queries: real production patterns with expected SQL output
GOLDEN_QUERIES = [
    {
        "name": "simple_equality",
        "where": {"status": "active"},
        "expected_sql_contains": ['"status"', "=", "%s"],
        "expected_params": ["active"],
    },
    {
        "name": "simple_eq_operator",
        "where": {"status": {"eq": "active"}},
        "expected_sql_contains": ['"status"', "=", "%s"],
        "expected_params": ["active"],
    },
    {
        "name": "in_operator",
        "where": {"status": {"in": ["active", "pending"]}},
        "expected_sql_contains": ['"status"', "IN", "%s"],
        "expected_params": ["active", "pending"],  # psycopg3 uses individual parameters
    },
    {
        "name": "fk_nested_filter",
        "where": {"machine": {"id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")}}},
        "expected_sql_contains": ['"machine_id"', "=", "%s"],
        "expected_params": [uuid.UUID("12345678-1234-5678-1234-567812345678")],
        "table_columns": {"machine_id", "data"},
    },
    {
        "name": "jsonb_nested_filter",
        "where": {"device": {"name": {"eq": "Printer"}}},
        "expected_sql_contains": ["data", "->", "'device'", "->>", "'name'", "=", "%s"],
        "expected_params": ["Printer"],
        "table_columns": {"id", "data"},  # No device_id column
    },
    {
        "name": "multiple_conditions_and",
        "where": {
            "status": {"eq": "active"},
            "machine": {"id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")}},
        },
        "expected_sql_contains": ['"status"', "=", "AND", '"machine_id"', "="],
        "expected_param_count": 2,
        "table_columns": {"status", "machine_id", "data"},
    },
    {
        "name": "or_operator",
        "where": {"OR": [{"status": {"eq": "active"}}, {"status": {"eq": "pending"}}]},
        "expected_sql_contains": ['"status"', "=", "OR", '"status"', "="],
        "expected_param_count": 2,
    },
    {
        "name": "contains_string_operator",
        "where": {"name": {"contains": "test"}},
        "expected_sql_contains": ["data", "->>", "'name'", "LIKE", "%s"],
        "expected_params": ["%test%"],
        "table_columns": {"id", "data"},  # No name column, so JSONB
    },
    {
        "name": "isnull_operator",
        "where": {"machine_id": {"isnull": True}},
        "expected_sql_contains": ['"machine_id"', "IS NULL"],
        "expected_param_count": 0,
    },
    {
        "name": "not_isnull_operator",
        "where": {"machine_id": {"isnull": False}},
        "expected_sql_contains": ['"machine_id"', "IS NOT NULL"],
        "expected_param_count": 0,
    },
    {
        "name": "gte_operator",
        "where": {"created_at": {"gte": "2024-01-01"}},
        "expected_sql_contains": ["data", "->>", "'created_at'", ">=", "%s"],
        "expected_params": ["2024-01-01"],
        "table_columns": {"id", "data"},  # No created_at column, so JSONB
    },
    {
        "name": "mixed_fk_and_jsonb",
        "where": {
            "machine": {
                "id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")},
                "name": {"contains": "Printer"},
            }
        },
        "expected_sql_contains": [
            '"machine_id"',
            "=",
            "data",
            "->",
            "'machine'",
            "->>",
            "'name'",
            "LIKE",
        ],
        "expected_param_count": 2,
        "table_columns": {"machine_id", "data"},
    },
]


class TestGoldenFileRegression:
    """Test SQL output unchanged for common WHERE patterns."""

    @pytest.mark.parametrize("golden", GOLDEN_QUERIES, ids=lambda g: g["name"])
    def test_where_sql_unchanged(self, golden):
        """Verify WHERE clause generates expected SQL."""
        repo = FraiseQLRepository(None)

        table_columns = golden.get("table_columns", {"status", "machine_id", "data"})
        where = golden["where"]

        # Normalize WHERE clause
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Verify SQL contains expected fragments
        sql_str = sql.as_string(None)
        for expected_fragment in golden.get("expected_sql_contains", []):
            assert expected_fragment in sql_str, (
                f"Golden test '{golden['name']}' failed: "
                f"Expected fragment '{expected_fragment}' not found in SQL: {sql_str}"
            )

        # Verify parameter count or exact params
        if "expected_params" in golden:
            assert params == golden["expected_params"], (
                f"Golden test '{golden['name']}' failed: "
                f"Expected params {golden['expected_params']}, got {params}"
            )
        elif "expected_param_count" in golden:
            assert len(params) == golden["expected_param_count"], (
                f"Golden test '{golden['name']}' failed: "
                f"Expected {golden['expected_param_count']} params, got {len(params)}"
            )

    def test_golden_queries_comprehensive_coverage(self):
        """Verify golden tests cover all major WHERE patterns."""
        golden_names = {g["name"] for g in GOLDEN_QUERIES}

        required_patterns = {
            "simple_equality",
            "simple_eq_operator",
            "in_operator",
            "fk_nested_filter",
            "jsonb_nested_filter",
            "or_operator",
            "contains_string_operator",
            "isnull_operator",
        }

        assert required_patterns.issubset(golden_names), (
            f"Missing golden tests for: {required_patterns - golden_names}"
        )
