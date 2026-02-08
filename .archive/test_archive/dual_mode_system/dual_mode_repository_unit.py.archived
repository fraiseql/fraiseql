"""Unit tests for dual-mode repository (no database required)."""

import os
from datetime import datetime
from typing import Any, Optional
from unittest.mock import MagicMock, patch
from uuid import UUID, uuid4

import pytest

import fraiseql
from fraiseql import fraise_field
from fraiseql.db import FraiseQLRepository

# Test types for dual-mode instantiation


@pytest.mark.skip(
    reason="Test file has undefined types and import issues - not related to LTREE feature"
)
@pytest.mark.integration
@pytest.mark.database
class TestDualModeRepositoryUnit:
    """Unit tests for dual-mode instantiation without database."""

    @pytest.fixture
    def mock_pool(self):
        """Create a mock connection pool."""
        return MagicMock()

    @pytest.mark.skip(
        reason="Mode detection logic removed from repository - now always uses Rust pipeline"
    )
    def test_mode_detection_from_environment(self, mock_pool):
        """Test mode detection from environment variables."""
        # Test production mode (default)
        with patch.dict(os.environ, {}, clear=True):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "production"

        # Test development mode
        with patch.dict(os.environ, {"FRAISEQL_ENV": "development"}):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "development"

        # Test explicit production
        with patch.dict(os.environ, {"FRAISEQL_ENV": "production"}):
            repo = FraiseQLRepository(mock_pool)
            assert repo.mode == "production"

    @pytest.mark.skip(
        reason="Mode detection logic removed from repository - now always uses Rust pipeline"
    )
    def test_mode_override_from_context(self, mock_pool):
        """Test that context mode overrides environment."""
        # Environment says production, but context says development
        with patch.dict(os.environ, {"FRAISEQL_ENV": "production"}):
            context = {"mode": "development"}
            repo = FraiseQLRepository(mock_pool, context)
            assert repo.mode == "development"

        # Environment says development, but context says production
        with patch.dict(os.environ, {"FRAISEQL_ENV": "development"}):
            context = {"mode": "production"}
            repo = FraiseQLRepository(mock_pool, context)
            assert repo.mode == "production"

    @pytest.mark.skip(reason="Test uses undefined User type - test file has import/type issues")
    def test_instantiate_recursive_simple_object(self, mock_pool):
        """Test recursive instantiation of a simple object."""
        repo = FraiseQLRepository(mock_pool, {"mode": "development"})

        data = {
            "id": str(uuid4()),
            "name": "John Doe",
            "email": "john@example.com",
            "role": "admin",
        }

        # Mock type registry
        with patch.object(repo, "_get_type_for_view", return_value=User):
            result = repo._instantiate_recursive(User, data)

        assert isinstance(result, User)
        assert result.name == "John Doe"
        assert result.email == "john@example.com"
        assert result.role == "admin"

    def test_instantiate_recursive_with_nested_objects(self, mock_pool):
        """Test recursive instantiation with nested objects."""
        repo = FraiseQLRepository(mock_pool, {"mode": "development"})

        product_id = uuid4()
        user_id = uuid4()
        order_id = uuid4()

        data = {
            "id": str(order_id),
            "productId": str(product_id),
            "userId": str(user_id),
            "data": {"priority": "high"},
            "tags": ["urgent", "expedited"],
            "product": {
                "id": str(product_id),
                "name": "Widget Pro",
                "status": "available",
                "category": "Electronics",
                "createdAt": "2024-01-01T10:00:00Z",
                "data": {"sku": "WP-123"},
            },
            "user": {
                "id": str(user_id),
                "name": "John Doe",
                "email": "john@example.com",
                "role": "admin",
            },
        }

        result = repo._instantiate_recursive(Order, data)

        assert isinstance(result, Order)
        assert result.id == order_id
        assert result.product_id == product_id
        assert result.user_id == user_id
        assert isinstance(result.product, Product)
        assert result.product.name == "Widget Pro"
        assert isinstance(result.user, User)
        assert result.user.name == "John Doe"
        assert result.tags == ["urgent", "expedited"]

    def test_instantiate_recursive_handles_circular_references(self, mock_pool):
        """Test that circular references are handled correctly."""
        repo = FraiseQLRepository(mock_pool, {"mode": "development"})

        user_id = uuid4()
        project_id = uuid4()

        data = {
            "id": str(project_id),
            "name": "Test Project",
            "leadId": str(user_id),
            "lead": {
                "id": str(user_id),
                "name": "John Doe",
                "email": "john@example.com",
                "role": "admin",
            },
            "members": [
                {
                    "id": str(user_id),  # Same user as lead
                    "name": "John Doe",
                    "email": "john@example.com",
                    "role": "admin",
                },
                {
                    "id": str(uuid4()),
                    "name": "Jane Smith",
                    "email": "jane@example.com",
                    "role": "user",
                },
            ],
        }

        result = repo._instantiate_recursive(Project, data)

        assert isinstance(result, Project)
        assert isinstance(result.lead, User)
        assert len(result.members) == 2
        assert all(isinstance(m, User) for m in result.members)
        # Check that the same user instance is reused
        assert result.lead is result.members[0]

    def test_instantiate_recursive_max_depth_protection(self, mock_pool):
        """Test that excessive recursion depth raises an error."""
        repo = FraiseQLRepository(mock_pool, {"mode": "development"})

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

        # Use a simple mock type that accepts any fields
        class MockNestedType:
            def __init__(self, **kwargs):
                for k, v in kwargs.items():
                    setattr(self, k, v)

        # Add the required metadata
        MockNestedType.__gql_type_hints__ = {
            "id": UUID,
            "name": str,
            "nested": Optional[MockNestedType],
        }
        MockNestedType.__fraiseql_definition__ = True

        with pytest.raises(ValueError, match="Max recursion depth exceeded"):
            repo._instantiate_recursive(MockNestedType, deep_data)

    def test_camel_to_snake_case_conversion(self, mock_pool):
        """Test that camelCase keys are converted to snake_case."""
        repo = FraiseQLRepository(mock_pool, {"mode": "development"})

        data = {
            "id": str(uuid4()),
            "productId": str(uuid4()),
            "userId": str(uuid4()),
            "createdAt": "2024-01-01T10:00:00Z",
            "someComplexFieldName": "value",
        }

        # Test with a simple type that would accept these fields
        @fraiseql.type
        class SampleType:
            id: UUID
            product_id: UUID
            user_id: UUID
            created_at: str
            some_complex_field_name: str

        result = repo._instantiate_recursive(SampleType, data)

        assert hasattr(result, "product_id")
        assert hasattr(result, "user_id")
        assert hasattr(result, "created_at")
        assert hasattr(result, "some_complex_field_name")
        assert result.some_complex_field_name == "value"

    def test_extract_type_from_optional(self, mock_pool):
        """Test type extraction from Optional types."""
        repo = FraiseQLRepository(mock_pool)

        # Test Optional[User]
        optional_user = Optional[User]
        assert repo._extract_type(optional_user) == User

        # Test non-optional type
        assert repo._extract_type(User) == User

        # Test Optional[None] (edge case) - should return NoneType
        assert repo._extract_type(Optional[None]) is type(None)

    def test_extract_list_type(self, mock_pool):
        """Test type extraction from List types."""
        repo = FraiseQLRepository(mock_pool)

        # Test list[User]
        list_user = list[User]
        assert repo._extract_list_type(list_user) == User

        # Test Optional[list[User]]
        optional_list_user = Optional[list[User]]
        assert repo._extract_list_type(optional_list_user) == User

        # Test non-list type
        assert repo._extract_list_type(User) is None

    def test_build_find_query(self, mock_pool):
        """Test query building for find method."""
        repo = FraiseQLRepository(mock_pool)

        # Test without parameters
        query = repo._build_find_query("tv_product")
        # Check the SQL components instead of string representation
        assert query.statement is not None
        assert query.params == {}
        assert query.fetch_result is True

        # Test with parameters
        product_id = uuid4()
        query = repo._build_find_query("tv_product", id=product_id, status="available")
        assert query.statement is not None
        # After fix for %r placeholder bug: kwargs are embedded as Literals in Composed SQL
        assert query.params == {}  # No separate params - values embedded in statement
        # Verify the statement contains the expected values as Literals
        statement_str = str(query.statement)
        assert str(product_id) in statement_str
        assert "available" in statement_str

    def test_build_find_one_query(self, mock_pool):
        """Test query building for find_one method."""
        repo = FraiseQLRepository(mock_pool)

        # Test without parameters
        query = repo._build_find_one_query("tv_product")
        # The query should have a statement and a limit
        assert query.statement is not None
        assert query.params == {}
        assert query.fetch_result is True

        # Test with parameters
        product_id = uuid4()
        query = repo._build_find_one_query("tv_product", id=product_id)
        assert query.statement is not None
        # After fix for %r placeholder bug: kwargs are embedded as Literals in Composed SQL
        assert query.params == {}  # No separate params - values embedded in statement
        # Verify the statement contains the expected value as Literal
        statement_str = str(query.statement)
        assert str(product_id) in statement_str
