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
        class_db_pool,
        clear_registry_class,
        create_fraiseql_app_with_db,
        create_test_table,
        create_test_view,
        db_connection,
        db_connection_committed,
        db_cursor,
        postgres_container,
        postgres_url,
        test_schema,
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


@pytest.fixture(scope="session")
def clear_type_caches() -> Generator[None]:
    """Clear type caches at session start and end.

    Use explicitly when tests need clean type registry state.

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


@pytest.fixture(scope="function")
def clear_registry() -> Generator[None]:
    """Clear the schema registry before and after each test.

    Use explicitly on tests that need schema registry isolation.
    This fixture performs comprehensive cleanup of all FraiseQL global state
    including Rust schema registry and FastAPI dependencies.

    Yields:
        None: This fixture performs setup/teardown only.
    """
    _clear_all_fraiseql_state()
    yield
    _clear_all_fraiseql_state()


def _clear_all_fraiseql_state() -> None:
    """Comprehensive cleanup of all FraiseQL global state.

    This function resets:
    - Python SchemaRegistry
    - Rust schema registry
    - FastAPI global dependencies (db_pool, auth_provider, config)
    - GraphQL type caches
    - Type registry (view mappings)
    """
    if not FRAISEQL_AVAILABLE:
        return

    # Clear type caches FIRST (before resetting Rust registry to avoid conflicts)
    try:
        if _graphql_type_cache is not None:
            _graphql_type_cache.clear()
        if _type_registry is not None:
            _type_registry.clear()
    except Exception:
        pass

    # Clear view type registry mapping
    try:
        from fraiseql.db import _view_type_registry
        _view_type_registry.clear()
    except ImportError:
        pass
    except Exception:
        pass

    # Clear Python SchemaRegistry
    try:
        SchemaRegistry.get_instance().clear()  # type: ignore
    except Exception:
        pass

    # Reset Rust schema registry LAST (after Python state is cleared)
    try:
        from fraiseql._fraiseql_rs import reset_schema_registry_for_testing
        reset_schema_registry_for_testing()
    except ImportError:
        pass
    except Exception:
        pass

    # Reset FastAPI global dependencies
    try:
        from fraiseql.fastapi.dependencies import (
            set_auth_provider,
            set_db_pool,
            set_fraiseql_config,
        )
        set_db_pool(None)
        set_auth_provider(None)
        set_fraiseql_config(None)
    except ImportError:
        pass
    except Exception:
        pass


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
