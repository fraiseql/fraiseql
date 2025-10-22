"""
FraiseQL SQL Utilities

SQL query building utilities, including:
- WHERE clause builder for JSONB queries
- Operator support (eq, ne, gt, lt, contains, in, etc.)
- SQL injection safe (parameterized queries)

Example:
    from fraiseql.sql.where_builder import build_where_clause

    where = {
        "status": "active",
        "age": {"gt": 18},
        "name": {"contains": "john"}
    }

    where_sql, params = build_where_clause(where, jsonb_column="data")
    # → data->>'status' = 'active'
    #   AND data->>'age' > '18'
    #   AND data->>'name' LIKE '%john%'
"""

# Exports will be added as implementation progresses:
# from fraiseql.sql.where_builder import build_where_clause
# from fraiseql.sql.operators import operators

__all__ = [
    # Add exports as they are implemented
]
