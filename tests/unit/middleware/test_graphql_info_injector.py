"""Unit tests for GraphQL info auto-injection middleware."""

import inspect
import pytest
from unittest.mock import AsyncMock, MagicMock

from fraiseql.middleware.graphql_info_injector import GraphQLInfoInjector


class TestGraphQLInfoInjection:
    """Tests for GraphQL info auto-injection."""

    def setup_method(self):
        """Set up test fixtures."""
        self.injector = GraphQLInfoInjector()

    def _create_mock_info(self):
        """Create mock GraphQLResolveInfo for testing."""
        return MagicMock(context={})

    @pytest.mark.asyncio
    async def test_info_injected_into_context(self):
        """Verify info is injected into context correctly."""
        mock_info = self._create_mock_info()

        @GraphQLInfoInjector.auto_inject
        async def resolver(info):
            return info.context.get("graphql_info")

        result = await resolver(mock_info)
        assert result == mock_info
        assert mock_info.context["graphql_info"] == mock_info

    @pytest.mark.asyncio
    async def test_explicit_info_parameter(self):
        """Verify explicit info parameter is injected properly."""
        mock_info = self._create_mock_info()

        @GraphQLInfoInjector.auto_inject
        async def resolver(info):
            return info

        result = await resolver(mock_info)
        assert result == mock_info
        assert mock_info.context["graphql_info"] == mock_info

    @pytest.mark.asyncio
    async def test_no_info_parameter_resolver(self):
        """Verify resolver without info parameter works."""
        @GraphQLInfoInjector.auto_inject
        async def resolver(param1, param2):
            return param1 + param2

        result = await resolver(1, 2)
        assert result == 3

    @pytest.mark.asyncio
    async def test_info_with_kwargs(self):
        """Verify info injection works with kwargs."""
        mock_info = self._create_mock_info()

        @GraphQLInfoInjector.auto_inject
        async def resolver(info, limit=100):
            return (info, limit)

        result = await resolver(info=mock_info, limit=50)
        assert result[0] == mock_info
        assert result[1] == 50
        assert mock_info.context["graphql_info"] == mock_info

    @pytest.mark.asyncio
    async def test_info_not_dict_context(self):
        """Verify handling when context is not a dict."""
        mock_info = MagicMock(context="not_a_dict")

        @GraphQLInfoInjector.auto_inject
        async def resolver(info):
            return info

        result = await resolver(mock_info)
        assert result == mock_info
        # Should not inject since context is not a dict

    @pytest.mark.asyncio
    async def test_backwards_compatibility(self):
        """Verify backwards compatibility with explicit info=info."""
        mock_info = self._create_mock_info()

        @GraphQLInfoInjector.auto_inject
        async def resolver(info=None):
            return info

        result = await resolver(mock_info)
        assert result == mock_info
