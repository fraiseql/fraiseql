"""Tests for @connection decorator for cursor-based pagination.

🚀 TDD Implementation - RED phase first!

This tests the @connection decorator that should:
1. Convert standard query resolvers to return Connection[T] types
2. Automatically handle cursor-based pagination parameters
3. Delegate to repository.paginate() for actual pagination logic
4. Support all Relay connection specification features
"""

import pytest

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

    def test_connection_decorator_can_be_imported(self) -> None:
        """Test that connection decorator can be imported - should PASS in GREEN phase."""
        # This should pass in GREEN phase since decorator now exists
        from fraiseql.decorators import connection

        assert connection is not None

    def test_connection_decorator_basic_usage(self) -> None:
        """Test basic @connection decorator usage."""
        from fraiseql.decorators import connection

        @connection(node_type=User)
        async def users_connection(info, first: int | None = None) -> Connection[User]:
            pass

        # Test that decorator properly wraps the function
        assert hasattr(users_connection, "__fraiseql_connection__")
        config = users_connection.__fraiseql_connection__
        assert config["node_type"] == User
        assert config["view_name"] == "v_users"  # Inferred from function name
        assert config["default_page_size"] == 20
        assert config["max_page_size"] == 100
        assert config["include_total_count"] is True
        assert config["cursor_field"] == "id"

    def test_connection_decorator_with_options(self) -> None:
        """Test @connection decorator with custom configuration."""
        from fraiseql.decorators import connection

        @connection(
            node_type=User,
            view_name="v_custom_users",
            default_page_size=25,
            max_page_size=50,
            include_total_count=False,
            cursor_field="created_at",
        )
        async def custom_users_connection(info, first: int | None = None) -> Connection[User]:
            pass

        config = custom_users_connection.__fraiseql_connection__
        assert config["node_type"] == User
        assert config["view_name"] == "v_custom_users"
        assert config["default_page_size"] == 25
        assert config["max_page_size"] == 50
        assert config["include_total_count"] is False
        assert config["cursor_field"] == "created_at"

    def test_connection_decorator_parameter_validation(self) -> None:
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

    def test_connection_decorator_jsonb_extraction_compatibility(self) -> None:
        """🔴 RED: Test that @connection decorator respects global JSONB configuration.

        This test documents enterprise JSONB scenarios where:
        1. Global JSONB config works for individual queries
        2. @connection decorator needs to inherit JSONB field extraction
        3. Connection wrapper type needs to extract JSONB fields

        This should FAIL in RED phase, then PASS after GREEN implementation.
        """
        from unittest.mock import AsyncMock, Mock

        from fraiseql.decorators import connection

        # Mock database repository with JSONB data structure
        mock_db = AsyncMock()
        mock_db.paginate.return_value = {
            "nodes": [
                {
                    "id": "dns-server-1",
                    "data": {  # JSONB column with extracted fields
                        "identifier": "dns-001",
                        "ip_address": "192.168.1.10",
                        "n_total_allocations": 5,
                    },
                },
                {
                    "id": "dns-server-2",
                    "data": {
                        "identifier": "dns-002",
                        "ip_address": "192.168.1.20",
                        "n_total_allocations": 3,
                    },
                },
            ],
            "pageInfo": {
                "hasNextPage": False,
                "hasPreviousPage": False,
                "startCursor": "dns-server-1",
                "endCursor": "dns-server-2",
            },
            "totalCount": 2,
        }

        # Mock GraphQL info with database context
        mock_info = Mock()
        mock_info.context = {"db": mock_db}

        # Create connection with JSONB extraction enabled
        @connection(
            node_type=User,
            view_name="v_dns_server",
            jsonb_extraction=True,  # 🔴 This parameter should exist but doesn't yet
            jsonb_column="data",  # 🔴 This parameter should exist but doesn't yet
        )
        async def dns_servers_connection(info, first: int | None = None) -> Connection[User]:
            pass

        # Test that JSONB configuration is stored in connection metadata
        config = dns_servers_connection.__fraiseql_connection__
        assert config["jsonb_extraction"] is True
        assert config["jsonb_column"] == "data"
        assert config["supports_global_jsonb"] is True  # ✅ Now supported!

    def test_connection_decorator_global_jsonb_inheritance(self) -> None:
        """🔄 REFACTOR: Test that @connection inherits global JSONB configuration."""
        from unittest.mock import AsyncMock, Mock

        from fraiseql.decorators import connection

        # Mock config with global JSONB settings
        mock_config = Mock()
        mock_config.jsonb_extraction_enabled = True
        mock_config.jsonb_default_columns = ["metadata", "data"]  # Test priority

        mock_db = AsyncMock()
        mock_info = Mock()
        mock_info.context = {"db": mock_db, "config": mock_config}

        # Connection WITHOUT explicit JSONB parameters - should inherit
        @connection(node_type=User, view_name="v_test")
        async def auto_jsonb_connection(info, first: int | None = None) -> Connection[User]:
            pass

        # Test metadata shows None (will be resolved at runtime)
        config = auto_jsonb_connection.__fraiseql_connection__
        assert config["jsonb_extraction"] is None  # Will inherit at runtime
        assert config["jsonb_column"] is None  # Will inherit at runtime
        assert config["supports_global_jsonb"] is True

        # Test runtime resolution by calling the function
        # This would verify the global config inheritance works
        # (We can't easily test the actual paginate call without integration tests)

    def test_connection_decorator_explicit_overrides_global(self) -> None:
        """🔄 REFACTOR: Test that explicit params override global config."""
        from fraiseql.decorators import connection

        # Connection WITH explicit JSONB parameters - should override global
        @connection(
            node_type=User,
            view_name="v_test",
            jsonb_extraction=False,  # Explicit override
            jsonb_column="custom_data",  # Explicit override
        )
        async def explicit_jsonb_connection(info, first: int | None = None) -> Connection[User]:
            pass

        config = explicit_jsonb_connection.__fraiseql_connection__
        assert config["jsonb_extraction"] is False  # Explicit override
        assert config["jsonb_column"] == "custom_data"  # Explicit override
        assert config["supports_global_jsonb"] is True

    def test_connection_decorator_backward_compatibility(self) -> None:
        """🔄 REFACTOR: Test backward compatibility - old code still works."""
        from fraiseql.decorators import connection

        # Old-style usage without JSONB parameters - should work unchanged
        @connection(node_type=User, view_name="v_legacy")
        async def legacy_connection(info, first: int | None = None) -> Connection[User]:
            pass

        # Should have all expected metadata with sensible defaults
        config = legacy_connection.__fraiseql_connection__
        assert config["node_type"] == User
        assert config["view_name"] == "v_legacy"
        assert config["default_page_size"] == 20
        assert config["max_page_size"] == 100
        assert config["include_total_count"] is True
        assert config["cursor_field"] == "id"
        assert config["jsonb_extraction"] is None  # Will inherit global
        assert config["jsonb_column"] is None  # Will inherit global
        assert config["supports_global_jsonb"] is True
