"""Tests for PostgreSQL JSONB filtering capabilities."""

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
    attributes: dict  # JSONB column


# Generate where types
ProductWhere = safe_create_where_type(Product)


class TestJSONBKeyExistence:
    """Test PostgreSQL JSONB key existence operators."""

    @pytest.fixture
    async def setup_test_products(self, db_pool):
        """Create test products with JSONB attributes."""
        # Register types for views (for development mode)
        register_type_for_view("test_products_view", Product)

        async with db_pool.connection() as conn:
            # Create tables
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_products (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    attributes JSONB NOT NULL
                )
            """
            )

            # Create views with JSONB data column
            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_products_view AS
                SELECT
                    id, name, attributes,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'attributes', attributes
                    ) as data
                FROM test_products
            """
            )

            # Insert test data with JSONB attributes
            await conn.execute(
                """
                INSERT INTO test_products (id, name, attributes)
                VALUES
                    ('prod-001', 'Laptop', '{"brand": "Dell", "ram": "16GB", "ssd": "512GB"}'),
                    ('prod-002', 'Phone', '{"brand": "Apple", "storage": "128GB", "color": "black"}'),
                    ('prod-003', 'Tablet', '{"brand": "Samsung", "storage": "64GB"}')
            """
            )

        yield

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_products_view")
            await conn.execute("DROP TABLE IF EXISTS test_products")

    @pytest.mark.asyncio
    async def test_has_key_operator(self, db_pool, setup_test_products):
        """Test JSONB ? operator for key existence."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test has_key operator - find products with "ram" key
        where = ProductWhere(attributes={"has_key": "ram"})

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match only Laptop
        assert len(products) == 1
        assert products[0]["name"] == "Laptop"

    @pytest.mark.asyncio
    async def test_has_any_keys_operator(self, db_pool, setup_test_products):
        """Test JSONB ?| operator for any key existence."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test has_any_keys operator - find products with either "ram" OR "storage"
        where = ProductWhere(attributes={"has_any_keys": ["ram", "storage"]})

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match Laptop (has ram), Phone (has storage), and Tablet (has storage)
        assert len(products) == 3
        names = {p["name"] for p in products}
        assert names == {"Laptop", "Phone", "Tablet"}

    @pytest.mark.asyncio
    async def test_has_all_keys_operator(self, db_pool, setup_test_products):
        """Test JSONB ?& operator for all keys existence."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test has_all_keys operator - find products with BOTH "brand" AND "storage"
        where = ProductWhere(attributes={"has_all_keys": ["brand", "storage"]})

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match Phone and Tablet (both have brand and storage)
        assert len(products) == 2
        names = {p["name"] for p in products}
        assert names == {"Phone", "Tablet"}


class TestJSONBContainment:
    """Test PostgreSQL JSONB containment operators."""

    @pytest.fixture
    async def setup_test_products(self, db_pool):
        """Create test products with JSONB attributes."""
        register_type_for_view("test_products_view", Product)

        async with db_pool.connection() as conn:
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_products (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    attributes JSONB NOT NULL
                )
            """
            )

            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_products_view AS
                SELECT
                    id, name, attributes,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'attributes', attributes
                    ) as data
                FROM test_products
            """
            )

            await conn.execute(
                """
                INSERT INTO test_products (id, name, attributes)
                VALUES
                    ('prod-001', 'Laptop', '{"brand": "Dell", "specs": {"ram": "16GB", "ssd": "512GB"}}'),
                    ('prod-002', 'Phone', '{"brand": "Apple", "specs": {"storage": "128GB", "color": "black"}}'),
                    ('prod-003', 'Tablet', '{"brand": "Samsung"}')
            """
            )

        yield

        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_products_view")
            await conn.execute("DROP TABLE IF EXISTS test_products")

    @pytest.mark.asyncio
    async def test_contains_operator(self, db_pool, setup_test_products):
        """Test JSONB @> operator for containment."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test contains operator - find products that contain {"brand": "Apple"}
        where = ProductWhere(attributes={"contains": {"brand": "Apple"}})

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match only Phone
        assert len(products) == 1
        assert products[0]["name"] == "Phone"

    @pytest.mark.asyncio
    async def test_contained_by_operator(self, db_pool, setup_test_products):
        """Test JSONB <@ operator for contained by."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test contained_by operator - find products contained by a larger object
        where = ProductWhere(
            attributes={
                "contained_by": {"brand": "Samsung", "extra_field": "value", "another": "field"}
            }
        )

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match Tablet (its attributes are contained by the larger object)
        assert len(products) == 1
        assert products[0]["name"] == "Tablet"


class TestJSONBJSONPath:
    """Test PostgreSQL JSONB JSONPath operators."""

    @pytest.fixture
    async def setup_test_products(self, db_pool):
        """Create test products with nested JSONB."""
        register_type_for_view("test_products_view", Product)

        async with db_pool.connection() as conn:
            await conn.execute(
                """
                CREATE TABLE IF NOT EXISTS test_products (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    attributes JSONB NOT NULL
                )
            """
            )

            await conn.execute(
                """
                CREATE OR REPLACE VIEW test_products_view AS
                SELECT
                    id, name, attributes,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'attributes', attributes
                    ) as data
                FROM test_products
            """
            )

            await conn.execute(
                """
                INSERT INTO test_products (id, name, attributes)
                VALUES
                    ('prod-001', 'Laptop', '{"specs": {"ram": 16, "ssd": 512}}'),
                    ('prod-002', 'Phone', '{"specs": {"storage": 128}}'),
                    ('prod-003', 'Tablet', '{"price": 299}')
            """
            )

        yield

        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS test_products_view")
            await conn.execute("DROP TABLE IF EXISTS test_products")

    @pytest.mark.asyncio
    async def test_path_exists_operator(self, db_pool, setup_test_products):
        """Test JSONB @? operator for JSONPath existence."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test path_exists operator - find products with $.specs.ram path
        where = ProductWhere(attributes={"path_exists": "$.specs.ram"})

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match only Laptop
        assert len(products) == 1
        assert products[0]["name"] == "Laptop"

    @pytest.mark.asyncio
    async def test_path_match_operator(self, db_pool, setup_test_products):
        """Test JSONB @@ operator for JSONPath predicates."""
        repo = FraiseQLRepository(db_pool, context={"mode": "development"})

        # Test path_match operator - find products where specs.ram > 8
        where = ProductWhere(attributes={"path_match": "$.specs.ram > 8"})

        result = await repo.find("test_products_view", where=where)
        products = extract_graphql_data(result, "test_products_view")

        # Should match Laptop (ram is 16)
        assert len(products) == 1
        assert products[0]["name"] == "Laptop"
