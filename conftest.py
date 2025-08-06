# ABOUTME: Pytest configuration for FraiseQL test suite
# ABOUTME: Sets up test database URL and other test fixtures

import os

import pytest


def pytest_configure(config):
    """Configure pytest with test database URL."""
    # Only set test database URL if not already set and if we actually need it
    if "TEST_DATABASE_URL" not in os.environ:
        # Check if we have a standard PostgreSQL instance available
        standard_urls = [
            "postgresql://localhost/fraiseql_test",  # Standard PostgreSQL without auth
            "postgresql://fraiseql:fraiseql@localhost/fraiseql_test",  # With auth
            "postgresql://postgres:postgres@localhost/fraiseql_test",  # Common default
        ]

        # Try to connect to a standard PostgreSQL instance first
        import psycopg

        database_url = None

        for url in standard_urls:
            try:
                # Quick connection test
                with psycopg.connect(url, connect_timeout=2):
                    database_url = url
                    print(f"[conftest] Found working database: {url}")
                    break
            except (psycopg.Error, Exception):
                continue

        # If no local database found, let database_conftest.py handle testcontainers
        if database_url:
            os.environ["TEST_DATABASE_URL"] = database_url
        else:
            print("[conftest] No local database found, will use testcontainers if available")

    # Make test database URL available to tests if set
    if "TEST_DATABASE_URL" in os.environ:
        config.test_database_url = os.environ["TEST_DATABASE_URL"]


@pytest.fixture
def test_database_url(request):
    """Provide test database URL to tests."""
    return request.config.test_database_url
