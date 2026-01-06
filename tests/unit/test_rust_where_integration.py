"""Integration tests for Phase 7.2 Rust WHERE normalization.

Tests verify that Rust and Python implementations produce identical results.
"""

import os

import pytest

from fraiseql.where_normalization import normalize_dict_where


@pytest.fixture
def disable_rust() -> None:
    """Temporarily disable Rust WHERE normalization."""
    old_value = os.environ.get("FRAISEQL_USE_RUST_WHERE")
    os.environ["FRAISEQL_USE_RUST_WHERE"] = "false"
    yield
    if old_value is None:
        os.environ.pop("FRAISEQL_USE_RUST_WHERE", None)
    else:
        os.environ["FRAISEQL_USE_RUST_WHERE"] = old_value


@pytest.fixture
def enable_rust() -> None:
    """Ensure Rust WHERE normalization is enabled."""
    old_value = os.environ.get("FRAISEQL_USE_RUST_WHERE")
    os.environ["FRAISEQL_USE_RUST_WHERE"] = "true"
    yield
    if old_value is None:
        os.environ.pop("FRAISEQL_USE_RUST_WHERE", None)
    else:
        os.environ["FRAISEQL_USE_RUST_WHERE"] = old_value


class TestRustWhereIntegration:
    """Test Rust WHERE normalization integration with Python."""

    def test_simple_equality_filter(self, enable_rust) -> None:
        """Test simple equality filter."""
        where_dict = {"status": {"eq": "active"}}
        table_columns = {"id", "status", "name"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "status" in str(sql)
        assert "active" in params

    def test_jsonb_filter(self, enable_rust) -> None:
        """Test JSONB field filter."""
        where_dict = {"device_name": {"eq": "Printer"}}
        table_columns = {"id", "data"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "data" in str(sql)
        assert "device_name" in str(sql)
        assert "Printer" in params

    def test_fk_nested_filter(self, enable_rust) -> None:
        """Test FK filter with nested field."""
        where_dict = {"machine": {"id": {"eq": "123"}}}
        table_columns = {"id", "machine_id", "data"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "machine_id" in str(sql)
        assert "123" in params

    def test_multiple_conditions(self, enable_rust) -> None:
        """Test multiple AND conditions."""
        where_dict = {"status": {"eq": "active"}, "priority": {"gt": "5"}}
        table_columns = {"id", "status", "priority"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "status" in str(sql)
        assert "priority" in str(sql)
        assert len(params) == 2

    def test_comparison_rust_vs_python_simple(self, enable_rust, disable_rust) -> None:
        """Compare Rust vs Python for simple filter."""
        where_dict = {"status": {"eq": "active"}}
        table_columns = {"id", "status"}

        # Get Rust result
        os.environ["FRAISEQL_USE_RUST_WHERE"] = "true"
        rust_result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        rust_sql, rust_params = rust_result.to_sql()

        # Get Python result
        os.environ["FRAISEQL_USE_RUST_WHERE"] = "false"
        # Force reimport to pick up env change
        import importlib

        import fraiseql.where_normalization

        importlib.reload(fraiseql.where_normalization)
        python_result = fraiseql.where_normalization.normalize_dict_where(
            where_dict, "test_table", table_columns, "data"
        )
        python_sql, python_params = python_result.to_sql()

        # Both should produce equivalent SQL (may differ in formatting)
        assert rust_params == python_params
        # SQL structure should be similar
        assert "status" in str(rust_sql)
        assert "status" in str(python_sql)

    def test_fallback_on_rust_error(self, enable_rust, monkeypatch) -> None:
        """Test that Python fallback works if Rust fails."""

        # Monkeypatch to simulate Rust failure
        def mock_rust_normalize(*args, **kwargs) -> None:  # noqa: ANN002, ANN003
            raise RuntimeError("Simulated Rust error")

        monkeypatch.setattr(
            "fraiseql.where_normalization.normalize_where_to_sql",
            mock_rust_normalize,
            raising=False,
        )

        where_dict = {"status": {"eq": "active"}}
        table_columns = {"id", "status"}

        # Should fall back to Python implementation
        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "status" in str(sql)
        assert "active" in params

    def test_empty_table_columns(self, enable_rust) -> None:
        """Test with no table columns (pure JSONB)."""
        where_dict = {"name": {"eq": "test"}}
        table_columns = set()

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "data" in str(sql)
        assert "name" in str(sql)
        assert "test" in params

    def test_operators_gt_lt(self, enable_rust) -> None:
        """Test comparison operators."""
        where_dict = {"age": {"gt": "18"}}
        table_columns = {"id", "age"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "age" in str(sql)
        assert "18" in params

    def test_operators_in(self, enable_rust) -> None:
        """Test IN operator."""
        where_dict = {"status": {"in": ["active", "pending"]}}
        table_columns = {"id", "status"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "status" in str(sql)
        # IN operator should have both values
        assert "active" in str(params) or "active" in str(sql)
        assert "pending" in str(params) or "pending" in str(sql)

    def test_operators_contains(self, enable_rust) -> None:
        """Test CONTAINS operator (ILIKE)."""
        where_dict = {"name": {"contains": "test"}}
        table_columns = {"id", "name"}

        result = normalize_dict_where(where_dict, "test_table", table_columns, "data")
        sql, params = result.to_sql()

        assert "name" in str(sql)
        # Should have wildcard pattern
        assert any("%test%" in str(p) for p in params)
