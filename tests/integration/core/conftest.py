"""Core module test fixtures."""

import pytest

# Import database fixtures
from tests.fixtures.database.database_conftest import (
    class_db_pool,
    test_schema,
    postgres_url,
    postgres_container,
)

from fraiseql.db import FraiseQLRepository


@pytest.fixture
def db_repo(class_db_pool) -> FraiseQLRepository:
    """FraiseQL repository fixture for core tests."""
    return FraiseQLRepository(class_db_pool)
