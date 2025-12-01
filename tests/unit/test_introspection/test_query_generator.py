"""Unit tests for QueryGenerator."""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from fraiseql.introspection.metadata_parser import TypeAnnotation
from fraiseql.introspection.query_generator import QueryGenerator


class TestQueryGenerator:
    """Test QueryGenerator functionality."""

    @pytest.fixture
    def query_generator(self) -> None:
        """Create a QueryGenerator instance."""
        return QueryGenerator()

    @pytest.fixture
    def mock_type_class(self) -> None:
        """Create a mock type class."""
        mock_class = MagicMock()
        mock_class.__name__ = "User"
        return mock_class

    def test_generate_queries_for_type_basic(self, query_generator, mock_type_class) -> None:
        """Test basic query generation."""
        annotation = TypeAnnotation()

        with patch("fraiseql.query") as mock_query_decorator:
            mock_query_decorator.return_value = "decorated_query"

            queries = query_generator.generate_queries_for_type(
                mock_type_class, "v_user", "public", annotation
            )

            # Should generate 2 queries (find_one and find_all)
            assert len(queries) == 2
            assert all(q == "decorated_query" for q in queries)

            # Should call @query decorator twice
            assert mock_query_decorator.call_count == 2

    def test_generate_queries_for_type_with_connection(
        self, query_generator, mock_type_class
    ) -> None:
        """Test query generation with connection (filter_config)."""
        annotation = TypeAnnotation(filter_config={"some": "config"})

        with patch("fraiseql.query") as mock_query_decorator:
            mock_query_decorator.return_value = "decorated_query"

            queries = query_generator.generate_queries_for_type(
                mock_type_class, "v_user", "public", annotation
            )

            # Should generate 3 queries (find_one, find_all, connection)
            assert len(queries) == 3
            assert all(q == "decorated_query" for q in queries)

            # Should call @query decorator three times
            assert mock_query_decorator.call_count == 3

    def test_generate_find_one_query(self, query_generator, mock_type_class) -> None:
        """Test find_one query generation."""
        with patch("fraiseql.query") as mock_query_decorator:
            mock_query_decorator.return_value = "decorated_find_one"

            query = query_generator._generate_find_one_query(mock_type_class, "v_user", "public")

            assert query == "decorated_find_one"

            # Check that the decorator was called
            mock_query_decorator.assert_called_once()

            # Check function naming
            func = mock_query_decorator.call_args[0][0]
            assert func.__name__ == "user"
            assert func.__qualname__ == "user"

    def test_generate_find_all_query(self, query_generator, mock_type_class) -> None:
        """Test find_all query generation."""
        with patch("fraiseql.query") as mock_query_decorator:
            mock_query_decorator.return_value = "decorated_find_all"

            query = query_generator._generate_find_all_query(mock_type_class, "v_user", "public")

            assert query == "decorated_find_all"

            # Check that the decorator was called
            mock_query_decorator.assert_called_once()

            # Check function naming
            func = mock_query_decorator.call_args[0][0]
            assert func.__name__ == "users"
            assert func.__qualname__ == "users"

    def test_generate_connection_query(self, query_generator, mock_type_class) -> None:
        """Test connection query generation."""
        with patch("fraiseql.query") as mock_query_decorator:
            mock_query_decorator.return_value = "decorated_connection"

            query = query_generator._generate_connection_query(mock_type_class, "v_user", "public")

            assert query == "decorated_connection"

            # Check that the decorator was called
            mock_query_decorator.assert_called_once()

            # Check function naming
            func = mock_query_decorator.call_args[0][0]
            assert func.__name__ == "userConnection"
            assert func.__qualname__ == "userConnection"

    @pytest.mark.asyncio
    async def test_find_one_query_execution(self, query_generator, mock_type_class) -> None:
        """Test that generated find_one query can be executed."""

        # Create a real query function to test execution
        async def find_one_impl(info, id) -> None:
            db = info.context["db"]
            sql_source = "public.v_user"
            return await db.find_one(sql_source, where={"id": id})

        # Mock the database call
        mock_db = AsyncMock()
        mock_db.find_one.return_value = {"id": "123", "name": "Test"}

        mock_info = MagicMock()
        mock_info.context = {"db": mock_db}

        # Test execution
        result = await find_one_impl(mock_info, "123")

        assert result == {"id": "123", "name": "Test"}
        mock_db.find_one.assert_called_once_with("public.v_user", where={"id": "123"})

    @pytest.mark.asyncio
    async def test_find_all_query_execution(self, query_generator, mock_type_class) -> None:
        """Test that generated find_all query can be executed."""

        # Create a real query function to test execution
        async def find_all_impl(info, where=None, order_by=None, limit=None, offset=None) -> None:
            db = info.context["db"]
            sql_source = "public.v_user"
            return await db.find(
                sql_source, where=where, order_by=order_by, limit=limit, offset=offset
            )

        # Mock the database call
        mock_db = AsyncMock()
        mock_db.find.return_value = [{"id": "123", "name": "Test"}]

        mock_info = MagicMock()
        mock_info.context = {"db": mock_db}

        # Test execution
        result = await find_all_impl(mock_info, where={"active": True}, limit=10)

        assert result == [{"id": "123", "name": "Test"}]
        mock_db.find.assert_called_once_with(
            "public.v_user", where={"active": True}, order_by=None, limit=10, offset=None
        )
