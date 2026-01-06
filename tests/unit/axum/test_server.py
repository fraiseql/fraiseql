"""Unit tests for AxumServer."""

from unittest.mock import MagicMock

import pytest

from fraiseql import fraise_type
from fraiseql.axum.config import AxumFraiseQLConfig
from fraiseql.axum.server import AxumServer


class TestAxumServerInitialization:
    """Test AxumServer initialization."""

    def test_server_initialization(self) -> None:
        """Test creating an AxumServer instance."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        server = AxumServer(config=config)

        assert server._config == config
        assert server._py_server is None
        assert server._is_running is False
        assert len(server._types) == 0
        assert len(server._mutations) == 0
        assert len(server._queries) == 0
        assert len(server._subscriptions) == 0

    def test_server_initialization_requires_config(self) -> None:
        """Test that server requires valid config."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        server = AxumServer(config=config)
        assert server.get_config() == config


class TestAxumServerTypeRegistration:
    """Test type registration."""

    def test_register_types(self) -> None:
        """Test registering GraphQL types."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        @fraise_type
        class User:
            id: str
            name: str

        @fraise_type
        class Post:
            id: str
            title: str

        server.register_types([User, Post])

        assert len(server.registered_types()) == 2
        assert "User" in server.registered_types()
        assert "Post" in server.registered_types()

    def test_register_mutations(self) -> None:
        """Test registering mutations."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        class CreateUser:
            pass

        class UpdateUser:
            pass

        server.register_mutations([CreateUser, UpdateUser])

        assert len(server.registered_mutations()) == 2
        assert "CreateUser" in server.registered_mutations()
        assert "UpdateUser" in server.registered_mutations()

    def test_register_queries(self) -> None:
        """Test registering queries."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        class GetUsers:
            pass

        class GetUser:
            pass

        server.register_queries([GetUsers, GetUser])

        assert len(server.registered_queries()) == 2
        assert "GetUsers" in server.registered_queries()
        assert "GetUser" in server.registered_queries()

    def test_register_subscriptions(self) -> None:
        """Test registering subscriptions."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        class OnUserCreated:
            pass

        class OnUserUpdated:
            pass

        server.register_subscriptions([OnUserCreated, OnUserUpdated])

        assert len(server.registered_subscriptions()) == 2
        assert "OnUserCreated" in server.registered_subscriptions()
        assert "OnUserUpdated" in server.registered_subscriptions()

    def test_register_multiple_times(self) -> None:
        """Test registering types multiple times."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        class Type1:
            pass

        class Type2:
            pass

        server.register_types([Type1])
        assert len(server.registered_types()) == 1

        server.register_types([Type2])
        assert len(server.registered_types()) == 2

    def test_add_middleware_placeholder(self) -> None:
        """Test that add_middleware is a placeholder."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        # Should not raise error (placeholder)
        server.add_middleware(None)  # type: ignore[misc]


class TestAxumServerState:
    """Test server state management."""

    def test_is_running_initial_state(self) -> None:
        """Test that server is not running initially."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        assert server.is_running() is False

    def test_is_running_after_start(self) -> None:
        """Test is_running after start (mocked)."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        # Mock PyAxumServer
        mock_py_server = MagicMock()
        mock_py_server.is_running.return_value = True

        server._py_server = mock_py_server
        server._is_running = True

        assert server.is_running() is True


class TestAxumServerConfiguration:
    """Test configuration access."""

    def test_get_config(self) -> None:
        """Test getting server configuration."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_host="0.0.0.0",  # noqa: S104
            axum_port=3000
        )
        server = AxumServer(config=config)

        retrieved_config = server.get_config()

        assert retrieved_config == config
        assert retrieved_config.axum_host == "0.0.0.0"  # noqa: S104
        assert retrieved_config.axum_port == 3000

    def test_get_schema_without_server(self) -> None:
        """Test that get_schema fails without py_server."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        # Should fail because _py_server is None
        with pytest.raises(RuntimeError):
            server.execute_query("{ __schema { types { name } } }")


class TestAxumServerStringRepresentation:
    """Test string representations."""

    def test_repr(self) -> None:
        """Test __repr__ method."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_host="0.0.0.0",  # noqa: S104
            axum_port=3000
        )
        server = AxumServer(config=config)

        repr_str = repr(server)

        assert "AxumServer" in repr_str
        assert "0.0.0.0" in repr_str  # noqa: S104
        assert "3000" in repr_str

    def test_str(self) -> None:
        """Test __str__ method."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_host="0.0.0.0",  # noqa: S104
            axum_port=3000
        )
        server = AxumServer(config=config)

        str_repr = str(server)

        assert "FraiseQL Axum Server" in str_repr
        assert "stopped" in str_repr


class TestAxumServerErrorHandling:
    """Test error handling."""

    def test_execute_query_without_server(self) -> None:
        """Test execute_query fails without py_server."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        with pytest.raises(RuntimeError) as exc_info:
            server.execute_query("{ __schema { types { name } } }")

        assert "Server not initialized" in str(exc_info.value)

    def test_shutdown_when_not_running(self) -> None:
        """Test shutdown when server not running."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        # Should not raise error
        import asyncio
        asyncio.run(server.shutdown())

    def test_start_when_already_running(self) -> None:
        """Test that start raises error when already running."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        # Mark as running
        server._is_running = True

        with pytest.raises(RuntimeError) as exc_info:
            server.start()

        assert "already running" in str(exc_info.value)

    def test_start_async_when_already_running(self) -> None:
        """Test that start_async raises error when already running."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )
        server = AxumServer(config=config)

        # Mark as running
        server._is_running = True

        import asyncio
        with pytest.raises(RuntimeError) as exc_info:
            asyncio.run(server.start_async())

        assert "already running" in str(exc_info.value)
