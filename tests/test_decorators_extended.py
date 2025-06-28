"""Extended tests for FraiseQL decorators to improve coverage."""

import asyncio
from typing import AsyncGenerator
from unittest.mock import MagicMock, patch

import pytest

from fraiseql.decorators import field, query, subscription
from fraiseql.gql.schema_builder import SchemaRegistry


class TestQueryDecorator:
    """Test query decorator functionality."""

    def test_query_decorator_with_function(self):
        """Test query decorator applied directly to function."""
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @query
            async def test_query():
                return "test"
            
            # Should register the function
            mock_instance.register_query.assert_called_once_with(test_query)
            
            # Function should remain unchanged
            assert test_query.__name__ == "test_query"

    def test_query_decorator_without_parentheses(self):
        """Test query decorator without parentheses."""
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @query
            def sync_query():
                return "sync"
            
            mock_instance.register_query.assert_called_once_with(sync_query)

    def test_query_decorator_with_parentheses(self):
        """Test query decorator with parentheses."""
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @query()
            async def parameterized_query():
                return "parameterized"
            
            mock_instance.register_query.assert_called_once_with(parameterized_query)


class TestSubscriptionDecorator:
    """Test subscription decorator functionality."""

    def test_subscription_decorator_with_function(self):
        """Test subscription decorator applied directly to function."""
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @subscription
            async def test_subscription() -> AsyncGenerator[str, None]:
                yield "test"
            
            # Should register the function
            mock_instance.register_subscription.assert_called_once_with(test_subscription)
            
            # Function should remain unchanged
            assert test_subscription.__name__ == "test_subscription"

    def test_subscription_decorator_with_parentheses(self):
        """Test subscription decorator with parentheses."""
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @subscription()
            async def parameterized_subscription():
                yield "parameterized"
            
            mock_instance.register_subscription.assert_called_once_with(parameterized_subscription)


class TestFieldDecorator:
    """Test field decorator functionality."""

    def test_field_decorator_simple(self):
        """Test field decorator with simple usage."""
        class TestType:
            @field
            def simple_field(self):
                return "simple"
        
        # Should add resolver metadata
        assert hasattr(TestType.simple_field, '__fraiseql_field__')

    def test_field_decorator_with_resolver(self):
        """Test field decorator with custom resolver."""
        def custom_resolver(obj, info):
            return "custom"
        
        class TestType:
            @field(resolver=custom_resolver)
            def custom_field(self):
                return "original"
        
        assert TestType.custom_field.__fraiseql_field_resolver__ is custom_resolver

    def test_field_decorator_with_description(self):
        """Test field decorator with description."""
        class TestType:
            @field(description="A test field")
            def described_field(self):
                return "described"
        
        assert TestType.described_field.__fraiseql_field_description__ == "A test field"

    def test_field_decorator_with_all_params(self):
        """Test field decorator with all parameters."""
        def resolver_func(obj, info):
            return "resolved"
        
        class TestType:
            @field(resolver=resolver_func, description="Full field")
            def full_field(self):
                return "full"
        
        assert TestType.full_field.__fraiseql_field_resolver__ is resolver_func
        assert TestType.full_field.__fraiseql_field_description__ == "Full field"

    def test_field_decorator_parameterized(self):
        """Test field decorator with parentheses but no parameters."""
        class TestType:
            @field()
            def empty_params_field(self):
                return "empty"
        
        assert hasattr(TestType.empty_params_field, '__fraiseql_field__')

    def test_field_decorator_preserves_function(self):
        """Test field decorator preserves original function."""
        class TestType:
            @field
            def preserved_field(self):
                return "preserved"
        
        # Should be callable and return original value
        instance = TestType()
        assert instance.preserved_field() == "preserved"




class TestDecoratorEdgeCases:
    """Test edge cases for decorators."""

    def test_query_decorator_none_function(self):
        """Test query decorator returns decorator when called with no args."""
        decorator_func = query()
        assert callable(decorator_func)
        
        # Should work when applied
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @decorator_func
            def delayed_query():
                return "delayed"
            
            mock_instance.register_query.assert_called_once_with(delayed_query)

    def test_subscription_decorator_none_function(self):
        """Test subscription decorator returns decorator when called with no args."""
        decorator_func = subscription()
        assert callable(decorator_func)
        
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @decorator_func
            def delayed_subscription():
                yield "delayed"
            
            mock_instance.register_subscription.assert_called_once_with(delayed_subscription)

    def test_field_decorator_none_function(self):
        """Test field decorator returns decorator when called with params."""
        decorator_func = field(description="Test description")
        assert callable(decorator_func)
        
        class TestType:
            @decorator_func
            def delayed_field(self):
                return "delayed"
        
        assert TestType.delayed_field.__fraiseql_field_description__ == "Test description"

    def test_multiple_decorators_combination(self):
        """Test combining multiple decorators."""
        with patch.object(SchemaRegistry, 'get_instance') as mock_registry:
            mock_instance = MagicMock()
            mock_registry.return_value = mock_instance
            
            @query
            @field
            async def combined_function():
                return "combined"
            
            # Query should be registered
            mock_instance.register_query.assert_called_once()
            
            # Function should still be callable
            assert asyncio.iscoroutinefunction(combined_function)

    def test_schema_registry_error_handling(self):
        """Test decorator behavior when schema registry fails."""
        with patch.object(SchemaRegistry, 'get_instance', side_effect=Exception("Registry error")):
            # Should not raise, just skip registration
            @query
            def error_query():
                return "error"
            
            # Function should still exist and be callable
            assert error_query() == "error"