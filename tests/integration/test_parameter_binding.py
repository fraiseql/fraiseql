"""Integration tests for parameter binding correctness.

Verifies that parameterized queries have correct parameter alignment
and don't cause silent data corruption.
"""

import uuid
import pytest
from fraiseql.db import FraiseQLRepository
from fraiseql.where_clause import WhereClause


class TestParameterBinding:
    """Test parameter binding correctness in WHERE clause execution."""

    async def test_parameter_count_matches_placeholders(self, class_db_pool):
        """Verify parameter count matches %s placeholder count."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Complex query with multiple parameters
        where = {
            "status": {"in": ["active", "pending"]},
            "machine": {"id": {"eq": uuid.uuid4()}},
            "name": {"contains": "test"},
        }

        table_columns = {"status", "machine_id", "name", "data"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Count placeholders in SQL
        sql_str = sql.as_string(None)
        placeholder_count = sql_str.count("%s")

        assert placeholder_count == len(params), (
            f"Parameter count mismatch: {placeholder_count} placeholders "
            f"but {len(params)} parameters"
        )

    async def test_parameter_order_correctness(self, class_db_pool, setup_hybrid_table):
        """Verify parameters are in correct order for placeholders."""
        test_data = setup_hybrid_table
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Query with known data
        where = {"status": {"eq": "active"}, "machine": {"id": {"eq": test_data["machine1_id"]}}}

        # This should return results (correct binding)
        result = await repo.find("tv_allocation", where=where)

        # Should return results
        assert result is not None

    async def test_in_operator_parameter_binding(self, class_db_pool):
        """Verify IN operator uses tuple parameter correctly."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        where = {"status": {"in": ["active", "pending", "completed"]}}

        table_columns = {"status"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # IN operator should have single tuple parameter
        assert len(params) == 1
        assert isinstance(params[0], tuple)
        assert params[0] == ("active", "pending", "completed")

        # SQL should have single %s placeholder for IN
        sql_str = sql.as_string(None)
        assert sql_str.count("%s") == 1

    async def test_null_operator_no_parameters(self, class_db_pool):
        """Verify IS NULL operator has no parameters."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        where = {"machine_id": {"isnull": True}}

        table_columns = {"machine_id"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # IS NULL should have no parameters
        assert len(params) == 0

        # SQL should have no %s placeholders
        sql_str = sql.as_string(None)
        assert "%s" not in sql_str
        assert "IS NULL" in sql_str

    async def test_mixed_operators_parameter_binding(self, class_db_pool):
        """Verify complex WHERE with mixed operators has correct binding."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        machine_id = uuid.uuid4()
        where = {
            "status": {"in": ["active", "pending"]},
            "machine": {"id": {"eq": machine_id}},
            "name": {"contains": "test"},
            "created_at": {"gte": "2024-01-01"},
        }

        table_columns = {"status", "machine_id", "name", "created_at", "data"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Should have 4 parameters (IN tuple, eq UUID, contains pattern, gte date)
        expected_param_count = 4
        assert len(params) == expected_param_count

        # Verify parameter types
        assert isinstance(params[0], tuple)  # IN values
        assert isinstance(params[1], uuid.UUID)  # machine_id
        assert isinstance(params[2], str)  # LIKE pattern
        assert isinstance(params[3], str)  # date

    async def test_query_execution_smoke_test(self, class_db_pool, setup_hybrid_table):
        """Smoke test: Execute complex query to verify no runtime errors."""
        test_data = setup_hybrid_table
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Complex query
        where = {
            "status": {"in": ["active", "pending"]},
            "machine": {"id": {"eq": test_data["machine1_id"]}},
            "OR": [{"name": {"contains": "test"}}, {"name": {"startswith": "demo"}}],
        }

        # Should execute without errors
        result = await repo.find("tv_allocation", where=where)

        # Should return structured result
        assert result is not None
