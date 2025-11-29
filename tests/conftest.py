import os
import pytest

# Check if Rust extension should be skipped for performance optimization
SKIP_RUST = os.getenv("FRAISEQL_SKIP_RUST") == "1"

# Try to import FraiseQL components, skip if not available
try:
    from fraiseql.config.schema_config import SchemaConfig
    from fraiseql.core.graphql_type import _graphql_type_cache
    from fraiseql.db import _type_registry
    from fraiseql.gql.schema_builder import SchemaRegistry

    FRAISEQL_AVAILABLE = True
except ImportError:
    FRAISEQL_AVAILABLE = False
    SchemaConfig = None  # type: ignore
    SchemaRegistry = None  # type: ignore
    _graphql_type_cache = None  # type: ignore
    _type_registry = None  # type: ignore

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
def clear_registry(request) -> None:
    """Clear the schema registry before and after each test.

    Optimized: Only clears for tests that need full isolation.
    Unit tests skip expensive clearing for better performance.
    """
    if not FRAISEQL_AVAILABLE:
        return  # Skip if not available

    # Only do expensive clearing for tests that need full isolation
    needs_isolation = any(
        marker in request.keywords
        for marker in ["database", "integration", "e2e", "forked", "slow", "enterprise"]
    )

    if needs_isolation:
        # Clear before test for heavy tests that need isolation
        SchemaRegistry.get_instance().clear()  # type: ignore

    yield

    if needs_isolation:
        # Clear after test for heavy tests
        SchemaRegistry.get_instance().clear()  # type: ignore


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


def pytest_collection_modifyitems(config, items):
    """Skip Rust-dependent tests when FRAISEQL_SKIP_RUST=1."""
    if SKIP_RUST:
        skip_rust = pytest.mark.skip(reason="Rust extension disabled via FRAISEQL_SKIP_RUST=1")
        for item in items:
            # Skip tests that import or use Rust extension
            if any(marker in item.keywords for marker in ["rust"]):
                item.add_marker(skip_rust)
            # Also skip tests that have 'rust' in the filename or path
            elif "rust" in str(item.fspath).lower():
                item.add_marker(skip_rust)
