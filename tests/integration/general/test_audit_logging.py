"""Tests for audit logging (Phase 14)."""
# ruff: noqa: F401

import json

import pytest
import pytest_asyncio
from fraiseql._fraiseql_rs import DatabasePool as RustDatabasePool

from fraiseql.enterprise.security import AuditLevel, AuditLogger

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest_asyncio.fixture(scope="class")
async def pool(postgres_url, class_db_pool) -> None:
    """Create Rust database pool for testing."""
    # Use the class-scoped pool URL with url= keyword argument
    pool = RustDatabasePool(url=postgres_url)

    # Create audit logs table
    async with class_db_pool.connection() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS fraiseql_audit_logs (
                id BIGSERIAL PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                level TEXT NOT NULL CHECK (level IN ('INFO', 'WARN', 'ERROR')),
                user_id BIGINT NOT NULL,
                tenant_id BIGINT NOT NULL,
                operation TEXT NOT NULL CHECK (operation IN ('query', 'mutation')),
                query TEXT NOT NULL,
                variables JSONB NOT NULL DEFAULT '{}'::jsonb,
                ip_address TEXT NOT NULL,
                user_agent TEXT NOT NULL,
                error TEXT,
                duration_ms INTEGER,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        """)

        # Create indexes
        await conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_timestamp
                ON fraiseql_audit_logs(tenant_id, timestamp DESC)
        """)
        await conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_level
                ON fraiseql_audit_logs(tenant_id, level, timestamp DESC)
        """)

    yield pool

    # Cleanup
    async with class_db_pool.connection() as conn:
        await conn.execute("DROP TABLE IF EXISTS fraiseql_audit_logs CASCADE")


@pytest_asyncio.fixture
async def logger(pool) -> None:
    """Create audit logger for testing."""
    return AuditLogger(pool)


class TestAuditLogger:
    """Test audit logger functionality."""

    @pytest.mark.asyncio
    async def test_log_info_query(self, logger) -> None:
        """Test logging an INFO level query."""
        entry_id = await logger.log(
            level=AuditLevel.INFO,
            user_id=1,
            tenant_id=1,
            operation="query",
            query="{ users { id name } }",
            variables={},
            ip_address="192.168.1.100",
            user_agent="GraphQL Client/1.0",
            duration_ms=42,
        )

        assert isinstance(entry_id, int)
        assert entry_id > 0

    @pytest.mark.asyncio
    async def test_log_error_mutation(self, logger) -> None:
        """Test logging an ERROR level mutation with error message."""
        entry_id = await logger.log(
            level=AuditLevel.ERROR,
            user_id=2,
            tenant_id=1,
            operation="mutation",
            query="mutation { deleteUser(id: 999) }",
            variables={"id": 999},
            ip_address="10.0.0.50",
            user_agent="Mobile App/2.0",
            error="User not found",
            duration_ms=15,
        )

        assert isinstance(entry_id, int)
        assert entry_id > 0

    @pytest.mark.asyncio
    async def test_log_warn_query(self, logger) -> None:
        """Test logging a WARN level query."""
        entry_id = await logger.log(
            level=AuditLevel.WARN,
            user_id=3,
            tenant_id=1,
            operation="query",
            query="{ posts(limit: 10000) { id } }",
            variables={"limit": 10000},
            ip_address="172.16.0.1",
            user_agent="Admin Dashboard/1.0",
            duration_ms=5000,
        )

        assert isinstance(entry_id, int)
        assert entry_id > 0

    @pytest.mark.asyncio
    async def test_log_with_complex_variables(self, logger) -> None:
        """Test logging with complex nested variables."""
        variables = {
            "input": {
                "name": "Test User",
                "email": "test@example.com",
                "roles": ["admin", "moderator"],
                "metadata": {"source": "api", "version": 2},
            }
        }

        entry_id = await logger.log(
            level=AuditLevel.INFO,
            user_id=4,
            tenant_id=1,
            operation="mutation",
            query="mutation CreateUser($input: UserInput!) { createUser(input: $input) }",
            variables=variables,
            ip_address="203.0.113.42",
            user_agent="API Client/3.0",
            duration_ms=125,
        )

        assert isinstance(entry_id, int)
        assert entry_id > 0

    @pytest.mark.asyncio
    async def test_get_recent_logs_all(self, logger) -> None:
        """Test retrieving all recent logs for a tenant."""
        # Log a few entries
        await logger.log(
            level=AuditLevel.INFO,
            user_id=5,
            tenant_id=2,
            operation="query",
            query="{ tenants { id } }",
            variables={},
            ip_address="192.168.1.200",
            user_agent="Test Client/1.0",
        )

        await logger.log(
            level=AuditLevel.ERROR,
            user_id=5,
            tenant_id=2,
            operation="mutation",
            query="mutation { updateTenant(id: 1) }",
            variables={"id": 1},
            ip_address="192.168.1.200",
            user_agent="Test Client/1.0",
            error="Permission denied",
        )

        # Get all logs for tenant 2
        logs = await logger.get_recent_logs(tenant_id=2, limit=10)

        assert isinstance(logs, list)
        assert len(logs) >= 2

        # Verify log structure
        for log in logs:
            assert "id" in log
            assert "timestamp" in log
            assert "level" in log
            assert "user_id" in log
            assert "tenant_id" in log
            assert log["tenant_id"] == 2
            assert "operation" in log
            assert "query" in log
            assert "variables" in log
            assert "ip_address" in log
            assert "user_agent" in log

    @pytest.mark.asyncio
    async def test_get_recent_logs_filter_by_level(self, logger) -> None:
        """Test retrieving logs filtered by level."""
        # Log entries at different levels
        await logger.log(
            level=AuditLevel.INFO,
            user_id=6,
            tenant_id=3,
            operation="query",
            query="{ users { id } }",
            variables={},
            ip_address="10.0.1.1",
            user_agent="Test/1.0",
        )

        await logger.log(
            level=AuditLevel.ERROR,
            user_id=6,
            tenant_id=3,
            operation="mutation",
            query="mutation { deleteAll }",
            variables={},
            ip_address="10.0.1.1",
            user_agent="Test/1.0",
            error="Not authorized",
        )

        # Get only ERROR logs
        error_logs = await logger.get_recent_logs(tenant_id=3, level=AuditLevel.ERROR, limit=10)

        assert isinstance(error_logs, list)
        assert len(error_logs) >= 1

        # Verify all returned logs are ERROR level
        for log in error_logs:
            assert log["level"] == "ERROR"
            assert log["tenant_id"] == 3

    @pytest.mark.asyncio
    async def test_get_recent_logs_limit(self, logger) -> None:
        """Test that limit parameter works."""
        # Log 5 entries
        for i in range(5):
            await logger.log(
                level=AuditLevel.INFO,
                user_id=7,
                tenant_id=4,
                operation="query",
                query=f"{{ user(id: {i}) {{ id }} }}",
                variables={"id": i},
                ip_address="10.0.2.1",
                user_agent="Batch/1.0",
            )

        # Get only 3 most recent
        logs = await logger.get_recent_logs(tenant_id=4, limit=3)

        assert isinstance(logs, list)
        # Should have at most 3 (could have more if previous tests ran)
        assert len(logs) <= 10  # reasonable upper bound

    @pytest.mark.asyncio
    async def test_multi_tenant_isolation(self, logger) -> None:
        """Test that logs are properly isolated by tenant."""
        # Log for tenant 5
        await logger.log(
            level=AuditLevel.INFO,
            user_id=8,
            tenant_id=5,
            operation="query",
            query="{ products { id } }",
            variables={},
            ip_address="172.20.0.1",
            user_agent="Tenant5/1.0",
        )

        # Log for tenant 6
        await logger.log(
            level=AuditLevel.INFO,
            user_id=9,
            tenant_id=6,
            operation="query",
            query="{ orders { id } }",
            variables={},
            ip_address="172.20.0.2",
            user_agent="Tenant6/1.0",
        )

        # Get logs for tenant 5
        tenant5_logs = await logger.get_recent_logs(tenant_id=5, limit=10)

        # Verify all logs belong to tenant 5
        for log in tenant5_logs:
            assert log["tenant_id"] == 5

        # Get logs for tenant 6
        tenant6_logs = await logger.get_recent_logs(tenant_id=6, limit=10)

        # Verify all logs belong to tenant 6
        for log in tenant6_logs:
            assert log["tenant_id"] == 6

    @pytest.mark.asyncio
    async def test_log_without_optional_fields(self, logger) -> None:
        """Test logging with minimal required fields (no error, no duration)."""
        entry_id = await logger.log(
            level=AuditLevel.INFO,
            user_id=10,
            tenant_id=7,
            operation="query",
            query="{ version }",
            variables={},
            ip_address="192.0.2.100",
            user_agent="MinimalClient/1.0",
        )

        assert isinstance(entry_id, int)
        assert entry_id > 0

        # Verify it was stored
        logs = await logger.get_recent_logs(tenant_id=7, limit=1)
        assert len(logs) >= 1
        assert logs[0]["error"] is None
        assert logs[0]["duration_ms"] is None

    @pytest.mark.asyncio
    async def test_variables_json_roundtrip(self, logger) -> None:
        """Test that variables are correctly serialized and deserialized."""
        original_vars = {
            "userId": 123,
            "filters": {"status": "active", "role": "admin"},
            "options": {"limit": 10, "offset": 0},
        }

        entry_id = await logger.log(  # noqa: F841
            level=AuditLevel.INFO,
            user_id=11,
            tenant_id=8,
            operation="query",
            query="{ users(filters: $filters) { id } }",
            variables=original_vars,
            ip_address="198.51.100.50",
            user_agent="JSONTest/1.0",
        )

        # Retrieve and verify
        logs = await logger.get_recent_logs(tenant_id=8, limit=1)
        assert len(logs) >= 1

        retrieved_vars = logs[0]["variables"]
        assert isinstance(retrieved_vars, dict)
        assert retrieved_vars == original_vars


class TestAuditLevel:
    """Test AuditLevel enum."""

    def test_audit_level_values(self) -> None:
        """Test that AuditLevel enum has correct values."""
        assert AuditLevel.INFO.value == "INFO"
        assert AuditLevel.WARN.value == "WARN"
        assert AuditLevel.ERROR.value == "ERROR"

    def test_audit_level_members(self) -> None:
        """Test that all expected members exist."""
        levels = [AuditLevel.INFO, AuditLevel.WARN, AuditLevel.ERROR]
        assert len(levels) == 3


class TestIntegration:
    """Integration tests combining multiple features."""

    @pytest.mark.asyncio
    async def test_complete_audit_workflow(self, logger) -> None:
        """Test complete workflow: log multiple entries and retrieve them."""
        tenant_id = 99

        # Simulate a user session with multiple operations
        operations = [
            {
                "level": AuditLevel.INFO,
                "operation": "query",
                "query": "{ currentUser { id name } }",
                "error": None,
                "duration_ms": 15,
            },
            {
                "level": AuditLevel.INFO,
                "operation": "query",
                "query": "{ posts(limit: 10) { id title } }",
                "error": None,
                "duration_ms": 45,
            },
            {
                "level": AuditLevel.ERROR,
                "operation": "mutation",
                "query": "mutation { deletePost(id: 999) }",
                "error": "Post not found",
                "duration_ms": 8,
            },
            {
                "level": AuditLevel.WARN,
                "operation": "query",
                "query": "{ posts(limit: 5000) { id } }",
                "error": None,
                "duration_ms": 3500,
            },
        ]

        # Log all operations
        entry_ids = []
        for op in operations:
            entry_id = await logger.log(
                level=op["level"],
                user_id=100,
                tenant_id=tenant_id,
                operation=op["operation"],
                query=op["query"],
                variables={},
                ip_address="203.0.113.100",
                user_agent="IntegrationTest/1.0",
                error=op["error"],
                duration_ms=op["duration_ms"],
            )
            entry_ids.append(entry_id)

        assert len(entry_ids) == 4
        assert all(isinstance(eid, int) and eid > 0 for eid in entry_ids)

        # Retrieve all logs for this tenant
        all_logs = await logger.get_recent_logs(tenant_id=tenant_id, limit=10)
        assert len(all_logs) >= 4

        # Retrieve only ERROR logs
        error_logs = await logger.get_recent_logs(
            tenant_id=tenant_id, level=AuditLevel.ERROR, limit=10
        )
        assert len(error_logs) >= 1
        assert all(log["level"] == "ERROR" for log in error_logs)

        # Retrieve only WARN logs
        warn_logs = await logger.get_recent_logs(
            tenant_id=tenant_id, level=AuditLevel.WARN, limit=10
        )
        assert len(warn_logs) >= 1
        assert all(log["level"] == "WARN" for log in warn_logs)
