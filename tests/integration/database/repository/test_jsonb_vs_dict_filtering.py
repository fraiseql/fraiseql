"""Test to verify that both JSONB WhereInput and dictionary filters work correctly.

This test ensures that:
1. WhereInput types use JSONB paths for views with JSONB data columns
2. Dictionary filters use direct column names for regular tables
"""

import pytest
from decimal import Decimal
from uuid import uuid4

pytestmark = pytest.mark.database

from tests.fixtures.database.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql.where_generator import safe_create_where_type


@fraiseql.type
class TestProduct:
    """Product type for testing."""
    id: str
    name: str
    price: Decimal
    category: str
    is_active: bool


# Generate WhereInput type for JSONB filtering
TestProductWhere = safe_create_where_type(TestProduct)


class TestJSONBvsDictFiltering:
    """Test that both JSONB and direct column filtering work correctly."""

    @pytest.fixture
    async def setup_test_data(self, db_pool):
        """Create both regular table and JSONB view for testing."""
        async with db_pool.connection() as conn:
            # Create regular table (no JSONB column)
            await conn.execute("""
                CREATE TABLE IF NOT EXISTS test_products_regular (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    name TEXT NOT NULL,
                    price NUMERIC(10, 2) NOT NULL,
                    category TEXT NOT NULL,
                    is_active BOOLEAN NOT NULL DEFAULT true
                )
            """)

            # Create view with JSONB data column
            await conn.execute("""
                CREATE OR REPLACE VIEW test_products_jsonb AS
                SELECT
                    id, name, price, category, is_active,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'price', price,
                        'category', category,
                        'is_active', is_active
                    ) as data
                FROM test_products_regular
            """)

            # Clear and insert test data
            await conn.execute("DELETE FROM test_products_regular")

            products = [
                (str(uuid4()), "Widget A", Decimal("99.99"), "electronics", True),
                (str(uuid4()), "Widget B", Decimal("149.99"), "electronics", True),
                (str(uuid4()), "Gadget A", Decimal("49.99"), "accessories", False),
                (str(uuid4()), "Tool A", Decimal("199.99"), "tools", True),
            ]

            async with conn.cursor() as cursor:
                for id_val, name, price, category, is_active in products:
                    await cursor.execute(
                        """
                        INSERT INTO test_products_regular
                        (id, name, price, category, is_active)
                        VALUES (%s, %s, %s, %s, %s)
                        """,
                        (id_val, name, price, category, is_active)
                    )
            await conn.commit()

            return len(products)

    @pytest.mark.asyncio
    async def test_whereinput_uses_jsonb_paths(self, db_pool, setup_test_data):
        """Test that WhereInput types correctly use JSONB paths for views with data column."""
        # setup_test_data is already executed as a fixture

        # Register type for development mode
        register_type_for_view("test_products_jsonb", TestProduct)

        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Use WhereInput type - this should use JSONB paths (data->>'field')
        where = TestProductWhere(
            category={"eq": "electronics"},
            is_active={"eq": True}
        )

        # This should work because the view has a 'data' JSONB column
        results = await repo.find("test_products_jsonb", where=where)

        assert len(results) == 2  # Widget A and Widget B
        for product in results:
            assert product.category == "electronics"
            assert product.is_active is True

    @pytest.mark.asyncio
    async def test_dict_filters_use_direct_columns(self, db_pool, setup_test_data):
        """Test that dictionary filters use direct column names for regular tables."""
        # setup_test_data is already executed as a fixture

        repo = FraiseQLRepository(db_pool, context={"mode": "production"})

        # Use dictionary filter - this should use direct column names
        where = {
            "category": {"eq": "electronics"},
            "is_active": {"eq": True}
        }

        # This should work on the regular table (no JSONB column)
        results = await repo.find("test_products_regular", where=where)

        assert len(results) == 2  # Widget A and Widget B
        for product in results:
            assert product["category"] == "electronics"
            assert product["is_active"] is True

    @pytest.mark.asyncio
    async def test_dynamic_dict_filter_construction(self, db_pool, setup_test_data):
        """Test dynamic filter construction pattern (the original bug case)."""
        # setup_test_data is already executed as a fixture

        repo = FraiseQLRepository(db_pool, context={"mode": "production"})

        # Simulate dynamic filter construction in a resolver
        where = {}

        # Dynamically add filters based on conditions
        filter_active = True
        if filter_active:
            where["is_active"] = {"eq": True}

        min_price = 100
        if min_price:
            where["price"] = {"gte": min_price}

        # This should work on regular table
        results = await repo.find("test_products_regular", where=where)

        # Should return products that are active AND price >= 100
        assert len(results) == 2  # Widget B and Tool A
        for product in results:
            assert product["is_active"] is True
            assert float(product["price"]) >= 100

    @pytest.mark.asyncio
    async def test_whereinput_on_regular_table_works(self, db_pool, setup_test_data):
        """Test that WhereInput now works on regular tables after hybrid table fix.

        Previously, WhereInput would fail on regular tables because it generated
        JSONB paths. After the v0.9.5 fix for hybrid tables, WhereInput is converted
        to dict format which works correctly on regular tables too.
        """
        # setup_test_data is already executed as a fixture

        register_type_for_view("test_products_regular", TestProduct)
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # WhereInput type now works on regular tables
        where = TestProductWhere(category={"eq": "electronics"})

        # This should now work correctly
        results = await repo.find("test_products_regular", where=where)

        # Should return electronics products
        assert len(results) == 2  # Widget B and Gadget C
        for product in results:
            assert product.category == "electronics"

    @pytest.mark.asyncio
    async def test_mixed_whereinput_and_kwargs(self, db_pool, setup_test_data):
        """Test combining WhereInput with additional kwargs filters."""
        # setup_test_data is already executed as a fixture

        register_type_for_view("test_products_jsonb", TestProduct)
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Use WhereInput for complex filtering
        where = TestProductWhere(price={"gte": 50, "lte": 150})

        # Add simple kwargs filter
        results = await repo.find(
            "test_products_jsonb",
            where=where,
            is_active=True  # Additional simple filter
        )

        # Should return products with price between 50-150 AND active
        # That's Widget A (99.99) but NOT Widget B (149.99) because it's at the upper bound
        # Actually Widget B is 149.99 which is <= 150, so both should match
        assert len(results) == 2  # Widget A and Widget B
        for product in results:
            assert product.is_active is True
            assert 50 <= float(product.price) <= 150
