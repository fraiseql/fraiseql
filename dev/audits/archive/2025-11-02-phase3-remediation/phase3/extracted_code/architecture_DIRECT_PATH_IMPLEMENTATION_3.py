# Extracted from: docs/architecture/DIRECT_PATH_IMPLEMENTATION.md
# Block number: 3
@fraiseql_type(sql_source="v_user", jsonb_column="data")
class User:
    id: str
    first_name: str
    email: str
