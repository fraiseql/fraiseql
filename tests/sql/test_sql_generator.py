from psycopg.sql import SQL

from fraiseql.core.ast_parser import FieldPath
from fraiseql.sql.sql_generator import build_sql_query
import pytest




@pytest.mark.unit
def test_basic_select_flat_fields() -> None:
    query = build_sql_query(
        table="my_table",
        field_paths=[
            FieldPath(alias="age", path=["profile", "age"]),
            FieldPath(alias="nickname", path=["profile", "username"]),
        ],
    )

    sql_str = query.as_string(None)

    assert "SELECT" in sql_str
    assert 'FROM "my_table"' in sql_str
    # Type-aware operator selection: -> for age (numeric), ->> for username (string)
    assert "data->'profile'->'age' AS \"age\"" in sql_str
    assert "data->'profile'->>'username' AS \"nickname\"" in sql_str
    assert "WHERE" not in sql_str


def test_select_with_where_clause() -> None:
    where = SQL("data->>'status' = 'active'")
    query = build_sql_query(
        table="users",
        field_paths=[FieldPath(alias="email", path=["contact", "email"])],
        where_clause=where,
    )

    sql_str = query.as_string(None)

    assert 'FROM "users"' in sql_str
    assert "WHERE data->>'status' = 'active'" in sql_str


def test_nested_path_multiple_levels() -> None:
    query = build_sql_query(
        table="events", field_paths=[FieldPath(alias="city", path=["location", "address", "city"])]
    )

    sql_str = query.as_string(None)
    # city is a string field, so uses ->> operator
    assert "data->'location'->'address'->>'city' AS \"city\"" in sql_str


def test_field_path_aliasing() -> None:
    query = build_sql_query(
        table="products",
        field_paths=[
            FieldPath(alias="productName", path=["info", "name"]),
            FieldPath(alias="productPrice", path=["pricing", "retail"]),
        ],
    )

    sql_str = query.as_string(None)
    assert 'AS "productName"' in sql_str
    assert 'AS "productPrice"' in sql_str


def test_empty_field_paths() -> None:
    query = build_sql_query(table="empty_case", field_paths=[])

    sql_str = query.as_string(None)
    assert sql_str.startswith('SELECT  FROM "empty_case"')
    assert "WHERE" not in sql_str


def test_json_output_with_typename() -> None:
    query = build_sql_query(
        table="accounts",
        field_paths=[
            FieldPath(alias="id", path=["meta", "id"]),
            FieldPath(alias="role", path=["meta", "role"]),
        ],
        where_clause=SQL("data->>'deleted' IS NULL"),
        json_output=True,
        typename="Account",
    )

    sql_str = query.as_string(None)

    assert sql_str.startswith("SELECT jsonb_build_object(")
    # ID and role are string fields, so use ->> operator
    assert "'id', data->'meta'->>'id'" in sql_str
    assert "'role', data->'meta'->>'role'" in sql_str
    assert "'__typename', 'Account'" in sql_str
    assert 'FROM "accounts"' in sql_str
    assert "WHERE data->>'deleted' IS NULL" in sql_str


def test_order_by_single_field() -> None:
    """Test ORDER BY with a single top-level field."""
    query = build_sql_query(
        table="users",
        field_paths=[
            FieldPath(alias="name", path=["name"]),
            FieldPath(alias="email", path=["email"]),
        ],
        order_by=[("created_at", "desc")],
    )

    sql_str = query.as_string(None)
    assert "ORDER BY data->>'created_at' DESC" in sql_str


def test_order_by_nested_field() -> None:
    """Test ORDER BY with nested fields."""
    query = build_sql_query(
        table="users",
        field_paths=[
            FieldPath(alias="name", path=["name"]),
            FieldPath(alias="age", path=["profile", "age"]),
        ],
        order_by=[("profile.age", "asc"), ("profile.location.city", "desc")],
    )

    sql_str = query.as_string(None)
    assert (
        "ORDER BY data->'profile'->>'age' ASC, data->'profile'->'location'->>'city' DESC" in sql_str
    )


def test_order_by_multiple_fields() -> None:
    """Test ORDER BY with multiple fields including nested ones."""
    query = build_sql_query(
        table="products",
        field_paths=[
            FieldPath(alias="name", path=["name"]),
            FieldPath(alias="price", path=["pricing", "retail"]),
        ],
        order_by=[("category", "asc"), ("pricing.retail", "desc"), ("metadata.popularity", "desc")],
    )

    sql_str = query.as_string(None)
    assert "ORDER BY" in sql_str
    assert "data->>'category' ASC" in sql_str
    assert "data->'pricing'->>'retail' DESC" in sql_str
    assert "data->'metadata'->>'popularity' DESC" in sql_str


def test_group_by_single_field() -> None:
    """Test GROUP BY with a single field."""
    query = build_sql_query(
        table="orders",
        field_paths=[FieldPath(alias="status", path=["status"])],
        group_by=["status"],
    )

    sql_str = query.as_string(None)
    assert "GROUP BY data->>'status'" in sql_str


def test_group_by_nested_fields() -> None:
    """Test GROUP BY with nested fields."""
    query = build_sql_query(
        table="users",
        field_paths=[
            FieldPath(alias="country", path=["address", "country"]),
            FieldPath(alias="city", path=["address", "city"]),
        ],
        group_by=["address.country", "address.city"],
    )

    sql_str = query.as_string(None)
    assert "GROUP BY data->'address'->>'country', data->'address'->>'city'" in sql_str


def test_group_by_deeply_nested() -> None:
    """Test GROUP BY with deeply nested fields."""
    query = build_sql_query(
        table="events",
        field_paths=[FieldPath(alias="venue", path=["location", "venue", "name"])],
        group_by=["location.venue.name", "location.venue.type"],
    )

    sql_str = query.as_string(None)
    assert "GROUP BY" in sql_str
    assert "data->'location'->'venue'->>'name'" in sql_str
    assert "data->'location'->'venue'->>'type'" in sql_str


def test_combined_where_group_by_order_by() -> None:
    """Test combining WHERE, GROUP BY, and ORDER BY clauses."""
    query = build_sql_query(
        table="sales",
        field_paths=[
            FieldPath(alias="region", path=["location", "region"]),
            FieldPath(alias="product", path=["product", "category"]),
        ],
        where_clause=SQL("data->>'year' = '2024'"),
        group_by=["location.region", "product.category"],
        order_by=[("location.region", "asc")],
    )

    sql_str = query.as_string(None)

    # Verify correct SQL clause order
    where_pos = sql_str.find("WHERE")
    group_pos = sql_str.find("GROUP BY")
    order_pos = sql_str.find("ORDER BY")

    assert where_pos > 0
    assert group_pos > where_pos
    assert order_pos > group_pos

    assert "WHERE data->>'year' = '2024'" in sql_str
    assert "GROUP BY data->'location'->>'region', data->'product'->>'category'" in sql_str
    assert "ORDER BY data->'location'->>'region' ASC" in sql_str


def test_json_output_with_order_by() -> None:
    """Test JSON output format with ORDER BY."""
    query = build_sql_query(
        table="users",
        field_paths=[
            FieldPath(alias="id", path=["id"]),
            FieldPath(alias="name", path=["profile", "fullName"]),
        ],
        json_output=True,
        typename="User",
        order_by=[("profile.fullName", "asc"), ("created_at", "desc")],
    )

    sql_str = query.as_string(None)

    assert "jsonb_build_object(" in sql_str
    assert "'__typename', 'User'" in sql_str
    assert "ORDER BY data->'profile'->>'full_name' ASC, data->>'created_at' DESC" in sql_str


def test_empty_order_by_and_group_by() -> None:
    """Test that empty order_by and group_by lists don't affect the query."""
    query = build_sql_query(
        table="users",
        field_paths=[FieldPath(alias="name", path=["name"])],
        order_by=[],
        group_by=[],
    )

    sql_str = query.as_string(None)
    assert "ORDER BY" not in sql_str
    assert "GROUP BY" not in sql_str


def test_complex_nested_scenario() -> None:
    """Test complex scenario with deeply nested fields in all clauses."""
    query = build_sql_query(
        table="analytics",
        field_paths=[
            FieldPath(alias="browser", path=["user", "device", "browser", "name"]),
            FieldPath(alias="country", path=["geo", "location", "country"]),
        ],
        where_clause=SQL("data->'user'->'device'->>'type' = 'mobile'"),
        group_by=["user.device.browser.name", "geo.location.country", "user.device.os.version"],
        order_by=[("geo.location.country", "asc"), ("user.device.browser.name", "desc")],
    )

    sql_str = query.as_string(None)

    # Check field extraction
    assert "data->'user'->'device'->'browser'->>'name'" in sql_str
    assert "data->'geo'->'location'->>'country'" in sql_str

    # Check WHERE clause
    assert "WHERE data->'user'->'device'->>'type' = 'mobile'" in sql_str

    # Check GROUP BY
    assert "GROUP BY" in sql_str
    assert "data->'user'->'device'->'os'->>'version'" in sql_str

    # Check ORDER BY
    assert "ORDER BY" in sql_str
    assert "data->'geo'->'location'->>'country' ASC" in sql_str
    assert "data->'user'->'device'->'browser'->>'name' DESC" in sql_str