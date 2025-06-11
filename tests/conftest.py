import pytest

from fraiseql.gql.schema_builder import SchemaRegistry

# Import database fixtures
from .database_conftest import *  # noqa: F403


@pytest.fixture
def clear_registry():
    # Clear the registry before and after each test
    SchemaRegistry.get_instance().clear()
    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()
    yield
    SchemaRegistry.get_instance().clear()
    _graphql_type_cache.clear()
