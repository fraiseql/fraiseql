from pathlib import Path

import pytest
import pytest_asyncio
from psycopg.sql import SQL

from fraiseql.db import DatabaseQuery

pytestmark = pytest.mark.integration


@pytest_asyncio.fixture(autouse=True, scope="class")
async def setup_audit_schema(class_db_pool, test_schema) -> None:
    """Set up audit schema before running tests."""
    # Read the migration file
    migration_path = Path("src/fraiseql/enterprise/migrations/001_audit_tables.sql")
    migration_sql = migration_path.read_text()

    # Execute the migration
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")
        await conn.execute(migration_sql)
        await conn.commit()


@pytest.mark.asyncio
async def test_audit_events_table_exists(db_repo) -> None:
    """Verify audit_events table exists with correct schema."""
    result = await db_repo.run(
        DatabaseQuery(
            statement=SQL(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'audit_events'"
            ),
            params={},
            fetch_result=True,
        )
    )

    required_columns = {
        "id": "uuid",
        "event_type": "character varying",
        "event_data": "jsonb",
        "user_id": "uuid",
        "tenant_id": "uuid",
        "timestamp": "timestamp with time zone",
        "ip_address": "inet",
        "previous_hash": "character varying",
        "event_hash": "character varying",
        "signature": "character varying",
    }

    assert len(result) >= len(required_columns)
    for row in result:
        column_name = row["column_name"]
        if column_name in required_columns:
            assert row["data_type"] == required_columns[column_name], (
                f"Column {column_name} has wrong type"
            )
