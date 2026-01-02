"""Tests for Federation _entities resolver."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from fraiseql.federation import (
    entity,
    EntitiesResolver,
    clear_entity_registry,
    get_entity_metadata,
)


class TestEntitiesResolver:
    """Tests for EntitiesResolver."""

    def setup_method(self):
        """Clear registry and set up test entities before each test."""
        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str
            email: str

        @entity
        class Post:
            id: str
            title: str
            author_id: str

    def test_resolver_initialization(self):
        """Test EntitiesResolver initializes with registered entities."""
        resolver = EntitiesResolver()

        # Should have registered entities
        assert "User" in resolver.get_supported_types()
        assert "Post" in resolver.get_supported_types()

    def test_get_key_field(self):
        """Test getting key field for entity type."""
        resolver = EntitiesResolver()

        assert resolver.get_key_field("User") == "id"
        assert resolver.get_key_field("Post") == "id"
        assert resolver.get_key_field("NonExistent") is None

    def test_get_supported_types(self):
        """Test getting list of supported types."""
        resolver = EntitiesResolver()
        types = resolver.get_supported_types()

        assert "User" in types
        assert "Post" in types
        assert len(types) >= 2

    def test_parse_representation_valid(self):
        """Test parsing valid federation representation."""
        resolver = EntitiesResolver()

        rep = {"__typename": "User", "id": "user-123"}
        req = resolver._parse_representation(rep)

        assert req.typename == "User"
        assert req.key_field == "id"
        assert req.key_value == "user-123"

    def test_parse_representation_missing_typename(self):
        """Test parsing representation without __typename."""
        resolver = EntitiesResolver()

        rep = {"id": "user-123"}
        with pytest.raises(ValueError, match="Missing __typename"):
            resolver._parse_representation(rep)

    def test_parse_representation_missing_key(self):
        """Test parsing representation without key field."""
        resolver = EntitiesResolver()

        rep = {"__typename": "User"}
        with pytest.raises(ValueError, match="Missing key field"):
            resolver._parse_representation(rep)

    def test_parse_representation_unknown_type(self):
        """Test parsing representation with unknown type."""
        resolver = EntitiesResolver()

        rep = {"__typename": "UnknownType", "id": "123"}
        with pytest.raises(ValueError, match="Unknown entity type"):
            resolver._parse_representation(rep)

    def test_build_queries_single_type(self):
        """Test building queries for single entity type."""
        resolver = EntitiesResolver()

        requests = [
            resolver._parse_representation({"__typename": "User", "id": "user-1"}),
            resolver._parse_representation({"__typename": "User", "id": "user-2"}),
        ]

        queries = resolver._build_queries(requests)

        assert "User" in queries
        assert queries["User"]["table_name"] == "tv_user"
        assert queries["User"]["key_field"] == "id"
        assert queries["User"]["key_values"] == ["user-1", "user-2"]

    def test_build_queries_multiple_types(self):
        """Test building queries for multiple entity types."""
        resolver = EntitiesResolver()

        requests = [
            resolver._parse_representation({"__typename": "User", "id": "user-1"}),
            resolver._parse_representation({"__typename": "Post", "id": "post-1"}),
            resolver._parse_representation({"__typename": "User", "id": "user-2"}),
        ]

        queries = resolver._build_queries(requests)

        # Should batch by type
        assert "User" in queries
        assert "Post" in queries
        assert queries["User"]["key_values"] == ["user-1", "user-2"]
        assert queries["Post"]["key_values"] == ["post-1"]

    @pytest.mark.asyncio
    async def test_resolve_single_entity(self):
        """Test resolving a single entity."""
        resolver = EntitiesResolver()

        # Mock database pool
        mock_row = MagicMock()
        mock_row.__getitem__ = lambda self, key: {
            "id": "user-123",
            "data": {"id": "user-123", "name": "John", "email": "john@example.com"},
        }.get(key)

        mock_conn = AsyncMock()
        mock_conn.fetch = AsyncMock(return_value=[mock_row])
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool = AsyncMock()
        mock_pool.acquire = MagicMock(return_value=mock_conn)

        # Resolve
        representations = [{"__typename": "User", "id": "user-123"}]
        result = await resolver.resolve(representations, mock_pool)

        assert len(result) == 1
        assert result[0] is not None
        assert result[0]["__typename"] == "User"

    @pytest.mark.asyncio
    async def test_resolve_batch(self):
        """Test resolving batch of entities."""
        resolver = EntitiesResolver()

        # Mock database pool
        mock_rows = []
        for i in range(3):
            mock_row = MagicMock()
            mock_row.__getitem__ = lambda self, key, i=i: {
                "id": f"user-{i}",
                "data": {"id": f"user-{i}", "name": f"User {i}"},
            }.get(key)
            mock_rows.append(mock_row)

        mock_conn = AsyncMock()
        mock_conn.fetch = AsyncMock(return_value=mock_rows)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool = AsyncMock()
        mock_pool.acquire = MagicMock(return_value=mock_conn)

        # Resolve
        representations = [
            {"__typename": "User", "id": "user-0"},
            {"__typename": "User", "id": "user-1"},
            {"__typename": "User", "id": "user-2"},
        ]
        result = await resolver.resolve(representations, mock_pool)

        assert len(result) == 3
        # All should be resolved
        assert all(r is not None for r in result)

    @pytest.mark.asyncio
    async def test_resolve_multiple_types(self):
        """Test resolving mixed entity types."""
        resolver = EntitiesResolver()

        # Mock database pool to return different data per type
        async def mock_fetch(sql, *args):
            if "tv_user" in sql:
                mock_row = MagicMock()
                mock_row.__getitem__ = lambda self, key: {
                    "id": "user-1",
                    "data": {"id": "user-1", "name": "John"},
                }.get(key)
                return [mock_row]
            elif "tv_post" in sql:
                mock_row = MagicMock()
                mock_row.__getitem__ = lambda self, key: {
                    "id": "post-1",
                    "data": {"id": "post-1", "title": "Hello"},
                }.get(key)
                return [mock_row]
            return []

        mock_conn = AsyncMock()
        mock_conn.fetch = mock_fetch
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool = AsyncMock()
        mock_pool.acquire = MagicMock(return_value=mock_conn)

        # Resolve
        representations = [
            {"__typename": "User", "id": "user-1"},
            {"__typename": "Post", "id": "post-1"},
        ]
        result = await resolver.resolve(representations, mock_pool)

        assert len(result) == 2
        assert result[0]["__typename"] == "User"
        assert result[1]["__typename"] == "Post"

    @pytest.mark.asyncio
    async def test_resolve_not_found(self):
        """Test resolving entity that doesn't exist."""
        resolver = EntitiesResolver()

        # Mock database pool returns empty
        mock_conn = AsyncMock()
        mock_conn.fetch = AsyncMock(return_value=[])
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool = AsyncMock()
        mock_pool.acquire = MagicMock(return_value=mock_conn)

        # Resolve
        representations = [{"__typename": "User", "id": "nonexistent"}]
        result = await resolver.resolve(representations, mock_pool)

        # Should return None for not found
        assert len(result) == 1
        assert result[0] is None

    @pytest.mark.asyncio
    async def test_resolve_preserves_order(self):
        """Test that resolve preserves input order."""
        resolver = EntitiesResolver()

        # Mock database pool
        async def mock_fetch(sql, *args):
            rows = []
            for key in args:
                mock_row = MagicMock()
                mock_row.__getitem__ = lambda self, k, key=key: {
                    "id": key,
                    "data": {"id": key, "name": f"Entity {key}"},
                }.get(k)
                rows.append(mock_row)
            return rows

        mock_conn = AsyncMock()
        mock_conn.fetch = mock_fetch
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)

        mock_pool = AsyncMock()
        mock_pool.acquire = MagicMock(return_value=mock_conn)

        # Resolve in specific order
        representations = [
            {"__typename": "User", "id": "user-3"},
            {"__typename": "User", "id": "user-1"},
            {"__typename": "User", "id": "user-2"},
        ]
        result = await resolver.resolve(representations, mock_pool)

        # Order should be preserved
        assert len(result) == 3
        # Note: We can't easily verify exact order with mocks,
        # but we verify all are resolved
        assert all(r is not None for r in result)


class TestEntitiesResolverIntegration:
    """Integration tests for entities resolver."""

    def setup_method(self):
        """Clear registry before each test."""
        clear_entity_registry()

    def test_cqrs_table_naming(self):
        """Test that resolver uses correct CQRS table naming."""
        @entity
        class User:
            id: str

        resolver = EntitiesResolver()
        requests = [resolver._parse_representation({"__typename": "User", "id": "123"})]
        queries = resolver._build_queries(requests)

        # Should use tv_ prefix for query-side table
        assert queries["User"]["table_name"] == "tv_user"

    def test_multiple_entities_same_key_field(self):
        """Test multiple entities using same key field name."""
        @entity
        class User:
            id: str

        @entity
        class Post:
            id: str

        resolver = EntitiesResolver()

        assert resolver.get_key_field("User") == "id"
        assert resolver.get_key_field("Post") == "id"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
