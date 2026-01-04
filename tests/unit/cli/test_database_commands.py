"""Tests for database CLI commands (Phase 19, Commit 7).

Tests the database monitoring CLI commands.
"""

from datetime import UTC, datetime

import pytest
from click.testing import CliRunner

from fraiseql.monitoring.db_monitor import DatabaseMonitor, QueryMetrics
from fraiseql.monitoring.runtime.db_monitor_sync import DatabaseMonitorSync, set_database_monitor
from fraiseql.cli.monitoring.database_commands import database


@pytest.fixture
def runner() -> CliRunner:
    """Create a Click test runner."""
    return CliRunner()


@pytest.fixture
def test_monitor() -> DatabaseMonitor:
    """Create a fresh DatabaseMonitor for testing."""
    monitor = DatabaseMonitor()
    set_database_monitor(monitor)
    return monitor


@pytest.fixture
def monitor_with_queries(test_monitor: DatabaseMonitor) -> DatabaseMonitor:
    """Create a monitor populated with test data."""
    # Clear the singleton instance and reset
    from fraiseql.monitoring.runtime.db_monitor_sync import DatabaseMonitorSync
    test_monitor._recent_queries.clear()

    for i in range(5):
        query = QueryMetrics(
            query_id=f"q{i}",
            query_hash=f"hash{i}",
            query_type="SELECT",
            timestamp=datetime.now(UTC),
            duration_ms=float(10 + i * 5),
            rows_affected=10 + i,
        )
        test_monitor._recent_queries.append(query)

    # Update the singleton to use this monitor
    from fraiseql.monitoring.runtime.db_monitor_sync import set_database_monitor
    set_database_monitor(test_monitor)

    return test_monitor


class TestDatabaseRecentCommand:
    """Tests for 'database recent' command."""

    def test_recent_no_data(self, runner: CliRunner, test_monitor: DatabaseMonitor) -> None:
        """Test recent command with no data."""
        result = runner.invoke(database, ["recent"])

        assert result.exit_code == 0
        assert "No queries" in result.output

    def test_recent_with_data(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test recent command with data."""
        result = runner.invoke(database, ["recent"])

        assert result.exit_code == 0
        assert "SELECT" in result.output

    def test_recent_with_limit(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test recent command with limit option."""
        result = runner.invoke(database, ["recent", "--limit", "2"])

        assert result.exit_code == 0

    def test_recent_json_format(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test recent command with JSON format."""
        result = runner.invoke(database, ["recent", "--format", "json"])

        assert result.exit_code == 0
        import json

        data = json.loads(result.output)
        assert isinstance(data, list)

    def test_recent_csv_format(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test recent command with CSV format."""
        result = runner.invoke(database, ["recent", "--format", "csv"])

        assert result.exit_code == 0
        assert "Timestamp" in result.output

    def test_recent_filter_by_type(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test recent command with type filter."""
        result = runner.invoke(database, ["recent", "--type", "SELECT"])

        assert result.exit_code == 0


class TestDatabaseSlowCommand:
    """Tests for 'database slow' command."""

    def test_slow_no_data(self, runner: CliRunner, test_monitor: DatabaseMonitor) -> None:
        """Test slow command with no data."""
        result = runner.invoke(database, ["slow"])

        assert result.exit_code == 0
        assert "No queries" in result.output or "slower than" in result.output

    def test_slow_with_threshold(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test slow command with custom threshold."""
        result = runner.invoke(database, ["slow", "--threshold", "100"])

        assert result.exit_code == 0

    def test_slow_json_format(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test slow command with JSON format."""
        result = runner.invoke(database, ["slow", "--format", "json"])

        assert result.exit_code == 0


class TestDatabasePoolCommand:
    """Tests for 'database pool' command."""

    def test_pool_no_data(self, runner: CliRunner, test_monitor: DatabaseMonitor) -> None:
        """Test pool command with no data."""
        result = runner.invoke(database, ["pool"])

        assert result.exit_code == 0

    def test_pool_json_format(self, runner: CliRunner) -> None:
        """Test pool command with JSON format."""
        result = runner.invoke(database, ["pool", "--format", "json"])

        assert result.exit_code == 0


class TestDatabaseStatsCommand:
    """Tests for 'database stats' command."""

    def test_stats_no_data(self, runner: CliRunner, test_monitor: DatabaseMonitor) -> None:
        """Test stats command with no data."""
        result = runner.invoke(database, ["stats"])

        assert result.exit_code == 0

    def test_stats_with_data(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test stats command with data."""
        result = runner.invoke(database, ["stats"])

        assert result.exit_code == 0
        assert "Total Queries" in result.output or "total" in result.output.lower()

    def test_stats_json_format(
        self, runner: CliRunner, monitor_with_queries: DatabaseMonitor
    ) -> None:
        """Test stats command with JSON format."""
        result = runner.invoke(database, ["stats", "--format", "json"])

        assert result.exit_code == 0
        import json

        data = json.loads(result.output)
        assert "total_count" in data
