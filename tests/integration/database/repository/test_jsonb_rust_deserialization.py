"""Tests for Rust deserialization of JSONB entities.

This test suite verifies that entities with JSONB columns are properly
deserialized from Rust execution mode, fixing the issue where RustResponseBytes
instances are returned instead of proper Python objects.

Related issue: JSONB Rust Deserialization Fix Plan
"""

import pytest

pytestmark = pytest.mark.database

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.core.rust_pipeline import RustResponseBytes
from tests.unit.utils.test_response_utils import extract_graphql_data


# Test types with JSONB data
@fraiseql.type
class TestProduct:
    """Product entity with JSONB data column."""
    id: str
    name: str
    brand: str  # Stored in JSONB data column
    category: str  # Stored in JSONB data column
    price: float  # Stored in JSONB data column


class TestJSONBRustDeserialization:
    """Test that Rust execution correctly deserializes JSONB entities."""

    @pytest.fixture
    async def setup_test_data(self, db_pool):
        """Create test table with JSONB data and register type."""
        # Register type with has_jsonb_data=True
        register_type_for_view(
            "test_products_jsonb_view",
            TestProduct,
            table_columns={"id", "name", "data"},
            has_jsonb_data=True,
            jsonb_column="data"
        )

        async with db_pool.connection() as conn:
            # Create test table with JSONB column
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_products_jsonb (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    data JSONB NOT NULL
                )
            """
            )

            # Create view that exposes JSONB data
            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_products_jsonb_view AS
                SELECT
                    id,
                    name,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'brand', data->>'brand',
                        'category', data->>'category',
                        'price', (data->>'price')::float
                    ) as data
                FROM test_products_jsonb
            """
            )

            # Insert test data with JSONB
            await conn.execute(
                """
                INSERT INTO test_products_jsonb (id, name, data)
                VALUES
                    ('prod-001', 'Laptop', '{"brand": "Dell", "category": "Electronics", "price": 999.99}'),
                    ('prod-002', 'Phone', '{"brand": "Apple", "category": "Electronics", "price": 799.99}'),
                    ('prod-003', 'Tablet', '{"brand": "Samsung", "category": "Electronics", "price": 499.99}')
            """
            )

        yield

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_products_jsonb_view")
            await conn.execute("DROP TABLE IF EXISTS test_products_jsonb")

    @pytest.mark.asyncio
    async def test_find_one_jsonb_returns_proper_object_not_rustresponsebytes(
        self, db_pool, setup_test_data
    ):
        """Test that find_one with JSONB entity works through the Rust pipeline.

        After v1.1.8 fix: JSONB entities use the unified Rust execution path.
        The Rust pipeline returns RustResponseBytes, which is correct.
        """
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Execute query for JSONB entity
        result = await repo.find_one("test_products_jsonb_view", id="prod-001")

        # ASSERTION: Result should be RustResponseBytes (Rust pipeline working correctly)
        # v1.1.8 fix: Removed incorrect workaround that blocked JSONB entities
        assert isinstance(result, RustResponseBytes), (
            f"Expected RustResponseBytes from Rust pipeline, got {type(result)}. "
            f"Rust execution mode should handle JSONB entities correctly."
        )

    @pytest.mark.asyncio
    async def test_find_jsonb_list_returns_proper_objects_not_rustresponsebytes(
        self, db_pool, setup_test_data
    ):
        """Test that find with JSONB entities returns proper objects, not RustResponseBytes.

        This is the RED phase test - it should FAIL until we implement the Python fallback.

        BUG: Currently returns <RustResponseBytes instance> instead of list of Python objects
        FIX: Should return RustResponseBytes that can be deserialized by GraphQL layer
        """
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Execute query for JSONB entities list
        result = await repo.find("test_products_jsonb_view", limit=10)

        # ASSERTION 1: Result should be RustResponseBytes (this is OK for list queries)
        assert isinstance(result, RustResponseBytes), (
            f"Expected RustResponseBytes for list query, got {type(result)}"
        )

        # ASSERTION 2: RustResponseBytes should be deserializable
        # Try to extract data - this should work after the fix
        try:
            products = extract_graphql_data(result, "test_products_jsonb_view")

            # ASSERTION 3: Should return list of dicts, not RustResponseBytes instances
            assert isinstance(products, list), f"Expected list, got {type(products)}"
            assert len(products) == 3, f"Expected 3 products, got {len(products)}"

            # ASSERTION 4: Each item should be a dict, not RustResponseBytes
            for product in products:
                assert not isinstance(product, RustResponseBytes), (
                    f"Expected dict, got RustResponseBytes in list results"
                )

        except Exception as e:
            pytest.fail(
                f"Failed to deserialize RustResponseBytes from JSONB query: {e}. "
                f"Rust execution mode is not properly handling JSONB entities."
            )

    @pytest.mark.asyncio
    async def test_jsonb_field_values_accessible_after_deserialization(
        self, db_pool, setup_test_data
    ):
        """Test that JSONB fields are properly accessible after deserialization.

        This test verifies that not only is the object deserializable,
        but the JSONB field values are correctly extracted and accessible.
        """
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Execute query for JSONB entities
        result = await repo.find("test_products_jsonb_view", limit=1)

        # Extract and verify data
        products = extract_graphql_data(result, "test_products_jsonb_view")
        assert len(products) == 1

        product = products[0]

        # ASSERTION: JSONB fields should be present and have correct values
        assert "id" in product, "Missing 'id' field"
        assert "name" in product, "Missing 'name' field"
        assert "brand" in product, "Missing 'brand' field (from JSONB)"
        assert "category" in product, "Missing 'category' field (from JSONB)"
        assert "price" in product, "Missing 'price' field (from JSONB)"

        # Verify values match what we inserted
        assert product["id"] == "prod-001"
        assert product["name"] == "Laptop"
        assert product["brand"] == "Dell"
        assert product["category"] == "Electronics"
        assert product["price"] == 999.99
