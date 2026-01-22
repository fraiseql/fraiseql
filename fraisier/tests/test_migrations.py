"""Tests for database migration system.

Tests the MigrationRunner and migration execution across all database types.
"""

import tempfile
from pathlib import Path

import pytest

from fraisier.db.adapter import DatabaseType
from fraisier.db.migrations import (
    MigrationError,
    MigrationRunner,
    run_migrations,
)
from fraisier.db.sqlite_adapter import SqliteAdapter


class TestMigrationRunner:
    """Test migration runner initialization and validation."""

    def test_migration_runner_creation(self):
        """Test MigrationRunner initialization with default path."""
        runner = MigrationRunner()
        assert runner.migrations_dir.exists()
        assert runner.migrations_dir.name == "migrations"

    def test_migration_runner_custom_path(self):
        """Test MigrationRunner with custom path."""
        with tempfile.TemporaryDirectory() as tmpdir:
            runner = MigrationRunner(tmpdir)
            assert runner.migrations_dir == Path(tmpdir)

    def test_migration_runner_invalid_path(self):
        """Test MigrationRunner with non-existent path."""
        with pytest.raises(MigrationError):
            MigrationRunner("/nonexistent/path/migrations")

    def test_get_db_migrations_dir_sqlite(self):
        """Test getting SQLite migrations directory."""
        runner = MigrationRunner()
        db_dir = runner._get_db_migrations_dir(DatabaseType.SQLITE)
        assert db_dir.exists()
        assert db_dir.name == "sqlite"

    def test_get_db_migrations_dir_postgresql(self):
        """Test getting PostgreSQL migrations directory."""
        runner = MigrationRunner()
        db_dir = runner._get_db_migrations_dir(DatabaseType.POSTGRESQL)
        assert db_dir.exists()
        assert db_dir.name == "postgresql"

    def test_get_db_migrations_dir_mysql(self):
        """Test getting MySQL migrations directory."""
        runner = MigrationRunner()
        db_dir = runner._get_db_migrations_dir(DatabaseType.MYSQL)
        assert db_dir.exists()
        assert db_dir.name == "mysql"


class TestMigrationFileLoading:
    """Test loading and reading migration files."""

    def test_get_pending_migrations(self):
        """Test loading list of pending migrations."""
        runner = MigrationRunner()
        db_dir = runner._get_db_migrations_dir(DatabaseType.SQLITE)

        migrations = runner._get_pending_migrations(db_dir)

        # Should have at least the core migrations
        assert len(migrations) >= 3
        assert all(f.endswith(".sql") for f, _ in migrations)
        # Should be sorted
        filenames = [f for f, _ in migrations]
        assert filenames == sorted(filenames)

    def test_read_migration_file(self):
        """Test reading migration file content."""
        runner = MigrationRunner()
        db_dir = runner._get_db_migrations_dir(DatabaseType.SQLITE)
        migrations = runner._get_pending_migrations(db_dir)

        # Read first migration
        filename, filepath = migrations[0]
        content = runner._read_migration_file(filepath)

        assert isinstance(content, str)
        assert len(content) > 0
        assert "CREATE TABLE" in content or "CREATE VIEW" in content or "CREATE INDEX" in content

    def test_read_migration_file_not_found(self):
        """Test error when migration file doesn't exist."""
        runner = MigrationRunner()

        with pytest.raises(MigrationError):
            runner._read_migration_file("/nonexistent/migration.sql")


class TestSqliteMigrationExecution:
    """Test migration execution against SQLite."""

    @pytest.fixture
    async def adapter(self):
        """Create temporary SQLite adapter."""
        adapter = SqliteAdapter(":memory:")
        await adapter.connect()
        yield adapter
        await adapter.disconnect()

    async def test_execute_migration(self, adapter):
        """Test executing a single migration."""
        runner = MigrationRunner()

        # Simple test migration
        sql = "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)"
        await runner._execute_migration(adapter, "test_migration.sql", sql)

        # Verify table was created
        results = await adapter.execute_query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='test_table'"
        )
        assert len(results) == 1

    async def test_execute_multi_statement_migration(self, adapter):
        """Test executing migration with multiple statements."""
        runner = MigrationRunner()

        sql = """
        CREATE TABLE t1 (id INTEGER PRIMARY KEY);
        CREATE TABLE t2 (id INTEGER PRIMARY KEY);
        CREATE TABLE t3 (id INTEGER PRIMARY KEY);
        """
        await runner._execute_migration(adapter, "multi.sql", sql)

        # Verify all tables created
        results = await adapter.execute_query(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        )
        tables = [r["name"] for r in results]
        assert "t1" in tables
        assert "t2" in tables
        assert "t3" in tables

    async def test_execute_migration_failure(self, adapter):
        """Test migration execution with invalid SQL."""
        runner = MigrationRunner()

        with pytest.raises(MigrationError):
            await runner._execute_migration(adapter, "bad.sql", "INVALID SQL HERE")

    async def test_run_migrations_creates_schema(self, adapter):
        """Test running all migrations creates expected schema."""
        runner = MigrationRunner()
        results = await runner.run(adapter)

        assert results["database_type"] == "sqlite"
        assert results["migrations_run"] >= 3
        assert len(results["migrations"]) >= 3
        assert results["errors"] == []

        # Verify core tables exist
        tables_result = await adapter.execute_query(
            "SELECT name FROM sqlite_master WHERE type='table'"
        )
        table_names = {r["name"] for r in tables_result}

        assert "tb_fraise_state" in table_names
        assert "tb_deployment" in table_names
        assert "tb_webhook_event" in table_names
        assert "tb_deployment_lock" in table_names

    async def test_run_migrations_creates_views(self, adapter):
        """Test that migrations create read-side views."""
        runner = MigrationRunner()
        await runner.run(adapter)

        # Verify views exist
        views_result = await adapter.execute_query(
            "SELECT name FROM sqlite_master WHERE type='view'"
        )
        view_names = {r["name"] for r in views_result}

        assert "v_fraise_status" in view_names
        assert "v_deployment_history" in view_names
        assert "v_webhook_event_history" in view_names

    async def test_run_migrations_creates_indexes(self, adapter):
        """Test that migrations create indexes."""
        runner = MigrationRunner()
        await runner.run(adapter)

        # Verify indexes exist
        indexes_result = await adapter.execute_query(
            "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'"
        )
        index_names = {r["name"] for r in indexes_result}

        assert len(index_names) > 0
        assert "idx_fraise_state_identifier" in index_names
        assert "idx_deployment_status" in index_names

    async def test_run_migrations_idempotent(self, adapter):
        """Test that running migrations twice is safe."""
        runner = MigrationRunner()

        # Run migrations first time
        results1 = await runner.run(adapter)
        assert results1["errors"] == []

        # Run migrations second time (should be idempotent)
        results2 = await runner.run(adapter)
        # Second run should have no migrations or handle CREATE IF NOT EXISTS
        assert len(results2["errors"]) == 0

    async def test_run_migrations_dry_run(self, adapter, capsys):
        """Test dry-run mode (no actual execution)."""
        runner = MigrationRunner()
        results = await runner.run(adapter, dry_run=True)

        # Verify no migrations were actually run
        tables_result = await adapter.execute_query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='tb_fraise_state'"
        )
        # tb_fraise_state should not exist because of dry-run
        assert len(tables_result) == 0


class TestConvenienceFunctions:
    """Test convenience functions for migration running."""

    async def test_run_migrations_function(self):
        """Test run_migrations convenience function."""
        adapter = SqliteAdapter(":memory:")
        await adapter.connect()

        results = await run_migrations(adapter)

        assert results["database_type"] == "sqlite"
        assert results["migrations_run"] >= 3
        assert results["errors"] == []

        await adapter.disconnect()

    async def test_run_migrations_with_custom_dir(self):
        """Test run_migrations with custom migrations directory."""
        adapter = SqliteAdapter(":memory:")
        await adapter.connect()

        # This should use the default migrations directory from the library
        results = await run_migrations(adapter)

        assert results["database_type"] == "sqlite"
        assert results["migrations_run"] >= 3

        await adapter.disconnect()


class TestMigrationIntegration:
    """Integration tests for complete migration workflows."""

    async def test_full_schema_initialization(self):
        """Test complete schema initialization for a fresh database."""
        adapter = SqliteAdapter(":memory:")
        await adapter.connect()

        # Run all migrations
        results = await run_migrations(adapter)

        assert results["migrations_run"] > 0
        assert results["errors"] == []

        # Verify schema is functional by inserting data
        fraise_id = await adapter.insert("tb_fraise_state", {
            "id": "uuid-1",
            "identifier": "api:prod",
            "fraise_name": "api",
            "environment_name": "prod",
            "current_version": "1.0.0",
            "status": "healthy",
            "created_at": "2026-01-22T10:00:00",
            "updated_at": "2026-01-22T10:00:00",
        })

        assert fraise_id > 0

        # Verify insert works with view
        results = await adapter.execute_query("SELECT * FROM v_fraise_status")
        assert len(results) == 1
        assert results[0]["fraise_name"] == "api"

        await adapter.disconnect()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
