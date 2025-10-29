import pytest

# Try to import FraiseQL components, skip if not available
try:
    from fraiseql.config.schema_config import SchemaConfig
    from fraiseql.gql.schema_builder import SchemaRegistry

    FRAISEQL_AVAILABLE = True
except ImportError:
    FRAISEQL_AVAILABLE = False
    SchemaConfig = None
    SchemaRegistry = None

# Import fixtures from the new organized structure
# Import examples fixtures first as they don't require heavy dependencies
from tests.fixtures.examples.conftest_examples import *  # noqa: F403

# Try to import database and auth fixtures if dependencies are available
try:
    from tests.fixtures.database.database_conftest import *  # noqa: F403
except ImportError:
    pass  # Skip database fixtures if dependencies not available

try:
    from tests.fixtures.auth.conftest_auth import *  # noqa: F403
except ImportError:
    pass  # Skip auth fixtures if dependencies not available


@pytest.fixture
def clear_registry():
    """Clear the registry before and after each test."""
    if not FRAISEQL_AVAILABLE:
        pytest.skip("FraiseQL not available - skipping registry fixture")

    # Clear the registry before and after each test
    SchemaRegistry.get_instance().clear()
    # Also clear the GraphQL type cache
    from fraiseql.core.graphql_type import _graphql_type_cache

    _graphql_type_cache.clear()

    # Clear the database type registry
    from fraiseql.db import _type_registry

    _type_registry.clear()

    yield
    SchemaRegistry.get_instance().clear()
    _graphql_type_cache.clear()
    _type_registry.clear()


@pytest.fixture
def use_snake_case():
    """Fixture to use snake_case field names in tests."""
    if not FRAISEQL_AVAILABLE:
        pytest.skip("FraiseQL not available - skipping snake_case fixture")

    # Save current config
    original_config = SchemaConfig.get_instance().camel_case_fields

    # Set to snake_case
    SchemaConfig.set_config(camel_case_fields=False)

    yield

    # Restore original config
    SchemaConfig.set_config(camel_case_fields=original_config)
