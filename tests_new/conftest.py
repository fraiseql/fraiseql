"""Main test configuration for FraiseQL test suite.

This module provides the central configuration for all FraiseQL tests,
importing and organizing fixtures from the centralized fixtures system.
It establishes the foundation for unit, integration, and E2E testing
with proper fixture scoping and dependency management.

The configuration follows a layered approach:
- Core fixtures (database, auth, GraphQL)
- Layer-specific fixtures (unit, integration, e2e)
- Utility functions and markers
- Test environment configuration
"""

import os
import sys
from pathlib import Path

import pytest

# Add src directory to Python path for imports
current_dir = Path(__file__).parent
project_root = current_dir.parent
src_dir = project_root / "src"

if str(src_dir) not in sys.path:
    sys.path.insert(0, str(src_dir))

# Core fixture imports - these provide the foundation
from tests_new.fixtures.database import (
    # Container and connection management
    postgres_container,
    postgres_url,
    db_pool,
    db_connection,
    db_cursor,
    db_connection_committed,
    db_schema_builder,

    # Utilities and helpers
    temporary_schema,
    database_required,
    slow_database
)

# Blog-specific fixtures
from tests_new.fixtures.blog_database import (
    blog_schema_setup,
    blog_with_test_data,
    blog_e2e_workflow,
    clean_blog_db
)

from tests_new.fixtures.auth import (
    # JWT and token management
    jwt_secret,
    jwt_algorithm,
    test_token_factory,

    # User fixtures
    admin_user,
    regular_user,
    guest_user,
    inactive_user,

    # Token fixtures
    admin_token,
    user_token,
    guest_token,
    expired_token,

    # Request mocking
    mock_request_factory,
    authenticated_request,
    admin_request,
    unauthenticated_request,

    # GraphQL context fixtures
    graphql_context_factory,
    admin_context,
    user_context,
    guest_context,
    anonymous_context,

    # CSRF and security
    csrf_token,
    csrf_request,

    # Auth0 integration
    auth0_config,
    auth0_user,

    # Utility functions
    has_permission,
    is_admin,
    is_authenticated
)

from tests_new.fixtures.graphql import (
    # Schema management
    clear_schema_registry,
    schema_config_factory,

    # Application factories
    fraiseql_app_factory,
    graphql_client_factory,
    simple_graphql_client,

    # Query building
    query_builder,

    # Validation utilities
    validate_schema_introspection,
    find_type_in_schema,

    # Test data
    sample_users,
    sample_posts
)

from tests_new.fixtures.mock_data import (
    # Data factories
    user_factory,
    post_factory,
    comment_factory,
    category_factory,
    analytics_factory,

    # Complete datasets
    sample_blog_data,

    # Utilities
    generate_jsonb_data,
    sequence_generator
)

# Test configuration and markers
def pytest_configure(config):
    """Configure pytest with custom markers and options."""

    # Register custom markers
    config.addinivalue_line(
        "markers",
        "unit: Unit tests (fast, isolated, no external dependencies)"
    )
    config.addinivalue_line(
        "markers",
        "integration: Integration tests (database, external services)"
    )
    config.addinivalue_line(
        "markers",
        "e2e: End-to-end tests (full system behavior)"
    )
    config.addinivalue_line(
        "markers",
        "performance: Performance and benchmark tests"
    )
    config.addinivalue_line(
        "markers",
        "security: Security-focused tests"
    )
    config.addinivalue_line(
        "markers",
        "slow: Tests that take a long time to run"
    )
    config.addinivalue_line(
        "markers",
        "database: Tests that require database access"
    )
    config.addinivalue_line(
        "markers",
        "auth: Tests that require authentication setup"
    )
    config.addinivalue_line(
        "markers",
        "blog_demo: Blog demo specific tests"
    )
    config.addinivalue_line(
        "markers",
        "regression: Regression tests for specific bugs"
    )
    config.addinivalue_line(
        "markers",
        "skip_ci: Skip in CI environment"
    )

    # Set test environment variables
    os.environ.setdefault("ENV", "test")
    os.environ.setdefault("DEBUG", "false")
    os.environ.setdefault("LOG_LEVEL", "WARNING")


def pytest_addoption(parser):
    """Add custom command line options."""

    parser.addoption(
        "--no-db",
        action="store_true",
        default=False,
        help="Skip database integration tests"
    )

    parser.addoption(
        "--no-docker",
        action="store_true",
        default=False,
        help="Skip tests requiring Docker"
    )

    parser.addoption(
        "--run-slow",
        action="store_true",
        default=False,
        help="Run slow tests (skipped by default)"
    )

    parser.addoption(
        "--run-e2e",
        action="store_true",
        default=False,
        help="Run E2E tests (skipped by default)"
    )

    parser.addoption(
        "--benchmark",
        action="store_true",
        default=False,
        help="Run performance benchmark tests"
    )

    parser.addoption(
        "--parallel",
        action="store_true",
        default=False,
        help="Enable parallel test execution"
    )


def pytest_collection_modifyitems(config, items):
    """Modify test collection based on command line options and markers."""

    # Skip database tests if --no-db flag is provided
    if config.getoption("--no-db"):
        skip_db = pytest.mark.skip(reason="Skipping database tests (--no-db flag)")
        for item in items:
            if "database" in item.keywords:
                item.add_marker(skip_db)

    # Skip Docker tests if --no-docker flag is provided
    if config.getoption("--no-docker"):
        skip_docker = pytest.mark.skip(reason="Skipping Docker tests (--no-docker flag)")
        for item in items:
            if "docker" in item.keywords:
                item.add_marker(skip_docker)

    # Skip slow tests unless --run-slow is provided
    if not config.getoption("--run-slow"):
        skip_slow = pytest.mark.skip(reason="Slow test (use --run-slow to run)")
        for item in items:
            if "slow" in item.keywords:
                item.add_marker(skip_slow)

    # Skip E2E tests unless --run-e2e is provided
    if not config.getoption("--run-e2e"):
        skip_e2e = pytest.mark.skip(reason="E2E test (use --run-e2e to run)")
        for item in items:
            if "e2e" in item.keywords:
                item.add_marker(skip_e2e)

    # Skip performance tests unless --benchmark is provided
    if not config.getoption("--benchmark"):
        skip_perf = pytest.mark.skip(reason="Performance test (use --benchmark to run)")
        for item in items:
            if "performance" in item.keywords:
                item.add_marker(skip_perf)

    # Skip CI-specific tests if in CI environment
    if os.environ.get("CI") or os.environ.get("GITHUB_ACTIONS"):
        skip_ci = pytest.mark.skip(reason="Skipped in CI environment")
        for item in items:
            if "skip_ci" in item.keywords:
                item.add_marker(skip_ci)


def pytest_sessionstart(session):
    """Actions to perform at the start of test session."""

    print("🚀 Starting FraiseQL test suite...")

    # Print test environment info
    env = os.environ.get("ENV", "unknown")
    print(f"   Environment: {env}")

    # Check for required dependencies
    try:
        import testcontainers
        print("   ✅ Testcontainers available")
    except ImportError:
        print("   ❌ Testcontainers not available")

    try:
        import docker
        docker.from_env().ping()
        print("   ✅ Docker available")
    except Exception:
        print("   ❌ Docker not available")

    # Setup test data directories
    test_data_dir = Path(__file__).parent / "data"
    test_data_dir.mkdir(exist_ok=True)

    # Set additional environment variables for testing
    os.environ.setdefault("FRAISEQL_TEST_MODE", "true")
    os.environ.setdefault("FRAISEQL_LOG_LEVEL", "WARNING")


def pytest_sessionfinish(session, exitstatus):
    """Actions to perform at the end of test session."""

    print("\n🏁 FraiseQL test suite completed")

    if exitstatus == 0:
        print("   ✅ All tests passed")
    else:
        print(f"   ❌ Tests failed (exit code: {exitstatus})")

    # Cleanup test data if needed
    cleanup_test_data = os.environ.get("CLEANUP_TEST_DATA", "true").lower() == "true"
    if cleanup_test_data:
        test_data_dir = Path(__file__).parent / "data"
        if test_data_dir.exists():
            import shutil
            shutil.rmtree(test_data_dir, ignore_errors=True)
            print("   🧹 Cleaned up test data")


# Global test fixtures that apply across all test types
@pytest.fixture(autouse=True)
def test_environment():
    """Ensure clean test environment for each test."""

    # Store original environment
    original_env = os.environ.copy()

    # Set test-specific environment variables
    test_env = {
        "ENV": "test",
        "DEBUG": "false",
        "LOG_LEVEL": "WARNING",
        "TESTING": "true",
    }

    os.environ.update(test_env)

    yield

    # Restore original environment
    os.environ.clear()
    os.environ.update(original_env)


@pytest.fixture(scope="session")
def test_config():
    """Test configuration dictionary."""
    return {
        "database": {
            "url": os.environ.get("TEST_DATABASE_URL"),
            "pool_size": 5,
            "timeout": 30,
        },
        "auth": {
            "jwt_secret": "test-jwt-secret",
            "jwt_algorithm": "HS256",
            "token_expiration": 3600,
        },
        "graphql": {
            "introspection": True,
            "playground": True,
            "debug": True,
        },
        "performance": {
            "query_timeout": 30,
            "max_query_depth": 10,
            "max_query_complexity": 1000,
        }
    }


@pytest.fixture
def mock_logger():
    """Mock logger for testing logging functionality."""
    from unittest.mock import Mock

    logger = Mock()
    logger.debug = Mock()
    logger.info = Mock()
    logger.warning = Mock()
    logger.error = Mock()
    logger.critical = Mock()

    return logger


@pytest.fixture
def temporary_file():
    """Create temporary file for testing file operations."""
    import tempfile

    with tempfile.NamedTemporaryFile(mode='w+', delete=False) as f:
        yield f.name

    # Cleanup
    try:
        os.unlink(f.name)
    except (OSError, FileNotFoundError):
        pass


@pytest.fixture
def temporary_directory():
    """Create temporary directory for testing."""
    import tempfile
    import shutil

    temp_dir = tempfile.mkdtemp()

    yield temp_dir

    # Cleanup
    try:
        shutil.rmtree(temp_dir)
    except (OSError, FileNotFoundError):
        pass


# Performance testing utilities
@pytest.fixture
def benchmark_config():
    """Configuration for performance benchmarks."""
    return {
        "max_query_time": 0.1,  # 100ms
        "max_mutation_time": 0.2,  # 200ms
        "max_memory_usage": 100 * 1024 * 1024,  # 100MB
        "min_queries_per_second": 100,
    }


# Error handling utilities
@pytest.fixture
def error_handler():
    """Mock error handler for testing error scenarios."""
    from unittest.mock import Mock

    handler = Mock()
    handler.handle_error = Mock()
    handler.log_error = Mock()
    handler.format_error = Mock(return_value="Formatted error message")

    return handler


# Test isolation utilities
@pytest.fixture(autouse=True)
def isolate_tests():
    """Ensure tests are properly isolated from each other."""

    # Clear any global state before test
    yield

    # Clear any global state after test
    # This runs after each test to ensure clean state


# Debugging utilities
@pytest.fixture
def debug_info():
    """Debug information collector for test analysis."""
    info = {
        "queries_executed": [],
        "mutations_executed": [],
        "errors_encountered": [],
        "performance_metrics": {},
    }

    yield info

    # Could log debug info if test fails
    # if hasattr(request.node, 'rep_call') and request.node.rep_call.failed:
    #     print(f"Debug info: {info}")


# Make sure all fixtures are available
__all__ = [
    # Database fixtures
    "postgres_container", "postgres_url", "db_pool", "db_connection",
    "db_cursor", "db_connection_committed", "db_schema_builder",

    # Blog database fixtures
    "blog_schema_setup", "blog_with_test_data", "blog_e2e_workflow", "clean_blog_db",

    # Auth fixtures
    "jwt_secret", "admin_user", "user_token", "authenticated_request",
    "admin_context", "user_context",

    # GraphQL fixtures
    "clear_schema_registry", "graphql_client_factory", "simple_graphql_client",
    "query_builder",

    # Mock data fixtures
    "user_factory", "post_factory", "sample_blog_data",

    # Test utilities
    "test_config", "mock_logger", "temporary_file", "benchmark_config",
    "error_handler", "debug_info"
]
