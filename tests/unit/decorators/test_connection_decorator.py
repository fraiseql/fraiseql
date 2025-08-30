"""Tests for @connection decorator for cursor-based pagination.

🚀 TDD Implementation - RED phase first!

This tests the @connection decorator that should:
1. Convert standard query resolvers to return Connection[T] types
2. Automatically handle cursor-based pagination parameters
3. Delegate to repository.paginate() for actual pagination logic
4. Support all Relay connection specification features
"""

import pytest
from typing import Any

from fraiseql.types import fraise_type
from fraiseql.types.generic import Connection


@fraise_type
class User:
    """Test user type for connection testing."""
    id: str
    name: str
    email: str
    created_at: str


@pytest.mark.unit
class TestConnectionDecorator:
    """Test the @connection decorator functionality - TDD RED phase."""

    def test_connection_decorator_can_be_imported(self):
        """Test that connection decorator can be imported - should PASS in GREEN phase."""
        # This should pass in GREEN phase since decorator now exists
        from fraiseql.decorators import connection
        assert connection is not None

    def test_connection_decorator_basic_usage(self):
        """Test basic @connection decorator usage."""
        from fraiseql.decorators import connection

        @connection(node_type=User)
        async def users_connection(info, first: int | None = None) -> Connection[User]:
            pass

        # Test that decorator properly wraps the function
        assert hasattr(users_connection, '__fraiseql_connection__')
        config = users_connection.__fraiseql_connection__
        assert config['node_type'] == User
        assert config['view_name'] == "v_users"  # Inferred from function name
        assert config['default_page_size'] == 20
        assert config['max_page_size'] == 100
        assert config['include_total_count'] is True
        assert config['cursor_field'] == "id"

    def test_connection_decorator_with_options(self):
        """Test @connection decorator with custom configuration."""
        from fraiseql.decorators import connection

        @connection(
            node_type=User,
            view_name="v_custom_users",
            default_page_size=25,
            max_page_size=50,
            include_total_count=False,
            cursor_field="created_at"
        )
        async def custom_users_connection(info, first: int | None = None) -> Connection[User]:
            pass

        config = custom_users_connection.__fraiseql_connection__
        assert config['node_type'] == User
        assert config['view_name'] == "v_custom_users"
        assert config['default_page_size'] == 25
        assert config['max_page_size'] == 50
        assert config['include_total_count'] is False
        assert config['cursor_field'] == "created_at"

    def test_connection_decorator_parameter_validation(self):
        """Test that @connection decorator validates parameters."""
        from fraiseql.decorators import connection

        # Should raise error for missing node_type
        with pytest.raises(ValueError, match="node_type is required"):
            @connection(node_type=None)  # type: ignore
            async def invalid_connection(info) -> Connection[User]:
                pass

        # Should raise error for invalid default_page_size
        with pytest.raises(ValueError, match="default_page_size must be positive"):
            @connection(node_type=User, default_page_size=0)
            async def invalid_page_size_connection(info) -> Connection[User]:
                pass

        # Should raise error for invalid max_page_size
        with pytest.raises(ValueError, match="max_page_size must be positive"):
            @connection(node_type=User, max_page_size=-1)
            async def invalid_max_page_size_connection(info) -> Connection[User]:
                pass

        # Should raise error if max_page_size < default_page_size
        with pytest.raises(ValueError, match="max_page_size must be >= default_page_size"):
            @connection(node_type=User, default_page_size=50, max_page_size=25)
            async def inconsistent_page_sizes_connection(info) -> Connection[User]:
                pass
