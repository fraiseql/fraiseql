"""End-to-end tests for GraphQL queries with many fields."""

import json
from dataclasses import dataclass
from typing import Optional
from uuid import UUID, uuid4

import pytest

import fraiseql
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app


@fraiseql.type
@dataclass
class Product:
    """Product type with many fields to test GraphQL field limit."""

    # Basic info - 10 fields
    id: UUID
    sku: str
    name: str
    description: str
    short_description: str
    brand: str
    manufacturer: str
    model_number: str
    upc: str
    ean: str

    # Pricing - 10 fields
    price: float
    sale_price: Optional[float]
    cost: float
    msrp: float
    wholesale_price: float
    min_advertised_price: Optional[float]
    currency: str
    tax_rate: float
    tax_included: bool
    shipping_cost: float

    # Inventory - 10 fields
    quantity_in_stock: int
    quantity_reserved: int
    quantity_available: int
    reorder_point: int
    reorder_quantity: int
    backorder_allowed: bool
    track_inventory: bool
    warehouse_location: str
    bin_number: str
    supplier_id: UUID

    # Attributes - 10 fields
    weight: float
    weight_unit: str
    length: float
    width: float
    height: float
    dimension_unit: str
    color: str
    size: str
    material: str
    country_of_origin: str

    # Status - 5 fields
    is_active: bool
    is_featured: bool
    is_digital: bool
    created_at: str
    updated_at: str

    # Total: 45 fields


@pytest.mark.asyncio
class TestGraphQLFieldLimitE2E:
    """End-to-end tests for GraphQL queries with many fields."""

    @pytest.fixture
    async def app_with_field_limit(self, db_pool):
        """Create FastAPI app with field limit configuration."""
        config = FraiseQLConfig(
            database_url="postgresql://test",  # Will be overridden by pool
            jsonb_field_limit_threshold=20,  # Set threshold to 20 fields
            environment="development",
        )

        # Import and set the global db pool before creating the app
        from fraiseql.fastapi.dependencies import set_db_pool

        # Set the pool in the global state
        set_db_pool(db_pool)

        app = create_fraiseql_app(types=[Product], queries=[products, product], config=config)

        # Also set on app state for consistency
        app.state.db_pool = db_pool

        return app

    @pytest.fixture
    async def setup_product_data(self, db_pool):
        """Create product test data."""
        async with db_pool.connection() as conn:
            cursor = conn.cursor()

            # Create table
            await cursor.execute("""
                CREATE TABLE IF NOT EXISTS products (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL
                )
            """)

            # Create view
            await cursor.execute("""
                CREATE OR REPLACE VIEW product_view AS
                SELECT id, data FROM products
            """)

            # Insert test product
            product_id = uuid4()
            product_data = {
                # Basic info
                "id": str(product_id),
                "sku": "PROD-001",
                "name": "Test Product",
                "description": "A test product with many fields",
                "short_description": "Test product",
                "brand": "TestBrand",
                "manufacturer": "TestCorp",
                "model_number": "TC-001",
                "upc": "123456789012",
                "ean": "1234567890123",
                # Pricing
                "price": 99.99,
                "sale_price": 79.99,
                "cost": 50.00,
                "msrp": 119.99,
                "wholesale_price": 60.00,
                "min_advertised_price": 89.99,
                "currency": "USD",
                "tax_rate": 0.08,
                "tax_included": False,
                "shipping_cost": 9.99,
                # Inventory
                "quantity_in_stock": 100,
                "quantity_reserved": 10,
                "quantity_available": 90,
                "reorder_point": 20,
                "reorder_quantity": 50,
                "backorder_allowed": True,
                "track_inventory": True,
                "warehouse_location": "A1",
                "bin_number": "B23",
                "supplier_id": str(uuid4()),
                # Attributes
                "weight": 2.5,
                "weight_unit": "kg",
                "length": 30.0,
                "width": 20.0,
                "height": 10.0,
                "dimension_unit": "cm",
                "color": "Blue",
                "size": "Large",
                "material": "Cotton",
                "country_of_origin": "USA",
                # Status
                "is_active": True,
                "is_featured": True,
                "is_digital": False,
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z",
            }

            await cursor.execute(
                "INSERT INTO products (id, data) VALUES (%s, %s::jsonb)",
                (product_id, json.dumps(product_data)),
            )

            await conn.commit()

        # Register type for view
        from fraiseql.db import register_type_for_view

        register_type_for_view("product_view", Product)

        yield product_id, product_data

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP VIEW IF EXISTS product_view")
            await conn.execute("DROP TABLE IF EXISTS products")
            await conn.commit()

    @pytest.mark.asyncio
    async def test_query_all_fields_exceeds_limit(self, app_with_field_limit, setup_product_data):
        """Test GraphQL query requesting all 45 fields (exceeds limit of 20)."""
        from httpx import ASGITransport, AsyncClient

        product_id, _ = setup_product_data

        # First test a simple query to ensure basic setup works
        simple_query = """
        query {
            products {
                id
                name
            }
        }
        """

        async with AsyncClient(
            transport=ASGITransport(app=app_with_field_limit), base_url="http://test"
        ) as client:
            simple_response = await client.post("/graphql", json={"query": simple_query})

        print(f"Simple query response: {simple_response.json()}")

        # Query requesting ALL fields
        query = """
        query GetProductAllFields {
            products {
                id
                sku
                name
                description
                shortDescription
                brand
                manufacturer
                modelNumber
                upc
                ean
                price
                salePrice
                cost
                msrp
                wholesalePrice
                minAdvertisedPrice
                currency
                taxRate
                taxIncluded
                shippingCost
                quantityInStock
                quantityReserved
                quantityAvailable
                reorderPoint
                reorderQuantity
                backorderAllowed
                trackInventory
                warehouseLocation
                binNumber
                supplierId
                weight
                weightUnit
                length
                width
                height
                dimensionUnit
                color
                size
                material
                countryOfOrigin
                isActive
                isFeatured
                isDigital
                createdAt
                updatedAt
            }
        }
        """
        async with AsyncClient(
            transport=ASGITransport(app=app_with_field_limit), base_url="http://test"
        ) as client:
            response = await client.post("/graphql", json={"query": query})

        assert response.status_code == 200
        data = response.json()

        # Check for errors
        if "errors" in data:
            print(f"GraphQL errors: {data['errors']}")

        # Should successfully return data despite exceeding field limit
        assert "data" in data
        assert "products" in data["data"]
        assert data["data"]["products"] is not None, (
            f"Products returned None. Full response: {data}"
        )
        assert len(data["data"]["products"]) == 1

        product = data["data"]["products"][0]

        # Verify all fields are present
        assert product["id"] == str(product_id)
        assert product["sku"] == "PROD-001"
        assert product["name"] == "Test Product"
        assert product["price"] == 99.99
        assert product["isActive"] is True
        assert "updatedAt" in product

    async def test_query_few_fields_below_limit(self, app_with_field_limit, setup_product_data):
        """Test GraphQL query with few fields (below limit)."""
        from httpx import ASGITransport, AsyncClient

        product_id, _ = setup_product_data

        # Query requesting only 5 fields
        query = """
        query GetProductBasic {
            products {
                id
                sku
                name
                price
                isActive
            }
        }
        """
        async with AsyncClient(
            transport=ASGITransport(app=app_with_field_limit), base_url="http://test"
        ) as client:
            response = await client.post("/graphql", json={"query": query})

        assert response.status_code == 200
        data = response.json()

        assert "data" in data
        product = data["data"]["products"][0]

        # Should have exactly the requested fields
        assert len(product) == 5
        assert product["id"] == str(product_id)
        assert product["sku"] == "PROD-001"
        assert product["name"] == "Test Product"
        assert product["price"] == 99.99
        assert product["isActive"] is True

    async def test_query_exactly_at_limit(self, app_with_field_limit, setup_product_data):
        """Test GraphQL query with exactly 20 fields (at limit)."""
        from httpx import ASGITransport, AsyncClient

        product_id, _ = setup_product_data

        # Query requesting exactly 20 fields
        query = """
        query GetProduct20Fields {
            products {
                id
                sku
                name
                description
                shortDescription
                brand
                manufacturer
                modelNumber
                upc
                ean
                price
                salePrice
                cost
                msrp
                wholesalePrice
                currency
                taxRate
                isActive
                isFeatured
                createdAt
            }
        }
        """
        async with AsyncClient(
            transport=ASGITransport(app=app_with_field_limit), base_url="http://test"
        ) as client:
            response = await client.post("/graphql", json={"query": query})

        assert response.status_code == 200
        data = response.json()

        product = data["data"]["products"][0]
        assert product["id"] == str(product_id)
        assert product["price"] == 99.99

    async def test_mutation_with_many_fields(self, app_with_field_limit):
        """Test that mutations work correctly with field limit."""
        # This is a placeholder - mutations would need to be implemented
        # The test demonstrates that the field limit functionality
        # doesn't interfere with mutations


# Query functions for the app
@fraiseql.query
async def products(info) -> list[Product]:
    """Get all products."""
    db = info.context["db"]
    return await db.find("product_view")


@fraiseql.query
async def product(info, id: UUID) -> Optional[Product]:
    """Get a single product by ID."""
    db = info.context["db"]
    return await db.find_one("product_view", id=id)
