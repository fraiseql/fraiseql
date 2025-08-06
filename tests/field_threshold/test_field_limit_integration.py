"""Integration tests for field limit threshold functionality."""

import json
from dataclasses import dataclass
from typing import Optional
from uuid import UUID, uuid4

import pytest

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view


@fraiseql.type
@dataclass
class UserWithManyFields:
    """User type with many fields to test field limit."""

    id: UUID
    username: str
    email: str
    first_name: str
    last_name: str
    middle_name: Optional[str]
    display_name: str
    phone: Optional[str]
    mobile: Optional[str]
    address_line1: Optional[str]
    address_line2: Optional[str]
    city: Optional[str]
    state: Optional[str]
    postal_code: Optional[str]
    country: str
    date_of_birth: Optional[str]
    bio: Optional[str]
    website: Optional[str]
    twitter: Optional[str]
    linkedin: Optional[str]
    github: Optional[str]
    company: Optional[str]
    job_title: Optional[str]
    department: Optional[str]
    manager_id: Optional[UUID]
    hire_date: Optional[str]
    salary: Optional[float]
    bonus: Optional[float]
    stock_options: Optional[int]
    vacation_days: int
    sick_days: int
    is_active: bool
    is_verified: bool
    is_admin: bool
    created_at: str
    updated_at: str
    last_login: Optional[str]
    login_count: int
    failed_login_count: int
    password_reset_token: Optional[str]
    password_reset_expires: Optional[str]

    # Total: 42 fields


@pytest.mark.asyncio
@pytest.mark.database
class TestFieldLimitRepositoryIntegration:
    """Test field limit threshold at repository level."""

    @pytest.fixture
    async def setup_test_data(self, db_pool):
        """Create test table and data."""
        async with db_pool.connection() as conn:
            cursor = conn.cursor()

            # Create table
            await cursor.execute("""
                CREATE TABLE IF NOT EXISTS users_many_fields (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL
                )
            """)

            # Create view
            await cursor.execute("""
                CREATE OR REPLACE VIEW user_many_fields_view AS
                SELECT id, data FROM users_many_fields
            """)

            # Insert test data
            test_user_id = uuid4()
            test_data = {
                "id": str(test_user_id),
                "username": "testuser",
                "email": "test@example.com",
                "first_name": "Test",
                "last_name": "User",
                "middle_name": "Middle",
                "display_name": "Test User",
                "phone": "555-1234",
                "mobile": "555-5678",
                "address_line1": "123 Main St",
                "address_line2": "Apt 4B",
                "city": "Testville",
                "state": "TS",
                "postal_code": "12345",
                "country": "Testland",
                "date_of_birth": "1990-01-01",
                "bio": "Test bio",
                "website": "https://test.com",
                "twitter": "@testuser",
                "linkedin": "testuser",
                "github": "testuser",
                "company": "Test Corp",
                "job_title": "Tester",
                "department": "QA",
                "manager_id": str(uuid4()),
                "hire_date": "2020-01-01",
                "salary": 100000.0,
                "bonus": 10000.0,
                "stock_options": 1000,
                "vacation_days": 20,
                "sick_days": 10,
                "is_active": True,
                "is_verified": True,
                "is_admin": False,
                "created_at": "2020-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z",
                "last_login": "2024-01-01T12:00:00Z",
                "login_count": 100,
                "failed_login_count": 2,
                "password_reset_token": None,
                "password_reset_expires": None,
            }

            await cursor.execute(
                """INSERT INTO users_many_fields (id, data) VALUES (%s, %s::jsonb)""",
                (test_user_id, json.dumps(test_data)),
            )

            await conn.commit()

            # Register type for view
            register_type_for_view("user_many_fields_view", UserWithManyFields)

            yield test_user_id, test_data

            # Cleanup after test
            await cursor.execute("DELETE FROM users_many_fields WHERE id = %s", (test_user_id,))
            await conn.commit()

    async def test_repository_with_field_limit_threshold(self, db_pool, setup_test_data):
        """Test repository behavior with field limit threshold."""
        test_user_id, test_data = setup_test_data

        # Create repository with field limit threshold
        context = {
            "mode": "development",
            "jsonb_field_limit_threshold": 20,  # Set threshold to 20
        }
        repo = FraiseQLRepository(pool=db_pool, context=context)

        # Query should work even with 42 fields
        users = await repo.find("user_many_fields_view", id=test_user_id)

        assert len(users) == 1
        user = users[0]

        # Verify all fields are present
        assert user.id == test_user_id
        assert user.username == test_data["username"]
        assert user.email == test_data["email"]
        assert user.is_active == test_data["is_active"]
        assert user.login_count == test_data["login_count"]

    async def test_repository_without_threshold(self, db_pool, setup_test_data):
        """Test repository behavior without field limit threshold."""
        test_user_id, test_data = setup_test_data

        # Create repository without threshold
        context = {"mode": "development"}
        repo = FraiseQLRepository(pool=db_pool, context=context)

        # Query should still work (will use jsonb_build_object with many params)
        users = await repo.find("user_many_fields_view", id=test_user_id)

        assert len(users) == 1
        user = users[0]
        assert user.id == test_user_id

    async def test_repository_find_one_with_threshold(self, db_pool, setup_test_data):
        """Test find_one with field limit threshold."""
        test_user_id, test_data = setup_test_data

        context = {"mode": "development", "jsonb_field_limit_threshold": 20}
        repo = FraiseQLRepository(pool=db_pool, context=context)

        # Find by ID
        user = await repo.find_one("user_many_fields_view", id=test_user_id)

        assert user is not None
        assert user.id == test_user_id
        assert user.username == test_data["username"]

    async def test_repository_with_where_conditions(self, db_pool, setup_test_data):
        """Test repository with WHERE conditions and field limit."""
        test_user_id, test_data = setup_test_data

        context = {"mode": "development", "jsonb_field_limit_threshold": 20}
        repo = FraiseQLRepository(pool=db_pool, context=context)

        # Query with WHERE condition on id (which exists as a column)
        users = await repo.find("user_many_fields_view", id=test_user_id)

        assert len(users) == 1
        assert users[0].username == "testuser"

    async def test_threshold_edge_cases(self, db_pool, setup_test_data):
        """Test edge cases for field limit threshold."""
        test_user_id, _ = setup_test_data

        # Test with very low threshold
        context = {
            "mode": "development",
            "jsonb_field_limit_threshold": 1,  # Everything should use full data
        }
        repo = FraiseQLRepository(pool=db_pool, context=context)

        users = await repo.find("user_many_fields_view", id=test_user_id)
        assert len(users) == 1
        assert users[0].id == test_user_id

        # Test with very high threshold
        context["jsonb_field_limit_threshold"] = 100  # Should use jsonb_build_object
        repo = FraiseQLRepository(pool=db_pool, context=context)

        users = await repo.find("user_many_fields_view", id=test_user_id)
        assert len(users) == 1
        assert users[0].id == test_user_id


@fraiseql.type
@dataclass
class SimpleUser:
    """Simple user type with few fields."""

    id: UUID
    name: str
    email: str


@pytest.mark.asyncio
@pytest.mark.database
class TestFieldLimitWithSimpleTypes:
    """Test that simple types still work normally."""

    @pytest.fixture
    async def setup_simple_data(self, db_pool):
        """Create simple test data."""
        async with db_pool.connection() as conn:
            cursor = conn.cursor()

            await cursor.execute("""
                CREATE TABLE IF NOT EXISTS simple_users (
                    id UUID PRIMARY KEY,
                    data JSONB NOT NULL
                )
            """)

            await cursor.execute("""
                CREATE OR REPLACE VIEW simple_user_view AS
                SELECT id, data FROM simple_users
            """)

            test_id = uuid4()
            await cursor.execute(
                """INSERT INTO simple_users (id, data) VALUES (%s, %s::jsonb)""",
                (
                    test_id,
                    json.dumps({"id": str(test_id), "name": "Simple", "email": "simple@test.com"}),
                ),
            )

            await conn.commit()
            register_type_for_view("simple_user_view", SimpleUser)

            yield test_id

            # Cleanup after test
            await cursor.execute("DELETE FROM simple_users WHERE id = %s", (test_id,))
            await conn.commit()

    async def test_simple_type_below_threshold(self, db_pool, setup_simple_data):
        """Test that simple types work normally below threshold."""
        test_id = setup_simple_data

        context = {
            "mode": "development",
            "jsonb_field_limit_threshold": 20,  # Simple type has only 3 fields
        }
        repo = FraiseQLRepository(pool=db_pool, context=context)

        users = await repo.find("simple_user_view", id=test_id)

        assert len(users) == 1
        assert users[0].id == test_id
        assert users[0].name == "Simple"
        assert users[0].email == "simple@test.com"
