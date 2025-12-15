"""Test array filtering operators in ArrayFilter.

This test suite ensures that PostgreSQL array filtering operators work correctly
in FraiseQL WHERE filtering.
"""

import pytest
import pytest_asyncio

pytestmark = pytest.mark.database

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403
from tests.unit.utils.test_response_utils import extract_graphql_data

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.sql.where_generator import safe_create_where_type


class TestArrayFilter:
    """Test suite for array operators in ArrayFilter."""

    @pytest.fixture(scope="class")
    def test_types(self, clear_registry_class):
        """Create test types inside a class-scoped fixture for proper isolation."""

        @fraiseql.type
        class Product:
            id: str
            name: str
            tags: list[str]  # Array field for testing

        ProductWhere = safe_create_where_type(Product)

        return {
            "Product": Product,
            "ProductWhere": ProductWhere,
        }

    @pytest_asyncio.fixture(scope="class")
    async def setup_test_views(self, class_db_pool, test_schema, test_types) -> None:
        """Create test views with array data.

        Uses the class-isolated schema and connection pool provided by
        class_db_pool. Schema is automatically cleaned up after
        the test class completes.
        """
        Product = test_types["Product"]
        # Register types for views (for development mode)
        register_type_for_view("test_product_view", Product)

        # Create tables in the test schema
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")

            await conn.execute(
                """
                CREATE TABLE test_products (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    tags TEXT[] NOT NULL
                )
            """
            )

            # Create views with JSONB data column
            await conn.execute(
                """
                CREATE VIEW test_product_view AS
                SELECT
                    id, name, tags,
                    jsonb_build_object(
                        'id', id,
                        'name', name,
                        'tags', tags
                    ) as data
                FROM test_products
            """
            )

            # Insert test data with array values
            await conn.execute(
                """
                INSERT INTO test_products (id, name, tags)
                VALUES
                    ('prod-001', 'Widget A', ARRAY['electronics', 'gadget']),
                    ('prod-002', 'Widget B', ARRAY['electronics', 'tool']),
                    ('prod-003', 'Book X', ARRAY['book', 'education']),
                    ('prod-004', 'Tool Y', ARRAY['tool', 'hardware']),
                    ('prod-005', 'Gadget Z', ARRAY['electronics', 'gadget', 'premium'])
            """
            )
            await conn.commit()

        yield

        # Cleanup happens automatically when test schema is dropped
        # (no need for explicit cleanup here)

    @pytest.mark.asyncio
    async def test_array_eq_operator_basic_equality(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        """Test basic array equality with eq operator."""
        ProductWhere = test_types["ProductWhere"]
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test exact array match
        where = ProductWhere(tags={"eq": ["electronics", "gadget"]})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        print(f"DEBUG: Found {len(results)} results")
        for r in results:
            print(f"  - {r['name']}: {r['tags']}")

        # Expected: 1 product (Widget A has exact match ['electronics', 'gadget'])
        assert len(results) == 1
        assert results[0]["name"] == "Widget A"
        assert results[0]["tags"] == ["electronics", "gadget"]

    @pytest.mark.asyncio
    async def test_array_neq_operator_inequality(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        """Test array inequality with neq operator."""
        ProductWhere = test_types["ProductWhere"]
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test array not equal
        where = ProductWhere(tags={"neq": ["electronics", "gadget"]})

        # This should fail initially because ArrayFilter type not defined
        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: 4 products (all except Widget A)
        assert len(results) == 4
        assert all(r["name"] != "Widget A" for r in results)

    @pytest.mark.asyncio
    async def test_array_contains_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array contains operator (@> in PostgreSQL)."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test contains: find arrays that contain ['electronics']
        where = ProductWhere(tags={"contains": ["electronics"]})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Widget A, Widget B, Gadget Z (all have 'electronics')
        assert len(results) == 3
        product_names = {r["name"] for r in results}
        assert product_names == {"Widget A", "Widget B", "Gadget Z"}

    @pytest.mark.asyncio
    async def test_array_contained_by_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array contained_by operator (<@ in PostgreSQL)."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test contained_by: find arrays contained by ['electronics', 'gadget', 'premium']
        where = ProductWhere(tags={"contained_by": ["electronics", "gadget", "premium"]})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Widget A and Gadget Z (both are subsets of search array)
        assert len(results) == 2
        product_names = {r["name"] for r in results}
        assert product_names == {"Widget A", "Gadget Z"}

    @pytest.mark.asyncio
    async def test_array_overlaps_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array overlaps operator (&& in PostgreSQL)."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test overlaps: find arrays that overlap with ['tool', 'hardware']
        where = ProductWhere(tags={"overlaps": ["tool", "hardware"]})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Widget B (has 'tool'), Tool Y (has 'tool' and 'hardware')
        assert len(results) == 2
        product_names = {r["name"] for r in results}
        assert product_names == {"Widget B", "Tool Y"}

    @pytest.mark.asyncio
    async def test_array_length_eq_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array length equality operator."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test len_eq: find arrays with exactly 2 elements
        where = ProductWhere(tags={"len_eq": 2})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Widget A, Widget B, Book X, Tool Y (all have 2 tags)
        assert len(results) == 4
        product_names = {r["name"] for r in results}
        assert product_names == {"Widget A", "Widget B", "Book X", "Tool Y"}

    @pytest.mark.asyncio
    async def test_array_length_gt_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array length greater than operator."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test len_gt: find arrays with more than 2 elements
        where = ProductWhere(tags={"len_gt": 2})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Gadget Z (3 tags)
        assert len(results) == 1
        assert results[0]["name"] == "Gadget Z"

    @pytest.mark.asyncio
    async def test_array_length_lt_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array length less than operator."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test len_lt: find arrays with less than 3 elements
        where = ProductWhere(tags={"len_lt": 3})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Widget A (2), Widget B (2), Book X (2), Tool Y (2)
        assert len(results) == 4
        product_names = {r["name"] for r in results}
        assert product_names == {"Widget A", "Widget B", "Book X", "Tool Y"}

    @pytest.mark.asyncio
    async def test_array_length_gte_operator(
        self, class_db_pool, setup_test_views, test_types
    ) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array length greater than or equal operator."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test len_gte: find arrays with 3 or more elements
        where = ProductWhere(tags={"len_gte": 3})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Gadget Z (3 tags)
        assert len(results) == 1
        assert results[0]["name"] == "Gadget Z"

    @pytest.mark.asyncio
    async def test_array_any_eq_operator(self, class_db_pool, setup_test_views, test_types) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array any_eq operator (ANY element equals value)."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test any_eq: find arrays where any element equals 'electronics'
        where = ProductWhere(tags={"any_eq": "electronics"})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: Widget A, Widget B, Gadget Z (all contain 'electronics')
        assert len(results) == 3
        product_names = {r["name"] for r in results}
        assert product_names == {"Widget A", "Widget B", "Gadget Z"}

    @pytest.mark.asyncio
    async def test_array_all_eq_operator(self, class_db_pool, setup_test_views, test_types) -> None:
        ProductWhere = test_types["ProductWhere"]
        """Test array all_eq operator (ALL elements equal value)."""
        repo = FraiseQLRepository(class_db_pool, context={"mode": "development"})

        # Test all_eq: find arrays where all elements equal 'electronics'
        where = ProductWhere(tags={"all_eq": "electronics"})

        result = await repo.find("test_product_view", where=where)
        results = extract_graphql_data(result, "test_product_view")

        # Expected: None (no array has all elements equal to 'electronics')
        assert len(results) == 0

    def test_graphql_schema_includes_array_filter(self, test_types) -> None:
        """Test that GraphQL schema generation includes ArrayFilter input type."""
        from graphql import print_schema

        from fraiseql.gql.schema_builder import build_fraiseql_schema
        from fraiseql.sql.graphql_where_generator import create_graphql_where_input

        Product = test_types["Product"]

        # Create a WHERE input type for Product that includes array filtering
        ProductWhereInput = create_graphql_where_input(Product)

        # Create a query that accepts a WHERE parameter
        @fraiseql.query
        async def products(where: ProductWhereInput | None = None) -> list[Product]:
            """Get all products with optional filtering."""
            return []

        # Build the schema
        schema = build_fraiseql_schema(query_types=[products])

        # Get schema SDL
        schema_sdl = print_schema(schema)

        # Verify ArrayFilter input type is present in the schema
        assert "input ArrayFilter" in schema_sdl, (
            "ArrayFilter input type missing from GraphQL schema"
        )

        # Verify specific array operators are present (note: GraphQL uses camelCase)
        assert "eq: [String]" in schema_sdl, "ArrayFilter.eq field missing"
        assert "neq: [String]" in schema_sdl, "ArrayFilter.neq field missing"
        assert "isnull: Boolean" in schema_sdl, "ArrayFilter.isnull field missing"
        assert "contains: [String]" in schema_sdl, "ArrayFilter.contains field missing"
        assert "containedBy: [String]" in schema_sdl, "ArrayFilter.containedBy field missing"
        assert "overlaps: [String]" in schema_sdl, "ArrayFilter.overlaps field missing"
        assert "lenEq: Int" in schema_sdl, "ArrayFilter.lenEq field missing"
        assert "lenNeq: Int" in schema_sdl, "ArrayFilter.lenNeq field missing"
        assert "lenGt: Int" in schema_sdl, "ArrayFilter.lenGt field missing"
        assert "lenGte: Int" in schema_sdl, "ArrayFilter.lenGte field missing"
        assert "lenLt: Int" in schema_sdl, "ArrayFilter.lenLt field missing"
        assert "lenLte: Int" in schema_sdl, "ArrayFilter.lenLte field missing"
        assert "anyEq: String" in schema_sdl, "ArrayFilter.anyEq field missing"
        assert "allEq: String" in schema_sdl, "ArrayFilter.allEq field missing"
