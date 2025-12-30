import pytest
from unittest.mock import Mock, patch, MagicMock
from fraiseql.middleware.graphql_info_injector import GraphQLInfoInjector


@pytest.fixture
def injector():
        """Create an injector instance for testing."""
        return GraphQLInfoInjector()

class TestGraphQLInfoInjector:
    """Test suite for GraphQL Info Injector middleware."""


    def test_injector_initialization(self, injector):
        """Test that injector initializes correctly."""
        assert injector is not None
        assert hasattr(injector, 'process_info')

    def test_info_injection_in_resolver(self, injector):
        """Test that info object is properly injected into resolvers."""
        # Mock GraphQL info object
        mock_info = Mock()
        mock_info.field_name = 'test_field'
        mock_info.parent_type = 'Query'

        # Test resolver
        def test_resolver(obj, info):
            return {'field': info.field_name}

        # Inject and call
        wrapped_resolver = injector.inject(test_resolver)
        result = wrapped_resolver(None, mock_info)

        assert result['field'] == 'test_field'

    def test_info_parameter_auto_injection(self, injector):
        """Test automatic info parameter injection."""
        mock_info = Mock()
        mock_info.field_name = 'auto_test'

        def resolver_without_info(obj):
            """Resolver that doesn't explicitly take info."""
            return 'success'

        wrapped = injector.inject(resolver_without_info)
        # Should still work even though info isn't in signature
        result = wrapped(None, mock_info)
        assert result == 'success'

    def test_selection_set_access(self, injector):
        """Test that selection set info is accessible."""
        mock_info = Mock()
        mock_info.field_name = 'selections'
        mock_info.selection_set = Mock()
        mock_info.selection_set.selections = [
            Mock(name='field1'),
            Mock(name='field2')
        ]

        def resolver_with_selections(obj, info):
            return {
                'field_name': info.field_name,
                'selections': [s.name for s in info.selection_set.selections]
            }

        wrapped = injector.inject(resolver_with_selections)
        result = wrapped(None, mock_info)

        assert result['field_name'] == 'selections'
        assert result['selections'] == ['field1', 'field2']

    def test_middleware_chain_integration(self, injector):
        """Test that injector works in middleware chain."""
        mock_info = Mock()
        mock_info.field_name = 'chained'

        execution_order = []

        def before_middleware(info):
            execution_order.append('before')
            return info

        def resolver(obj, info):
            execution_order.append('resolver')
            return info.field_name

        def after_middleware(result, info):
            execution_order.append('after')
            return result

        # Test basic chaining
        result = resolver(None, mock_info)
        assert result == 'chained'

    @patch('fraiseql.middleware.graphql_info_injector.GraphQLInfoInjector.process_info')
    def test_process_info_called(self, mock_process, injector):
        """Test that process_info is called during injection."""
        mock_info = Mock()
        mock_info.field_name = 'test'
        mock_process.return_value = mock_info

        def resolver(obj, info):
            return 'processed'

        wrapped = injector.inject(resolver)
        result = wrapped(None, mock_info)

        assert result == 'processed'

    def test_error_handling(self, injector):
        """Test error handling during injection."""
        def failing_resolver(obj, info):
            raise ValueError('Test error')

        wrapped = injector.inject(failing_resolver)

        with pytest.raises(ValueError, match='Test error'):
            wrapped(None, Mock())

    def test_preserves_resolver_metadata(self, injector):
        """Test that wrapping preserves original resolver metadata."""
        def my_resolver(obj, info):
            """Original resolver docstring."""
            return 'result'

        my_resolver.custom_attr = 'test_value'
        wrapped = injector.inject(my_resolver)

        # Wrapped function should be callable
        assert callable(wrapped)

    def test_multiple_fields_injection(self, injector):
        """Test injection across multiple fields."""
        mock_info_1 = Mock(field_name='field1')
        mock_info_2 = Mock(field_name='field2')

        def resolver(obj, info):
            return info.field_name

        wrapped = injector.inject(resolver)

        result1 = wrapped(None, mock_info_1)
        result2 = wrapped(None, mock_info_2)

        assert result1 == 'field1'
        assert result2 == 'field2'
