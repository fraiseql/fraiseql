"""Tests for dual-mode (dev/prod) repository instantiation."""

import os
from datetime import datetime
from typing import Any, Optional
from unittest.mock import patch
from uuid import UUID, uuid4

import pytest
import pytest_asyncio

# Import database fixtures for this database test
from .database_conftest import *  # noqa: F403

from fraiseql import fraise_field, fraise_type
from fraiseql.db import FraiseQLRepository


# Test types for dual-mode instantiation
@fraise_type
class Product:
    id: UUID
    name: str
    status: str
    category: Optional[str] = None
    created_at: datetime
    data: dict[str, Any]


@fraise_type
class User:
    id: UUID
    name: str
    email: str
    role: str = "user"


@fraise_type
class Order:
    id: UUID
    product_id: Optional[UUID] = None
    user_id: UUID
    data: dict[str, Any]

    # Nested relationships
    product: Optional[Product] = None
    user: User = fraise_field(default=None)

    # List relationships
    tags: list[str] = fraise_field(default_factory=list)


@fraise_type
class Project:
    id: UUID
    name: str
    lead_id: UUID

    # Circular reference test
    lead: Optional[User] = None
    members: list[User] = fraise_field(default_factory=list)
    orders: list[Order] = fraise_field(default_factory=list)


@pytest.mark.database
class TestDualModeRepository:
    """Test dual-mode instantiation in FraiseQLRepository."""

    @pytest_asyncio.fixture
    async def test_schema(self, db_connection):
        """Create test schema for dual-mode testing."""
        await db_connection.execute("""
            CREATE TABLE IF NOT EXISTS products (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                category TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                data JSONB DEFAULT '{}'::jsonb
            );

            CREATE TABLE IF NOT EXISTS users (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                role TEXT DEFAULT 'user'
            );

            CREATE TABLE IF NOT EXISTS orders (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                product_id UUID REFERENCES products(id),
                user_id UUID NOT NULL REFERENCES users(id),
                data JSONB DEFAULT '{}'::jsonb,
                tags TEXT[] DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS projects (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                lead_id UUID NOT NULL REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS project_members (
                project_id UUID NOT NULL REFERENCES projects(id),
                user_id UUID NOT NULL REFERENCES users(id),
                PRIMARY KEY (project_id, user_id)
            );

            -- Create views that mimic GraphQL return format (camelCase)
            CREATE OR REPLACE VIEW tv_product AS
            SELECT
                id,
                name,
                status,
                category,
                created_at as "createdAt",
                data
            FROM products;

            CREATE OR REPLACE VIEW tv_user AS
            SELECT
                id,
                name,
                email,
                role
            FROM users;

            CREATE OR REPLACE VIEW tv_order AS
            SELECT
                o.id,
                o.product_id as "productId",
                o.user_id as "userId",
                o.data,
                o.tags,
                -- Include nested data
                (SELECT row_to_json(p.*) FROM tv_product p WHERE p.id = o.product_id) as product,
                (SELECT row_to_json(u.*) FROM tv_user u WHERE u.id = o.user_id) as user
            FROM orders o;

            CREATE OR REPLACE VIEW tv_project AS
            SELECT
                p.id,
                p.name,
                p.lead_id as "leadId",
                (SELECT row_to_json(u.*) FROM tv_user u WHERE u.id = p.lead_id) as lead,
                COALESCE(
                    (SELECT json_agg(row_to_json(u.*))
                     FROM tv_user u
                     JOIN project_members pm ON u.id = pm.user_id
                     WHERE pm.project_id = p.id),
                    '[]'::json
                ) as members
            FROM projects p;
        """)
        await db_connection.commit()

    @pytest_asyncio.fixture
    async def test_data(self, db_connection, test_schema):
        """Insert test data and return IDs."""
        # Insert test users
        user1_id = uuid4()
        user2_id = uuid4()
        await db_connection.execute(
            """
            INSERT INTO users (id, name, email, role) VALUES
            (%s, %s, %s, %s),
            (%s, %s, %s, %s)
        """,
            (
                user1_id,
                "John Doe",
                "john@example.com",
                "admin",
                user2_id,
                "Jane Smith",
                "jane@example.com",
                "user",
            ),
        )

        # Insert test product
        product_id = uuid4()
        await db_connection.execute(
            """
            INSERT INTO products (id, name, status, category, data) VALUES
            (%s, %s, %s, %s, %s)
        """,
            (
                product_id,
                "Widget Pro",
                "available",
                "Electronics",
                '{"sku": "WP-123", "price": 99.99}',
            ),
        )

        # Insert test order
        order_id = uuid4()
        await db_connection.execute(
            """
            INSERT INTO orders (id, product_id, user_id, data, tags) VALUES
            (%s, %s, %s, %s, %s)
        """,
            (
                order_id,
                product_id,
                user1_id,
                '{"priority": "high", "quantity": 5}',
                ["urgent", "expedited", "bulk"],
            ),
        )

        # Insert test project
        project_id = uuid4()
        await db_connection.execute(
            """
            INSERT INTO projects (id, name, lead_id) VALUES
            (%s, %s, %s)
        """,
            (project_id, "Marketing Campaign", user1_id),
        )

        # Add project members
        await db_connection.execute(
            """
            INSERT INTO project_members (project_id, user_id) VALUES
            (%s, %s), (%s, %s)
        """,
            (project_id, user1_id, project_id, user2_id),
        )

        await db_connection.commit()

        return {
            "user1_id": user1_id,
            "user2_id": user2_id,
            "product_id": product_id,
            "order_id": order_id,
            "project_id": project_id,
        }

    @pytest.mark.asyncio
    async def test_production_mode_returns_raw_dicts(self, db_pool, test_data):
        """Test that production mode returns raw dictionary data."""
        # Arrange
        context = {"mode": "production"}
        repo = FraiseQLRepository(db_pool, context)

        # Act
        result = await repo.find("tv_order")

        # Assert
        assert len(result) == 1
        assert isinstance(result[0], dict)
        assert result[0]["id"] == str(test_data["order_id"])
        assert result[0]["product"]["name"] == "Widget Pro"
        assert isinstance(result[0]["product"], dict)
        assert isinstance(result[0]["user"], dict)
        assert result[0]["tags"] == ["urgent", "expedited", "bulk"]

    @pytest.mark.asyncio
    async def test_development_mode_returns_typed_objects(self, db_pool, test_data):
        """Test that development mode returns fully instantiated typed objects."""
        # Arrange
        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)

        # Mock type registry
        def mock_get_type_for_view(view_name):
            type_map = {
                "tv_order": Order,
                "tv_product": Product,
                "tv_user": User,
                "tv_project": Project,
            }
            return type_map.get(view_name)

        with patch.object(repo, "_get_type_for_view", side_effect=mock_get_type_for_view):
            # Act
            result = await repo.find("tv_order")

            # Assert
            assert len(result) == 1
            assert isinstance(result[0], Order)
            assert result[0].id == test_data["order_id"]
            assert isinstance(result[0].product, Product)
            assert result[0].product.name == "Widget Pro"
            assert isinstance(result[0].user, User)
            assert result[0].user.name == "John Doe"
            assert result[0].tags == ["urgent", "expedited", "bulk"]

    @pytest.mark.asyncio
    async def test_find_one_production_mode(self, db_pool, test_data):
        """Test find_one returns raw dict in production mode."""
        # Arrange
        context = {"mode": "production"}
        repo = FraiseQLRepository(db_pool, context)

        # Act
        result = await repo.find_one("tv_product", id=test_data["product_id"])

        # Assert
        assert isinstance(result, dict)
        assert result["name"] == "Widget Pro"
        assert result["status"] == "available"

    @pytest.mark.asyncio
    async def test_find_one_development_mode(self, db_pool, test_data):
        """Test find_one returns typed object in development mode."""
        # Arrange
        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)

        # Mock type registry
        with patch.object(repo, "_get_type_for_view", return_value=Product):
            # Act
            result = await repo.find_one("tv_product", id=test_data["product_id"])

            # Assert
            assert isinstance(result, Product)
            assert result.name == "Widget Pro"
            assert result.status == "available"
            assert result.category == "Electronics"

    @pytest.mark.asyncio
    async def test_find_one_returns_none_when_no_data(self, db_pool):
        """Test find_one returns None when no data found in both modes."""
        for mode in ["production", "development"]:
            # Arrange
            context = {"mode": mode}
            repo = FraiseQLRepository(db_pool, context)

            # Act
            result = await repo.find_one("tv_product", id=uuid4())

            # Assert
            assert result is None

    @pytest.mark.asyncio
    async def test_circular_reference_handling(self, db_pool, test_data):
        """Test that circular references are handled correctly in dev mode."""
        # Arrange
        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)

        # Mock type registry
        def mock_get_type_for_view(view_name):
            type_map = {
                "tv_project": Project,
                "tv_user": User,
            }
            return type_map.get(view_name)

        with patch.object(repo, "_get_type_for_view", side_effect=mock_get_type_for_view):
            # Act
            result = await repo.find_one("tv_project", id=test_data["project_id"])

            # Assert
            assert isinstance(result, Project)
            assert isinstance(result.lead, User)
            assert len(result.members) == 2
            assert all(isinstance(member, User) for member in result.members)
            # Check that the same user instance is reused
            lead_user = next((m for m in result.members if m.id == result.lead.id), None)
            assert lead_user is not None
            assert result.lead is lead_user

    @pytest.mark.asyncio
    async def test_max_recursion_depth_protection(self, db_pool):
        """Test that excessive recursion depth raises an error."""

        # Create a nested type
        @fraise_type
        class NestedType:
            id: UUID
            name: str
            nested: Optional["NestedType"] = None

        # Create deeply nested data structure
        def create_nested_data(depth):
            if depth == 0:
                return {"id": str(uuid4()), "name": "Base", "nested": None}
            return {
                "id": str(uuid4()),
                "name": f"Level {depth}",
                "nested": create_nested_data(depth - 1),
            }

        deep_data = create_nested_data(12)  # Exceed max depth of 10

        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)

        # Mock the data fetch
        with (
            patch.object(repo, "_get_type_for_view", return_value=NestedType),
            pytest.raises(ValueError, match="Max recursion depth exceeded"),
        ):
            # Manually call _instantiate_recursive since we can't easily mock the DB query
            repo._instantiate_recursive(NestedType, deep_data)

    def test_mode_detection_from_environment(self, db_pool):
        """Test mode detection from environment variables."""
        # Test production mode (default)
        with patch.dict(os.environ, {}, clear=True):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "production"

        # Test development mode
        with patch.dict(os.environ, {"FRAISEQL_ENV": "development"}):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "development"

        # Test explicit production
        with patch.dict(os.environ, {"FRAISEQL_ENV": "production"}):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "production"

    def test_mode_override_from_context(self, db_pool):
        """Test that context mode overrides environment."""
        # Environment says production, but context says development
        with patch.dict(os.environ, {"FRAISEQL_ENV": "production"}):
            context = {"mode": "development"}
            repo = FraiseQLRepository(db_pool, context)
            assert repo.mode == "development"

        # Environment says development, but context says production
        with patch.dict(os.environ, {"FRAISEQL_ENV": "development"}):
            context = {"mode": "production"}
            repo = FraiseQLRepository(db_pool, context)
            assert repo.mode == "production"

    @pytest.mark.asyncio
    async def test_camel_to_snake_case_conversion(self, db_pool, test_data):
        """Test that camelCase keys are converted to snake_case in dev mode."""
        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)

        # Mock type registry
        with patch.object(repo, "_get_type_for_view", return_value=Order):
            # Act
            result = await repo.find_one("tv_order", id=test_data["order_id"])

            # Assert
            assert hasattr(result, "product_id")  # snake_case
            assert hasattr(result, "user_id")  # snake_case
            assert result.product_id == test_data["product_id"]
            assert result.user_id == test_data["user1_id"]

    @pytest.mark.asyncio
    async def test_list_instantiation(self, db_pool, test_data):
        """Test that lists of typed objects are instantiated correctly."""
        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)

        # Mock type registry
        def mock_get_type_for_view(view_name):
            type_map = {
                "tv_project": Project,
                "tv_user": User,
            }
            return type_map.get(view_name)

        with patch.object(repo, "_get_type_for_view", side_effect=mock_get_type_for_view):
            # Act
            result = await repo.find_one("tv_project", id=test_data["project_id"])

            # Assert
            assert isinstance(result, Project)
            assert len(result.members) == 2
            for member in result.members:
                assert isinstance(member, User)
            assert {m.name for m in result.members} == {"John Doe", "Jane Smith"}

    @pytest.mark.parametrize("mode", ["development", "production"])
    @pytest.mark.asyncio
    async def test_both_modes_handle_null_fields(self, db_pool, test_data, mode):
        """Test that both modes correctly handle null/None fields."""
        # Insert a product with null category
        null_product_id = uuid4()
        async with db_pool.connection() as conn:
            await conn.execute(
                """
                INSERT INTO products (id, name, status, category, data) VALUES
                (%s, %s, %s, NULL, %s)
            """,
                (null_product_id, "Test Product", "available", "{}"),
            )
            await conn.commit()

        context = {"mode": mode}
        repo = FraiseQLRepository(db_pool, context)

        if mode == "development":
            with patch.object(repo, "_get_type_for_view", return_value=Product):
                result = await repo.find_one("tv_product", id=null_product_id)
                assert isinstance(result, Product)
                assert result.category is None
        else:
            result = await repo.find_one("tv_product", id=null_product_id)
            assert isinstance(result, dict)
            assert result["category"] is None
