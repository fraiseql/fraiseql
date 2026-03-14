"""Unit tests for FraiseQLRepository._set_session_variables.

Verifies that session variables including fraiseql.started_at are injected
via SET LOCAL before each query/mutation execution.
"""

from unittest.mock import AsyncMock, call

import pytest
from psycopg.sql import SQL

from fraiseql.db import FraiseQLRepository

pytestmark = pytest.mark.unit

STARTED_AT_SQL = SQL("SET LOCAL fraiseql.started_at = clock_timestamp()::text")
STARTED_AT_STR = "SET LOCAL fraiseql.started_at = clock_timestamp()::text"


def _make_repo(context: dict | None = None) -> FraiseQLRepository:
    """Create a FraiseQLRepository with a mock pool and optional context."""
    pool = AsyncMock()
    return FraiseQLRepository(pool=pool, context=context or {})


class TestStartedAtSessionVariable:
    """Tests for fraiseql.started_at injection in _set_session_variables."""

    @pytest.mark.asyncio
    async def test_started_at_injected_with_psycopg_cursor(self) -> None:
        """fraiseql.started_at is SET LOCAL'd via a psycopg cursor using SQL()."""
        repo = _make_repo()
        cursor = AsyncMock()
        cursor.execute = AsyncMock()
        cursor.fetchone = AsyncMock()

        await repo._set_session_variables(cursor)

        cursor.execute.assert_called_with(STARTED_AT_SQL)

    @pytest.mark.asyncio
    async def test_started_at_injected_with_asyncpg_connection(self) -> None:
        """fraiseql.started_at is SET LOCAL'd via an asyncpg connection as plain string."""
        repo = _make_repo()
        conn = AsyncMock()
        conn.execute = AsyncMock()
        if hasattr(conn, "fetchone"):
            del conn.fetchone

        await repo._set_session_variables(conn)

        conn.execute.assert_called_with(STARTED_AT_STR)

    @pytest.mark.asyncio
    async def test_started_at_is_last_set_local(self) -> None:
        """Ensure started_at is the last SET LOCAL.

        This guarantees the timestamp is captured closest to actual query execution.
        """
        repo = _make_repo({"tenant_id": "t1", "user_id": "u1"})
        cursor = AsyncMock()
        cursor.execute = AsyncMock()
        cursor.fetchone = AsyncMock()

        await repo._set_session_variables(cursor)

        last_call = cursor.execute.call_args_list[-1]
        assert last_call == call(STARTED_AT_SQL)

    @pytest.mark.asyncio
    async def test_started_at_injected_even_without_context(self) -> None:
        """fraiseql.started_at is always injected, regardless of context contents."""
        repo = _make_repo({})
        cursor = AsyncMock()
        cursor.execute = AsyncMock()
        cursor.fetchone = AsyncMock()

        await repo._set_session_variables(cursor)

        assert cursor.execute.call_count == 1
        cursor.execute.assert_called_once_with(STARTED_AT_SQL)
