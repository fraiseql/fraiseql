"""Tests for analytics decorators (@fact_table, @aggregate_query)."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


def setup_function() -> None:
    """Clear registry before each test."""
    SchemaRegistry.clear()


def test_fact_table_decorator() -> None:
    """Test @fraiseql.fact_table decorator registers metadata correctly."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue", "quantity"],
        dimension_paths=[
            {"name": "category", "json_path": "data->>'category'", "data_type": "text"}
        ],
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float
        quantity: int
        customer_id: str
        occurred_at: str

    schema = SchemaRegistry.get_schema()

    # Check fact table is registered
    assert "fact_tables" in schema
    assert len(schema["fact_tables"]) == 1

    fact_table = schema["fact_tables"][0]
    assert fact_table["table_name"] == "tf_sales"

    # Check measures
    assert len(fact_table["measures"]) == 2
    assert fact_table["measures"][0]["name"] == "revenue"
    assert fact_table["measures"][0]["sql_type"] == "Float"
    assert fact_table["measures"][1]["name"] == "quantity"
    assert fact_table["measures"][1]["sql_type"] == "Int"

    # Check dimensions
    assert fact_table["dimensions"]["name"] == "data"
    assert len(fact_table["dimensions"]["paths"]) == 1
    assert fact_table["dimensions"]["paths"][0]["name"] == "category"

    # Check denormalized filters
    assert len(fact_table["denormalized_filters"]) == 2
    filter_names = {f["name"] for f in fact_table["denormalized_filters"]}
    assert "customer_id" in filter_names
    assert "occurred_at" in filter_names


def test_fact_table_invalid_table_name() -> None:
    """Test @fraiseql.fact_table raises error for invalid table name."""
    with pytest.raises(ValueError, match="must start with 'tf_'"):

        @fraiseql.fact_table(
            table_name="sales",  # Missing tf_ prefix
            measures=["revenue"],
        )
        @fraiseql.type
        class Sale:
            id: int
            revenue: float


def test_fact_table_invalid_measure_name() -> None:
    """Test @fraiseql.fact_table raises error for unknown measure."""
    with pytest.raises(ValueError, match="not found in class fields"):

        @fraiseql.fact_table(
            table_name="tf_sales",
            measures=["unknown_field"],  # Field doesn't exist
        )
        @fraiseql.type
        class Sale:
            id: int
            revenue: float


def test_fact_table_invalid_measure_type() -> None:
    """Test @fraiseql.fact_table raises error for non-numeric measure."""
    with pytest.raises(ValueError, match="must be Int or Float"):

        @fraiseql.fact_table(
            table_name="tf_sales",
            measures=["customer_id"],  # String field, not numeric
        )
        @fraiseql.type
        class Sale:
            id: int
            revenue: float
            customer_id: str


def test_fact_table_default_dimension_column() -> None:
    """Test @fraiseql.fact_table uses default dimension column name."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue"],
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float
        customer_id: str

    schema = SchemaRegistry.get_schema()
    fact_table = schema["fact_tables"][0]
    assert fact_table["dimensions"]["name"] == "data"


def test_fact_table_custom_dimension_column() -> None:
    """Test @fraiseql.fact_table with custom dimension column name."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue"],
        dimension_column="metadata",
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float

    schema = SchemaRegistry.get_schema()
    fact_table = schema["fact_tables"][0]
    assert fact_table["dimensions"]["name"] == "metadata"


def test_aggregate_query_decorator() -> None:
    """Test @fraiseql.aggregate_query decorator registers correctly."""

    @fraiseql.aggregate_query(
        fact_table="tf_sales",
        auto_group_by=True,
        auto_aggregates=True,
    )
    @fraiseql.query
    def sales_aggregate() -> list[dict]:
        """Aggregate sales data."""

    schema = SchemaRegistry.get_schema()

    # Check aggregate query is registered
    assert "aggregate_queries" in schema
    assert len(schema["aggregate_queries"]) == 1

    agg_query = schema["aggregate_queries"][0]
    assert agg_query["name"] == "sales_aggregate"
    assert agg_query["fact_table"] == "tf_sales"
    assert agg_query["auto_group_by"] is True
    assert agg_query["auto_aggregates"] is True
    assert agg_query["description"] == "Aggregate sales data."


def test_aggregate_query_with_defaults() -> None:
    """Test @fraiseql.aggregate_query with default parameters."""

    @fraiseql.aggregate_query(fact_table="tf_sales")
    @fraiseql.query
    def sales_aggregate() -> list[dict]:
        """Aggregate sales data."""

    schema = SchemaRegistry.get_schema()
    agg_query = schema["aggregate_queries"][0]

    # Check defaults
    assert agg_query["auto_group_by"] is True
    assert agg_query["auto_aggregates"] is True


def test_fact_table_with_multiple_measures() -> None:
    """Test @fraiseql.fact_table with multiple measures."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue", "cost", "quantity", "discount"],
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float
        cost: float
        quantity: int
        discount: float
        customer_id: str

    schema = SchemaRegistry.get_schema()
    fact_table = schema["fact_tables"][0]

    assert len(fact_table["measures"]) == 4
    measure_names = {m["name"] for m in fact_table["measures"]}
    assert measure_names == {"revenue", "cost", "quantity", "discount"}


def test_fact_table_with_multiple_dimension_paths() -> None:
    """Test @fraiseql.fact_table with multiple dimension paths."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue"],
        dimension_paths=[
            {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
            {
                "name": "product_name",
                "json_path": "data->>'product_name'",
                "data_type": "text",
            },
            {"name": "region", "json_path": "data->>'region'", "data_type": "text"},
        ],
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float

    schema = SchemaRegistry.get_schema()
    fact_table = schema["fact_tables"][0]

    assert len(fact_table["dimensions"]["paths"]) == 3
    path_names = {p["name"] for p in fact_table["dimensions"]["paths"]}
    assert path_names == {"category", "product_name", "region"}


def test_fact_table_and_aggregate_query_together() -> None:
    """Test using both @fraiseql.fact_table and @fraiseql.aggregate_query."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue", "quantity"],
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float
        quantity: int
        customer_id: str

    @fraiseql.aggregate_query(fact_table="tf_sales")
    @fraiseql.query
    def sales_aggregate() -> list[dict]:
        """Aggregate sales data."""

    schema = SchemaRegistry.get_schema()

    # Both should be registered
    assert "fact_tables" in schema
    assert "aggregate_queries" in schema
    assert len(schema["fact_tables"]) == 1
    assert len(schema["aggregate_queries"]) == 1

    # Check they reference each other correctly
    assert schema["fact_tables"][0]["table_name"] == "tf_sales"
    assert schema["aggregate_queries"][0]["fact_table"] == "tf_sales"


def test_fact_table_sql_type_mapping() -> None:
    """Test @fraiseql.fact_table maps Python types to SQL types correctly."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["quantity"],  # int -> Int
    )
    @fraiseql.type
    class Sale:
        id: int
        quantity: int
        customer_id: str  # str -> Text
        occurred_at: str  # str -> Text (timestamp in reality)

    schema = SchemaRegistry.get_schema()
    fact_table = schema["fact_tables"][0]

    # Check measure type
    assert fact_table["measures"][0]["sql_type"] == "Int"

    # Check filter types
    filter_by_name = {f["name"]: f for f in fact_table["denormalized_filters"]}
    assert filter_by_name["customer_id"]["sql_type"] == "Text"
    assert filter_by_name["occurred_at"]["sql_type"] == "Text"


def test_registry_clear_includes_analytics() -> None:
    """Test SchemaRegistry.clear() clears fact tables and aggregate queries."""

    @fraiseql.fact_table(table_name="tf_sales", measures=["revenue"])
    @fraiseql.type
    class Sale:
        id: int
        revenue: float

    @fraiseql.aggregate_query(fact_table="tf_sales")
    @fraiseql.query
    def sales_aggregate() -> list[dict]:
        """Aggregate sales."""

    schema = SchemaRegistry.get_schema()
    assert "fact_tables" in schema
    assert "aggregate_queries" in schema

    # Clear registry
    SchemaRegistry.clear()

    schema = SchemaRegistry.get_schema()
    assert "fact_tables" not in schema
    assert "aggregate_queries" not in schema


def test_fact_table_nullable_measures() -> None:
    """Test @fraiseql.fact_table handles nullable measures."""

    @fraiseql.fact_table(
        table_name="tf_sales",
        measures=["revenue", "discount"],
    )
    @fraiseql.type
    class Sale:
        id: int
        revenue: float
        discount: float | None  # Nullable measure

    schema = SchemaRegistry.get_schema()
    fact_table = schema["fact_tables"][0]

    measure_by_name = {m["name"]: m for m in fact_table["measures"]}
    assert measure_by_name["revenue"]["nullable"] is False
    assert measure_by_name["discount"]["nullable"] is True
