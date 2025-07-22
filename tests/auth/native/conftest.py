"""Import database fixtures for native auth tests."""

from pathlib import Path

import pytest

# Import specific database fixtures from the main conftest
from tests.database_conftest import (
    db_connection,
    db_connection_committed,
    db_pool,
    postgres_url,
)


@pytest.fixture
async def db_with_native_auth(db_connection_committed):
    """Database connection with native auth schema applied."""
    async with db_connection_committed.cursor() as cursor:
        # Get current schema
        await cursor.execute("SELECT current_schema()")
        schema = (await cursor.fetchone())[0]

        # Read and apply migration
        migration_path = (
            Path(__file__).parent.parent.parent.parent
            / "src/fraiseql/auth/native/migrations/001_native_auth_schema.sql"
        )
        with open(migration_path) as f:
            migration_sql = f.read()

        # Execute migration
        await cursor.execute(migration_sql)
        await db_connection_committed.commit()

    return db_connection_committed
