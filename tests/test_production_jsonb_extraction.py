"""Test JSONB extraction in production mode."""

from uuid import uuid4

import pytest

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view


@fraiseql.type
class TestProduct:
    """Test product type."""

    id: str
    name: str
    price: float
    category: str | None = None


@fraiseql.type
class LegacyProduct:
    """Type for testing backward compatibility with non-JSONB tables."""

    id: str
    name: str
    price: float


class TestProductionJSONBExtraction:
    """Test that production mode properly extracts JSONB data."""

    @pytest.fixture
    async def setup_test_data(self, test_db):
        """Create test tables with and without JSONB columns."""
        # Register types
        register_type_for_view("products_jsonb", TestProduct)
        register_type_for_view("products_legacy", LegacyProduct)

        # Create JSONB-based table
        await test_db.execute("""
            CREATE TABLE IF NOT EXISTS products_jsonb (
                id UUID PRIMARY KEY,
                tenant_id UUID,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                data JSONB NOT NULL
            )
        """)

        # Create legacy table without JSONB
        await test_db.execute("""
            CREATE TABLE IF NOT EXISTS products_legacy (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                price NUMERIC(10, 2) NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
        """)

        # Insert test data
        product_id = str(uuid4())
        await test_db.execute(
            """
            INSERT INTO products_jsonb (id, tenant_id, data)
            VALUES ($1, $2, $3)
            """,
            product_id,
            str(uuid4()),
            {
                "id": product_id,
                "name": "Test Product",
                "price": 99.99,
                "category": "Electronics",
            },
        )

        legacy_id = str(uuid4())
        await test_db.execute(
            """
            INSERT INTO products_legacy (id, name, price)
            VALUES ($1, $2, $3)
            """,
            legacy_id,
            "Legacy Product",
            49.99,
        )

        yield {"jsonb_id": product_id, "legacy_id": legacy_id}

        # Cleanup
        await test_db.execute("DROP TABLE IF EXISTS products_jsonb")
        await test_db.execute("DROP TABLE IF EXISTS products_legacy")

    @pytest.mark.database
    async def test_production_mode_extracts_jsonb(self, test_pool, setup_test_data):
        """Test that production mode extracts JSONB data automatically."""
        # Force production mode
        repo = FraiseQLRepository(pool=test_pool, context={"mode": "production"})

        # Query JSONB table
        results = await repo.find("products_jsonb")

        assert len(results) == 1
        result = results[0]

        # Should get the extracted JSONB data, not the full row
        assert isinstance(result, dict)
        assert "data" not in result  # Should not have nested data column
        assert result["id"] == setup_test_data["jsonb_id"]
        assert result["name"] == "Test Product"
        assert result["price"] == 99.99
        assert result["category"] == "Electronics"

        # Should NOT have database metadata columns
        assert "tenant_id" not in result
        assert "created_at" not in result

    @pytest.mark.database
    async def test_production_mode_handles_legacy_tables(self, test_pool, setup_test_data):
        """Test backward compatibility with non-JSONB tables."""
        # Force production mode
        repo = FraiseQLRepository(pool=test_pool, context={"mode": "production"})

        # Query legacy table (no JSONB column)
        results = await repo.find("products_legacy")

        assert len(results) == 1
        result = results[0]

        # Should get the full row since there's no data column
        assert isinstance(result, dict)
        assert result["id"] == setup_test_data["legacy_id"]
        assert result["name"] == "Legacy Product"
        assert result["price"] == 49.99
        assert "created_at" in result  # Metadata columns are included

    @pytest.mark.database
    async def test_production_mode_find_one_extracts_jsonb(self, test_pool, setup_test_data):
        """Test that find_one also extracts JSONB in production mode."""
        # Force production mode
        repo = FraiseQLRepository(pool=test_pool, context={"mode": "production"})

        # Query single record from JSONB table
        result = await repo.find_one("products_jsonb", id=setup_test_data["jsonb_id"])

        assert result is not None
        assert isinstance(result, dict)
        assert "data" not in result  # Should not have nested data column
        assert result["id"] == setup_test_data["jsonb_id"]
        assert result["name"] == "Test Product"
        assert result["price"] == 99.99

    @pytest.mark.database
    async def test_development_mode_unchanged(self, test_pool, setup_test_data):
        """Test that development mode still instantiates objects."""
        # Force development mode
        repo = FraiseQLRepository(pool=test_pool, context={"mode": "development"})

        # Query JSONB table
        results = await repo.find("products_jsonb")

        assert len(results) == 1
        result = results[0]

        # Should get instantiated object in development mode
        assert isinstance(result, TestProduct)
        assert result.id == setup_test_data["jsonb_id"]
        assert result.name == "Test Product"
        assert result.price == 99.99
        assert result.category == "Electronics"

    @pytest.mark.database
    async def test_production_mode_empty_results(self, test_pool):
        """Test that empty results are handled correctly."""
        # Force production mode
        repo = FraiseQLRepository(pool=test_pool, context={"mode": "production"})

        # Create empty table
        await test_pool.execute("""
            CREATE TABLE IF NOT EXISTS empty_products (
                id UUID PRIMARY KEY,
                data JSONB NOT NULL
            )
        """)

        try:
            results = await repo.find("empty_products")
            assert results == []
        finally:
            await test_pool.execute("DROP TABLE IF EXISTS empty_products")

    @pytest.mark.database
    async def test_environment_variable_mode(self, test_pool, setup_test_data, monkeypatch):
        """Test that FRAISEQL_ENV=production enables JSONB extraction."""
        # Set environment variable
        monkeypatch.setenv("FRAISEQL_ENV", "production")

        # Create repo without explicit mode
        repo = FraiseQLRepository(pool=test_pool)

        # Query JSONB table
        results = await repo.find("products_jsonb")

        assert len(results) == 1
        result = results[0]

        # Should get extracted JSONB data
        assert isinstance(result, dict)
        assert "data" not in result
        assert result["name"] == "Test Product"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
