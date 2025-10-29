"""Test regex operators in StringFilter.

This test suite ensures that regex matching operators work correctly
in FraiseQL WHERE filtering.
"""

import pytest

pytestmark = pytest.mark.database

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql.where_generator import safe_create_where_type
from tests.unit.utils.test_response_utils import extract_graphql_data


# Test types
@fraiseql.type
class Product:
    id: str
    name: str
    description: str


# Generate where types
ProductWhere = safe_create_where_type(Product)


class TestStringFilterRegex:
    """Test suite for regex operators in StringFilter."""

    @pytest.fixture
    async def setup_test_views(self, db_pool):
        """Create test views with proper structure."""
        # Register types for views (for development mode)
        register_type_for_view("test_product_view", Product)

        async with db_pool.connection() as conn:
            # Create tables
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_products (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL
                )
            """
            )

            # Create views with JSONB data column
            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_product_view AS
                SELECT
                    id, name, description,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'description', description
                    ) as data
                FROM test_products
            """
            )

            # Insert test data with regex-friendly patterns
            await conn.execute(
                """
                INSERT INTO test_products (id, name, description)
                VALUES
                    ('prod-001', 'Widget Alpha', 'A high-quality widget for testing'),
                    ('prod-002', 'Widget Beta', 'Another widget with different features'),
                    ('prod-003', 'Gadget Gamma', 'A gadget that starts with G'),
                    ('prod-004', 'Tool Delta', 'Tool for development work'),
                    ('prod-005', 'Widget123', 'Widget with numbers in name')
            """
            )

        yield

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_product_view")
            await conn.execute("DROP TABLE IF EXISTS test_products")

    @pytest.mark.asyncio
    async def test_matches_operator_basic_regex(self, db_pool, setup_test_views):
        """Test basic regex matching with matches operator."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test regex pattern that matches names starting with 'Widget'
        where = ProductWhere(name={"matches": "^Widget"})

        # This should fail initially because 'matches' field doesn't exist in StringFilter
        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: 3 products (Widget Alpha, Widget Beta, Widget123)
        assert len(results) == 3
        assert all(r["name"].startswith("Widget") for r in results)

    @pytest.mark.asyncio
    async def test_matches_operator_case_sensitive(self, db_pool, setup_test_views):
        """Test case-sensitive regex matching."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test case-sensitive pattern (should not match lowercase)
        where = ProductWhere(name={"matches": "^[A-Z]"})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: All 5 products (all start with capital letters)
        assert len(results) == 5

    @pytest.mark.asyncio
    async def test_matches_operator_with_numbers(self, db_pool, setup_test_views):
        """Test regex matching with numeric patterns."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test pattern that matches names containing digits
        where = ProductWhere(name={"matches": "\\d+"})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: 1 product (Widget123)
        assert len(results) == 1
        assert results[0]["name"] == "Widget123"

    @pytest.mark.asyncio
    async def test_imatches_operator_case_insensitive_regex(self, db_pool, setup_test_views):
        """Test case-insensitive regex matching with imatches operator."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test case-insensitive pattern that should match names starting with 'widget' (lowercase)
        where = ProductWhere(name={"imatches": "^widget"})

        # This should fail initially because 'imatches' field doesn't exist in StringFilter
        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: 3 products (Widget Alpha, Widget Beta, Widget123 - case insensitive match)
        assert len(results) == 3
        assert all(r["name"].lower().startswith("widget") for r in results)

    @pytest.mark.asyncio
    async def test_not_matches_operator_negative_regex(self, db_pool, setup_test_views):
        """Test negative regex matching with not_matches operator."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test pattern that excludes names starting with 'Widget'
        where = ProductWhere(name={"not_matches": "^Widget"})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: 2 products (Gadget Gamma, Tool Delta - excluding Widget products)
        assert len(results) == 2
        assert all(not r["name"].startswith("Widget") for r in results)
