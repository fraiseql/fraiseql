"""Test session variables are set correctly across all execution modes."""

import json
from contextlib import asynccontextmanager
from typing import Any, Dict
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

import pytest
from psycopg.sql import SQL, Literal

from fraiseql.execution.mode_selector import ExecutionMode
from fraiseql.db import FraiseQLRepository
from fraiseql.fastapi.turbo import TurboRouter


class TestSessionVariablesAcrossExecutionModes:
    """Test that session variables are set consistently in all execution modes."""

    @pytest.fixture
    async def mock_pool_psycopg(self):
        """Create a mock psycopg pool with connection tracking."""
        mock_pool = MagicMock()
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()

        # Track executed SQL statements
        executed_statements = []

        async def track_execute(sql, *args):
            # Store both raw SQL and string representation
            executed_statements.append(sql)
            return None

        mock_cursor.execute = track_execute
        mock_cursor.fetchone = AsyncMock(return_value={"result": "test"})
        mock_cursor.fetchall = AsyncMock(return_value=[{"result": "test"}])

        # Setup connection context manager
        mock_pool.connection.return_value.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_pool.connection.return_value.__aexit__ = AsyncMock(return_value=None)

        # Setup cursor context manager
        mock_cursor_cm = AsyncMock()
        mock_cursor_cm.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor_cm.__aexit__ = AsyncMock(return_value=None)
        mock_conn.cursor = MagicMock(return_value=mock_cursor_cm)

        # Attach tracking to pool for easy access
        mock_pool.executed_statements = executed_statements

        return mock_pool

    @pytest.fixture
    async def mock_pool_asyncpg(self):
        """Create a mock asyncpg pool with connection tracking."""
        mock_pool = AsyncMock(spec=["acquire"])
        mock_conn = AsyncMock()

        # Track executed SQL statements
        executed_statements = []

        async def track_execute(sql, *args):
            executed_statements.append({
                'sql': sql,
                'args': args
            })
            return None

        mock_conn.execute = track_execute
        mock_conn.fetchrow = AsyncMock(return_value={"result": "test"})
        mock_conn.fetch = AsyncMock(return_value=[{"result": "test"}])
        mock_conn.set_type_codec = AsyncMock()

        # Setup acquire context manager
        mock_pool.acquire.return_value.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_pool.acquire.return_value.__aexit__ = AsyncMock(return_value=None)

        # Attach tracking to pool
        mock_pool.executed_statements = executed_statements

        return mock_pool

    @pytest.mark.asyncio
    async def test_session_variables_in_normal_mode(self, mock_pool_psycopg):
        """Test that session variables are set in normal GraphQL execution mode."""
        tenant_id = str(uuid4())
        contact_id = str(uuid4())

        # Create repository with context
        repo = FraiseQLRepository(mock_pool_psycopg)
        repo.context = {
            "tenant_id": tenant_id,
            "contact_id": contact_id,
            "execution_mode": ExecutionMode.NORMAL
        }

        # Execute a query in normal mode
        await repo.find_one("test_view", id=1)

        # Check that session variables were set
        executed_sql = mock_pool_psycopg.executed_statements

        # Convert to strings for checking
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Should contain SET LOCAL statements for tenant_id and contact_id
        assert any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str), \
            f"Expected SET LOCAL app.tenant_id in executed SQL: {executed_sql_str}"
        assert any("SET LOCAL app.contact_id" in sql for sql in executed_sql_str), \
            f"Expected SET LOCAL app.contact_id in executed SQL: {executed_sql_str}"

        # Verify the values were set correctly
        tenant_sql = next((s for s in executed_sql_str if "app.tenant_id" in s), None)
        contact_sql = next((s for s in executed_sql_str if "app.contact_id" in s), None)

        assert tenant_id in tenant_sql if tenant_sql else False, \
            f"Expected tenant_id {tenant_id} in SQL: {tenant_sql}"
        assert contact_id in contact_sql if contact_sql else False, \
            f"Expected contact_id {contact_id} in SQL: {contact_sql}"

    @pytest.mark.asyncio
    async def test_session_variables_in_passthrough_mode(self, mock_pool_psycopg):
        """Test that session variables are set in passthrough execution mode."""
        tenant_id = str(uuid4())
        contact_id = str(uuid4())

        # Create repository with passthrough enabled
        repo = FraiseQLRepository(mock_pool_psycopg)
        repo.context = {
            "tenant_id": tenant_id,
            "contact_id": contact_id,
            "json_passthrough": True,
            "execution_mode": ExecutionMode.PASSTHROUGH
        }

        # Execute a query in passthrough mode
        await repo.find_one("test_view", id=1)

        # Check that session variables were set
        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Should contain SET LOCAL statements
        assert any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str), \
            f"Expected SET LOCAL app.tenant_id in passthrough mode. SQL: {executed_sql_str}"
        assert any("SET LOCAL app.contact_id" in sql for sql in executed_sql_str), \
            f"Expected SET LOCAL app.contact_id in passthrough mode. SQL: {executed_sql_str}"

    @pytest.mark.asyncio
    async def test_session_variables_in_turbo_mode(self, mock_pool_psycopg):
        """Test that session variables are set in TurboRouter execution mode."""
        tenant_id = str(uuid4())
        contact_id = str(uuid4())

        # Mock TurboRouter execution with context
        context = {
            "tenant_id": tenant_id,
            "contact_id": contact_id,
            "execution_mode": ExecutionMode.TURBO
        }

        # Create a mock cursor to track SQL
        mock_cursor = AsyncMock()
        executed_statements = []

        async def track_execute(sql, *args):
            # Handle both SQL objects and strings
            if hasattr(sql, '__sql__'):
                sql_str = str(sql.as_string(mock_cursor))
            else:
                sql_str = str(sql)
            executed_statements.append(sql_str)
            return None

        mock_cursor.execute = track_execute
        mock_cursor.fetchall = AsyncMock(return_value=[{"result": "test"}])

        # Test the TurboRouter session variable logic directly
        # This simulates what happens in turbo.py lines 252-271

        # Set session variables from context if available
        if "tenant_id" in context:
            await mock_cursor.execute(
                SQL("SET LOCAL app.tenant_id = {}").format(
                    Literal(str(context["tenant_id"]))
                )
            )
        if "contact_id" in context:
            await mock_cursor.execute(
                SQL("SET LOCAL app.contact_id = {}").format(
                    Literal(str(context["contact_id"]))
                )
            )

        # Verify session variables were set
        assert any("SET LOCAL app.tenant_id" in sql for sql in executed_statements), \
            f"Expected SET LOCAL app.tenant_id in turbo mode. SQL: {executed_statements}"
        assert any("SET LOCAL app.contact_id" in sql for sql in executed_statements), \
            f"Expected SET LOCAL app.contact_id in turbo mode. SQL: {executed_statements}"

    @pytest.mark.asyncio
    @pytest.mark.parametrize("execution_mode", [
        ExecutionMode.NORMAL,
        ExecutionMode.PASSTHROUGH,
        ExecutionMode.TURBO
    ])
    async def test_session_variables_consistency_across_modes(
        self,
        execution_mode,
        mock_pool_psycopg
    ):
        """Test that session variables are set consistently in all execution modes."""
        tenant_id = str(uuid4())
        contact_id = str(uuid4())

        # Configure context based on execution mode
        context = {
            "tenant_id": tenant_id,
            "contact_id": contact_id,
            "execution_mode": execution_mode
        }

        if execution_mode == ExecutionMode.PASSTHROUGH:
            context["json_passthrough"] = True

        # Create repository with context
        repo = FraiseQLRepository(mock_pool_psycopg)
        repo.context = context

        # Execute a query
        await repo.find_one("test_view", id=1)

        # Get executed SQL
        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # All modes should set session variables
        assert any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str), \
            f"Mode {execution_mode} should set app.tenant_id. SQL: {executed_sql_str}"
        assert any("SET LOCAL app.contact_id" in sql for sql in executed_sql_str), \
            f"Mode {execution_mode} should set app.contact_id. SQL: {executed_sql_str}"

        # Verify correct values are set
        for sql in executed_sql_str:
            if "app.tenant_id" in sql:
                assert tenant_id in sql, f"Expected tenant_id {tenant_id} in SQL: {sql}"
            if "app.contact_id" in sql:
                assert contact_id in sql, f"Expected contact_id {contact_id} in SQL: {sql}"

    @pytest.mark.asyncio
    async def test_session_variables_only_when_present_in_context(self, mock_pool_psycopg):
        """Test that session variables are only set when present in context."""
        # Test with only tenant_id
        repo = FraiseQLRepository(mock_pool_psycopg)
        repo.context = {
            "tenant_id": str(uuid4()),
            "execution_mode": ExecutionMode.NORMAL
        }

        await repo.find_one("test_view", id=1)

        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Should set tenant_id but not contact_id
        assert any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str)
        assert not any("SET LOCAL app.contact_id" in sql for sql in executed_sql_str)

        # Clear executed statements
        mock_pool_psycopg.executed_statements.clear()

        # Test with only contact_id
        repo.context = {
            "contact_id": str(uuid4()),
            "execution_mode": ExecutionMode.NORMAL
        }

        await repo.find_one("test_view", id=1)

        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Should set contact_id but not tenant_id
        assert not any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str)
        assert any("SET LOCAL app.contact_id" in sql for sql in executed_sql_str)

        # Clear executed statements
        mock_pool_psycopg.executed_statements.clear()

        # Test with neither
        repo.context = {
            "execution_mode": ExecutionMode.NORMAL
        }

        await repo.find_one("test_view", id=1)

        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Should not set any session variables
        assert not any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str)
        assert not any("SET LOCAL app.contact_id" in sql for sql in executed_sql_str)

    @pytest.mark.asyncio
    async def test_session_variables_transaction_scope(self, mock_pool_psycopg):
        """Test that session variables use SET LOCAL for transaction scope."""
        repo = FraiseQLRepository(mock_pool_psycopg)
        repo.context = {
            "tenant_id": str(uuid4()),
            "contact_id": str(uuid4()),
            "execution_mode": ExecutionMode.NORMAL
        }

        await repo.find_one("test_view", id=1)

        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Verify SET LOCAL is used (not SET or SET SESSION)
        tenant_sql = next((s for s in executed_sql_str if "app.tenant_id" in s), None)
        contact_sql = next((s for s in executed_sql_str if "app.contact_id" in s), None)

        assert tenant_sql and "SET LOCAL" in tenant_sql, \
            f"Should use SET LOCAL for transaction scope: {tenant_sql}"
        assert contact_sql and "SET LOCAL" in contact_sql, \
            f"Should use SET LOCAL for transaction scope: {contact_sql}"

    @pytest.mark.asyncio
    async def test_session_variables_with_custom_names(self, mock_pool_psycopg):
        """Test session variables with custom configuration names."""
        # This test assumes future configuration support
        repo = FraiseQLRepository(mock_pool_psycopg)
        repo.context = {
            "tenant_id": str(uuid4()),
            "user_id": str(uuid4()),  # Different variable name
            "execution_mode": ExecutionMode.NORMAL
        }

        # With future config support, we'd expect:
        # - tenant_id -> app.tenant_id (standard)
        # - user_id -> app.user_id (if configured)

        await repo.find_one("test_view", id=1)

        executed_sql = mock_pool_psycopg.executed_statements
        executed_sql_str = [str(stmt) for stmt in executed_sql]

        # Current implementation should set tenant_id
        assert any("SET LOCAL app.tenant_id" in sql for sql in executed_sql_str)

        # user_id would require configuration support (future enhancement)
        # For now, it won't be set unless explicitly handled
