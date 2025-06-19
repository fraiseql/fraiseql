# ABOUTME: Pytest configuration for FraiseQL test suite
# ABOUTME: Sets up test database URL and other test fixtures

import os
import pytest


def pytest_configure(config):
    """Configure pytest with test database URL."""
    # Set test database URL if not already set
    if "TEST_DATABASE_URL" not in os.environ:
        # Default to localhost test database
        os.environ["TEST_DATABASE_URL"] = "postgresql://fraiseql_test:fraiseql_test@localhost:5435/fraiseql_test"
    
    # Make test database URL available to tests
    config.test_database_url = os.environ["TEST_DATABASE_URL"]


@pytest.fixture
def test_database_url(request):
    """Provide test database URL to tests."""
    return request.config.test_database_url