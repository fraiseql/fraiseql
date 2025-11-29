import pytest

# Try to import FraiseQL components, skip if not available
try:
    from fraiseql.config.schema_config import SchemaConfig
    from fraiseql.core.graphql_type import _graphql_type_cache
    from fraiseql.db import _type_registry
    from fraiseql.gql.schema_builder import SchemaRegistry

    FRAISEQL_AVAILABLE = True
except ImportError:
    FRAISEQL_AVAILABLE = False
    SchemaConfig = None
    SchemaRegistry = None
    _graphql_type_cache = None
    _type_registry = None

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

try:
    from tests.fixtures.cascade.conftest import *  # noqa: F403
except ImportError:
    pass  # Skip cascade fixtures if dependencies not available


@pytest.fixture(autouse=True, scope="session")
def clear_type_caches() -> None:
    """Clear type caches once at session start and end."""
    if not FRAISEQL_AVAILABLE:
        return

    # Clear at session start
    _graphql_type_cache.clear()
    _type_registry.clear()

    yield

    # Clear at session end
    _graphql_type_cache.clear()
    _type_registry.clear()


@pytest.fixture(autouse=True, scope="function")
def clear_registry() -> None:
    """Clear the schema registry before and after each test."""
    if not FRAISEQL_AVAILABLE:
        pytest.skip("FraiseQL not available - skipping registry fixture")

    # Clear before test - only the singleton registry needs per-test isolation
    SchemaRegistry.get_instance().clear()

    yield

    # Clear after test
    SchemaRegistry.get_instance().clear()


@pytest.fixture
def use_snake_case() -> None:
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
