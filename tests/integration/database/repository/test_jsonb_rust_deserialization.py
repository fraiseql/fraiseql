"""Tests for Rust deserialization of JSONB entities.

This test suite verifies that entities with JSONB columns are properly
deserialized from Rust execution mode, fixing the issue where RustResponseBytes
instances are returned instead of proper Python objects.

Related issue: JSONB Rust Deserialization Fix Plan
"""

import pytest
import pytest_asyncio

pytestmark = pytest.mark.database

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql.db import register_type_for_view


# Test types with JSONB data
@fraiseql.type
class Product:
    """Product entity with JSONB data column."""

    id: str
    name: str
    brand: str  # Stored in JSONB data column
    category: str  # Stored in JSONB data column
    price: float  # Stored in JSONB data column


class TestJSONBRustDeserialization:
    """Test that Rust execution correctly deserializes JSONB entities."""

    @pytest_asyncio.fixture(scope="class")
    async def setup_test_data(self, class_db_pool, test_schema) -> None:
        """Create test table with JSONB data and register type."""
        # Register type with has_jsonb_data=True
        register_type_for_view(
            "test_products_jsonb_view",
            Product,
            table_columns={"id", "name", "data"},
            has_jsonb_data=True,
            jsonb_column="data",
        )

        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
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
