"""Integration tests for GraphQL where inputs with FraiseQLRepository."""

import uuid
from datetime import UTC, datetime
from decimal import Decimal

import pytest

from fraiseql import fraise_type
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql import (
    BooleanFilter,
    DecimalFilter,
    IntFilter,
    StringFilter,
    create_graphql_where_input,
)

# Import database fixtures for this database test
from tests.database_conftest import *  # noqa: F403


@fraise_type
class Product:
    """Product model for testing."""

    id: uuid.UUID
    name: str
    price: Decimal
    stock: int
    category: str
    is_active: bool
    created_at: datetime


# Create GraphQL where input
ProductWhereInput = create_graphql_where_input(Product)


@pytest.mark.asyncio
@pytest.mark.database
class TestGraphQLWhereRepositoryIntegration:
    """Test GraphQL where inputs with repository."""

    @pytest.fixture
    async def setup_test_data(self, db_pool):
        """Set up test data in database."""
        async with db_pool.connection() as conn, conn.cursor() as cursor:
            # Create test table
            await cursor.execute(
                """
                CREATE TABLE IF NOT EXISTS test_products (
                    id UUID PRIMARY KEY,
                    tenant_id UUID DEFAULT '00000000-0000-0000-0000-000000000000'::uuid,
                    name TEXT NOT NULL,
                    price DECIMAL(10,2) NOT NULL,
                    stock INTEGER NOT NULL,
                    category TEXT,
                    is_active BOOLEAN DEFAULT true,
                    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
                    data JSONB NOT NULL
                )
            """
            )

            # Create view
            await cursor.execute(
                """
                CREATE OR REPLACE VIEW test_product_view AS
                SELECT
                    id,
                    tenant_id,
                    name,
                    price,
                    stock,
                    category,
                    is_active,
                    created_at,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'price', price,
                        'stock', stock,
                        'category', category,
                        'is_active', is_active,
                        'created_at', created_at
                    ) as data
                FROM test_products
            """
            )

            # Insert test data
            products = [
                (
                    uuid.uuid4(),
                    "Widget A",
                    Decimal("19.99"),
                    100,
                    "widgets",
                    True,
                    datetime(2024, 1, 1, 10, 0, 0, tzinfo=UTC),
                ),
                (
                    uuid.uuid4(),
                    "Widget B",
                    Decimal("29.99"),
                    50,
                    "widgets",
                    True,
                    datetime(2024, 1, 2, 10, 0, 0, tzinfo=UTC),
                ),
                (
                    uuid.uuid4(),
                    "Gadget X",
                    Decimal("99.99"),
                    25,
                    "gadgets",
                    True,
                    datetime(2024, 1, 3, 10, 0, 0, tzinfo=UTC),
                ),
                (
                    uuid.uuid4(),
                    "Old Widget",
                    Decimal("9.99"),
                    0,
                    "widgets",
                    False,
                    datetime(2024, 1, 4, 10, 0, 0, tzinfo=UTC),
                ),
            ]

            for prod in products:
                await cursor.execute(
                    """
                    INSERT INTO test_products (
                        id, name, price, stock, category, is_active, created_at, data
                    )
                    VALUES (%s, %s, %s, %s, %s, %s, %s, jsonb_build_object(
                        'id', %s::uuid,
                        'name', %s::text,
                        'price', %s::decimal,
                        'stock', %s::integer,
                        'category', %s::text,
                        'is_active', %s::boolean,
                        'created_at', %s::timestamptz
                    ))
                    """,
                    (*prod, *prod),
                )

        # Register type for view
        register_type_for_view("test_product_view", Product)

        yield

        # Cleanup
        async with db_pool.connection() as conn, conn.cursor() as cursor:
            await cursor.execute("DROP VIEW IF EXISTS test_product_view")
            await cursor.execute("DROP TABLE IF EXISTS test_products")

    async def test_graphql_where_basic_filtering(self, db_pool, setup_test_data):
        """Test basic filtering with GraphQL where input."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Create GraphQL where input
        where_input = ProductWhereInput(
            category=StringFilter(eq="widgets"), is_active=BooleanFilter(eq=True)
        )

        # Use directly in repository - should auto-convert
        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 2  # Only active widgets
        assert all(r.category == "widgets" for r in results)
        assert all(r.is_active for r in results)

    async def test_graphql_where_comparison_operators(self, db_pool, setup_test_data):
        """Test comparison operators with GraphQL where input."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test price range
        where_input = ProductWhereInput(price=DecimalFilter(gte=Decimal(20), lt=Decimal(100)))

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 2  # Widget B and Gadget X
        assert all(Decimal(20) <= r.price < Decimal(100) for r in results)

    async def test_graphql_where_string_operations(self, db_pool, setup_test_data):
        """Test string operations with GraphQL where input."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test string contains
        where_input = ProductWhereInput(name=StringFilter(contains="Widget"))

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 3  # All widgets
        assert all("Widget" in r.name for r in results)

        # Test string startswith
        where_input = ProductWhereInput(name=StringFilter(startswith="Widget"))

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 2  # Widget A and Widget B
        assert all(r.name.startswith("Widget") for r in results)

    async def test_graphql_where_multiple_conditions(self, db_pool, setup_test_data):
        """Test multiple conditions on same field."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Multiple price conditions
        where_input = ProductWhereInput(
            price=DecimalFilter(gt=Decimal(10), lte=Decimal(30)), stock=IntFilter(gt=0)
        )

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 2  # Widget A and Widget B
        assert all(Decimal(10) < r.price <= Decimal(30) for r in results)
        assert all(r.stock > 0 for r in results)

    async def test_graphql_where_in_operator(self, db_pool, setup_test_data):
        """Test 'in' operator with GraphQL where input."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test with categories
        where_input = ProductWhereInput(category=StringFilter(in_=["gadgets", "accessories"]))

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 1  # Only Gadget X
        assert results[0].category == "gadgets"

    async def test_graphql_where_null_checks(self, db_pool, setup_test_data):
        """Test null checking with GraphQL where input."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # All our test data has non-null categories, but test the operator
        where_input = ProductWhereInput(category=StringFilter(isnull=False))

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 4  # All products have categories
        assert all(r.category is not None for r in results)

    async def test_graphql_where_with_additional_filters(self, db_pool, setup_test_data):
        """Test GraphQL where combined with kwargs filters."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # GraphQL where for price
        where_input = ProductWhereInput(price=DecimalFilter(lt=Decimal(50)))

        # Additional kwargs filter
        results = await repo.find(
            "test_product_view",
            where=where_input,
            is_active=True,  # Additional filter
        )

        assert len(results) == 2  # Active products under $50
        assert all(r.price < Decimal(50) for r in results)
        assert all(r.is_active for r in results)

    async def test_graphql_where_empty_filter(self, db_pool, setup_test_data):
        """Test empty GraphQL where input returns all records."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Empty where input
        where_input = ProductWhereInput()

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 4  # All products

    async def test_graphql_where_production_mode(self, db_pool, setup_test_data):
        """Test GraphQL where input works in production mode."""
        repo = FraiseQLRepository(db_pool, context={"mode": "production"})

        where_input = ProductWhereInput(stock=IntFilter(eq=0))

        results = await repo.find("test_product_view", where=where_input)

        assert len(results) == 1
        # In production mode, we get dicts
        assert isinstance(results[0], dict)
        assert results[0]["stock"] == 0

    async def test_graphql_where_find_one(self, db_pool, setup_test_data):
        """Test GraphQL where input with find_one."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        where_input = ProductWhereInput(name=StringFilter(eq="Gadget X"))

        result = await repo.find_one("test_product_view", where=where_input)

        assert result is not None
        assert result.name == "Gadget X"
        assert result.price == Decimal("99.99")

    async def test_graphql_where_complex_scenario(self, db_pool, setup_test_data):
        """Test complex real-world filtering scenario."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Find active widgets in stock under $30
        where_input = ProductWhereInput(
            category=StringFilter(eq="widgets"),
            is_active=BooleanFilter(eq=True),
            stock=IntFilter(gt=0),
            price=DecimalFilter(lt=Decimal(30)),
        )

        results = await repo.find("test_product_view", where=where_input, order_by="price ASC")

        assert len(results) == 2  # Widget A and Widget B meet all criteria
        assert results[0].name == "Widget A"  # Ordered by price ASC
        assert results[0].price == Decimal("19.99")
        assert results[1].name == "Widget B"
        assert results[1].price == Decimal("29.99")
