"""Integration tests for GraphQL where input feature."""

from datetime import datetime
from decimal import Decimal
from uuid import UUID

import fraiseql
from fraiseql import query
from fraiseql.db import FraiseQLRepository
from fraiseql.sql import (
    BooleanFilter,
    DateTimeFilter,
    DecimalFilter,
    IntFilter,
    StringFilter,
    UUIDFilter,
    create_graphql_where_input,
)


@fraiseql.type
class Product:
    """Test product type."""

    id: UUID
    name: str
    description: str | None
    price: Decimal
    stock: int
    is_active: bool
    created_at: datetime


# Create GraphQL where input
ProductWhereInput = create_graphql_where_input(Product)


class TestGraphQLWhereInputIntegration:
    """Test the integration of GraphQL where inputs with the repository."""

    def test_where_input_in_query_resolver(self):
        """Test that where inputs can be used in query resolvers."""

        @query
        async def products(info, where: ProductWhereInput | None = None) -> list[Product]:
            """Query products with optional filtering."""
            db = info.context["db"]
            return await db.find("product_view", where=where)

        # Verify the query function accepts ProductWhereInput
        import inspect

        sig = inspect.signature(products)
        where_param = sig.parameters["where"]

        # Check the annotation includes ProductWhereInput
        assert ProductWhereInput in where_param.annotation.__args__

    def test_automatic_conversion_in_repository(self):
        """Test that GraphQL where inputs are automatically converted in the repository."""

        # Create a mock repository
        class MockPool:
            async def connection(self):
                return self

            async def __aenter__(self):
                return self

            async def __aexit__(self, *args):
                pass

            def cursor(self, **kwargs):
                return MockCursor()

        class MockCursor:
            async def __aenter__(self):
                return self

            async def __aexit__(self, *args):
                pass

            async def execute(self, query, params=None):
                # Store the executed query for verification
                self.last_query = query
                self.last_params = params

            async def fetchall(self):
                return []

        # Create repository (not actually used in this test, just testing conversion)
        pool = MockPool()
        _repo = FraiseQLRepository(pool)

        # Create GraphQL where input
        where_input = ProductWhereInput(
            name=StringFilter(contains="Widget"),
            price=DecimalFilter(gte=Decimal("10.00")),
            is_active=BooleanFilter(eq=True),
        )

        # Test that _to_sql_where is called
        sql_where = where_input._to_sql_where()

        # Verify conversion
        assert hasattr(sql_where, "to_sql")
        assert sql_where.name == {"contains": "Widget"}
        assert sql_where.price == {"gte": Decimal("10.00")}
        assert sql_where.is_active == {"eq": True}

    def test_graphql_schema_generation(self):
        """Test that where input types are properly registered for GraphQL schema."""
        # The type should have the necessary GraphQL metadata
        assert hasattr(ProductWhereInput, "__gql_typename__")
        assert hasattr(ProductWhereInput, "__gql_fields__")
        assert hasattr(ProductWhereInput, "__fraiseql_definition__")

        # Check field definitions
        fields = ProductWhereInput.__gql_fields__
        assert "name" in fields
        assert "price" in fields
        assert "is_active" in fields

        # Check that fields are optional filter types
        assert fields["name"].field_type == StringFilter | None
        assert fields["price"].field_type == DecimalFilter | None
        assert fields["is_active"].field_type == BooleanFilter | None

    def test_nested_filter_operations(self):
        """Test complex nested filter operations."""
        where_input = ProductWhereInput(
            name=StringFilter(contains="Widget", startswith="Super", isnull=False),
            price=DecimalFilter(gte=Decimal("10.00"), lte=Decimal("100.00"), neq=Decimal("50.00")),
            stock=IntFilter(gt=0, in_=[10, 20, 30]),
        )

        # Convert to SQL where
        sql_where = where_input._to_sql_where()

        # Verify all operators are preserved
        assert sql_where.name == {"contains": "Widget", "startswith": "Super", "isnull": False}
        assert sql_where.price == {
            "gte": Decimal("10.00"),
            "lte": Decimal("100.00"),
            "neq": Decimal("50.00"),
        }
        assert sql_where.stock == {
            "gt": 0,
            "in": [10, 20, 30],  # Note: in_ is mapped to in
        }

    def test_graphql_field_name_mapping(self):
        """Test that in_ is properly mapped to 'in' in GraphQL."""
        # The in_ field should have graphql_name="in",
        in_field = StringFilter.__gql_fields__["in_"]
        assert in_field.graphql_name == "in"

        # Same for all filter types
        for FilterType in [IntFilter, DecimalFilter, UUIDFilter, DateTimeFilter]:
            assert FilterType.__gql_fields__["in_"].graphql_name == "in"

    def test_empty_and_null_handling(self):
        """Test handling of empty filters and null values."""
        # Empty filter
        where_input1 = ProductWhereInput(
            name=StringFilter(),  # Empty filter
        )
        sql_where1 = where_input1._to_sql_where()
        assert sql_where1.name is None  # Empty filters become None

        # Null field in input
        where_input2 = ProductWhereInput(
            name=None,  # No filter at all
        )
        sql_where2 = where_input2._to_sql_where()
        # Default dict from SQL where type
        assert sql_where2.name == {}

        # Testing isnull operator
        where_input3 = ProductWhereInput(description=StringFilter(isnull=True))
        sql_where3 = where_input3._to_sql_where()
        assert sql_where3.description == {"isnull": True}
