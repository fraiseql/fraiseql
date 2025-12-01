import os
from collections.abc import Generator

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
from tests.fixtures.examples.conftest_examples import (  # noqa: F401
    blog_enterprise_app,
    blog_enterprise_client,
    blog_enterprise_db_url,
    blog_simple_app,
    blog_simple_client,
    blog_simple_context,
    blog_simple_db_connection,
    blog_simple_db_url,
    blog_simple_graphql_client,
    blog_simple_repository,
    examples_event_loop,
    reset_fraiseql_state,
    reset_fraiseql_state_module,
    sample_comment_data,
    sample_post_data,
    sample_tag_data,
    sample_user_data,
    smart_dependencies,
)

# Try to import database and auth fixtures if dependencies are available
try:
    from tests.fixtures.database.database_conftest import (  # noqa: F401
        create_fraiseql_app_with_db,
        create_test_table,
        create_test_view,
        db_connection,
        db_connection_committed,
        db_cursor,
        db_pool,
        postgres_container,
        postgres_url,
    )
except ImportError:
    pass  # Skip database fixtures if dependencies not available

try:
    from tests.fixtures.auth.conftest_auth import (  # noqa: F401
        admin_context,
        auth_context,
        authenticated_request,
        mock_auth_context,
        mock_csrf_request,
        mock_request_with_auth,
        unauthenticated_context,
        user_context,
    )
except ImportError:
    pass  # Skip auth fixtures if dependencies not available

try:
    from tests.fixtures.cascade.conftest import (  # noqa: F401
        cascade_app,
        cascade_client,
        cascade_db_schema,
        cascade_http_client,
        mock_apollo_client,
    )
except ImportError:
    pass  # Skip cascade fixtures if dependencies not available


@pytest.fixture(autouse=True, scope="session")
def clear_type_caches() -> Generator[None]:
    """Clear type caches at session start and end.

    This session-scoped fixture ensures clean type registry state
    at the beginning and end of test sessions.

    Yields:
        None: This fixture performs setup/teardown only.
    """
    if FRAISEQL_AVAILABLE:
        # Clear at session start
        _graphql_type_cache.clear()  # type: ignore
        _type_registry.clear()  # type: ignore

    yield

    if FRAISEQL_AVAILABLE:
        # Clear at session end
        _graphql_type_cache.clear()  # type: ignore
        _type_registry.clear()  # type: ignore


@pytest.fixture(autouse=True, scope="function")
def clear_registry(request: pytest.FixtureRequest) -> Generator[None]:
    """Clear the schema registry before and after each test.

    Optimized: Only clears for tests that need full isolation.
    Unit tests skip expensive clearing for better performance.

    Args:
        request: Pytest fixture request object for accessing test metadata.

    Yields:
        None: This fixture performs setup/teardown only.
    """
    if FRAISEQL_AVAILABLE:
        # Only do expensive clearing for tests that need full isolation
        needs_isolation = any(
            marker in request.keywords
            for marker in ["database", "integration", "e2e", "forked", "slow", "enterprise"]
        )

        if needs_isolation:
            # Clear before test for heavy tests that need isolation
            SchemaRegistry.get_instance().clear()  # type: ignore

    yield

    if FRAISEQL_AVAILABLE:
        # Only do expensive clearing for tests that need full isolation
        needs_isolation = any(
            marker in request.keywords
            for marker in ["database", "integration", "e2e", "forked", "slow", "enterprise"]
        )

        if needs_isolation:
            # Clear after test for heavy tests
            SchemaRegistry.get_instance().clear()  # type: ignore


@pytest.fixture
def use_snake_case() -> Generator[None]:
    """Fixture to use snake_case field names in tests.

    Yields:
        None: This fixture performs setup/teardown only.
    """
    if not FRAISEQL_AVAILABLE:
        pytest.skip("FraiseQL not available - skipping snake_case fixture")

    # Save current config
    original_config = SchemaConfig.get_instance().camel_case_fields  # type: ignore

    # Set to snake_case
    SchemaConfig.set_config(camel_case_fields=False)  # type: ignore

    yield

    # Restore original config
    SchemaConfig.set_config(camel_case_fields=original_config)  # type: ignore


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    """Skip Rust-dependent tests when FRAISEQL_SKIP_RUST=1."""
    if SKIP_RUST:
        skip_rust = pytest.mark.skip(reason="Rust extension disabled via FRAISEQL_SKIP_RUST=1")
        for item in items:
            # Skip tests that import or use Rust extension
            if (
                any(marker in item.keywords for marker in ["rust"])
                or "rust" in str(item.fspath).lower()
            ):
                item.add_marker(skip_rust)
