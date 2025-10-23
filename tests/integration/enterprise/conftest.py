"""Enterprise module test fixtures."""

import pytest
from fraiseql.db import FraiseQLRepository


@pytest.fixture
async def db_repo(db_pool):
    """FraiseQL repository fixture for enterprise tests."""
    return FraiseQLRepository(db_pool)
