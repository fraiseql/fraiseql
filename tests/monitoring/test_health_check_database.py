"""Tests for pre-built database health check functions."""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from fraiseql.monitoring.health import CheckResult, HealthStatus
from fraiseql.monitoring.health_checks import check_database, check_pool_stats

pytestmark = pytest.mark.integration


class TestDatabaseHealthCheck:
    """Test database connectivity check."""

    @pytest.mark.asyncio
    async def test_check_database_success(self) -> None:
        """Test successful database connectivity check."""
        # Mock database pool
        mock_pool = MagicMock()
        mock_conn = AsyncMock()
        mock_result = AsyncMock()
        mock_result.fetchone.return_value = ("PostgreSQL 16.3",)

        # Setup async context manager for connection
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        mock_conn.execute.return_value = mock_result

        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=mock_pool):
            result = await check_database()

        assert isinstance(result, CheckResult)
        assert result.name == "database"
        assert result.status == HealthStatus.HEALTHY
        assert "connected" in result.message.lower() or "success" in result.message.lower()

    @pytest.mark.asyncio
    async def test_check_database_connection_failure(self) -> None:
        """Test database check when connection fails."""
        # Mock database pool that raises exception
        mock_pool = MagicMock()
        mock_pool.connection.side_effect = Exception("Connection refused")

        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=mock_pool):
            result = await check_database()

        assert isinstance(result, CheckResult)
        assert result.name == "database"
        assert result.status == HealthStatus.UNHEALTHY
        assert "connection refused" in result.message.lower() or "failed" in result.message.lower()

    @pytest.mark.asyncio
    async def test_check_database_no_pool_available(self) -> None:
        """Test database check when pool is not available."""
        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=None):
            result = await check_database()

        assert isinstance(result, CheckResult)
        assert result.name == "database"
        assert result.status == HealthStatus.UNHEALTHY
        assert (
            "not available" in result.message.lower() or "not configured" in result.message.lower()
        )

    @pytest.mark.asyncio
    async def test_check_database_with_metadata(self) -> None:
        """Test that database check includes version metadata."""
        mock_pool = MagicMock()
        mock_conn = AsyncMock()
        mock_result = AsyncMock()
        mock_result.fetchone.return_value = ("PostgreSQL 16.3 on x86_64-pc-linux-gnu",)

        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
        mock_conn.execute.return_value = mock_result

        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=mock_pool):
            result = await check_database()

        assert result.status == HealthStatus.HEALTHY
        # Should include version metadata
        assert "version" in result.metadata or "database_version" in result.metadata


class TestPoolStatsHealthCheck:
    """Test connection pool statistics check."""

    @pytest.mark.asyncio
    async def test_check_pool_stats_success(self) -> None:
        """Test successful pool stats check."""
        mock_pool = MagicMock()
        mock_pool.get_stats.return_value = {
            "pool_size": 10,
            "pool_available": 7,
        }
        mock_pool.max_size = 20
        mock_pool.min_size = 5

        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=mock_pool):
            result = await check_pool_stats()

        assert isinstance(result, CheckResult)
        assert result.name == "database_pool"
        assert result.status == HealthStatus.HEALTHY
        assert result.metadata["pool_size"] == 10
        assert result.metadata["active_connections"] == 3  # 10 - 7
        assert result.metadata["idle_connections"] == 7

    @pytest.mark.asyncio
    async def test_check_pool_stats_high_usage(self) -> None:
        """Test pool stats check when pool is highly utilized."""
        mock_pool = MagicMock()
        mock_pool.get_stats.return_value = {
            "pool_size": 19,  # 95% utilization
            "pool_available": 1,
        }
        mock_pool.max_size = 20
        mock_pool.min_size = 5

        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=mock_pool):
            result = await check_pool_stats()

        assert isinstance(result, CheckResult)
        assert result.name == "database_pool"
        # Should be healthy but message should warn about high usage
        assert "95" in result.message or "high" in result.message.lower()

    @pytest.mark.asyncio
    async def test_check_pool_stats_no_pool(self) -> None:
        """Test pool stats check when pool is not available."""
        with patch("fraiseql.fastapi.dependencies.get_db_pool", return_value=None):
            result = await check_pool_stats()

        assert isinstance(result, CheckResult)
        assert result.name == "database_pool"
        assert result.status == HealthStatus.UNHEALTHY
        assert "not available" in result.message.lower()
