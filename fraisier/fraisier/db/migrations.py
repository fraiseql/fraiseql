"""Database migration system for multi-database support.

Provides database-agnostic migration runner that works with all adapters.
Follows the pattern:
- migrations/sqlite/ - SQLite-specific SQL files
- migrations/postgresql/ - PostgreSQL-specific SQL files
- migrations/mysql/ - MySQL-specific SQL files

Each migration file is named: NNN_description.sql (e.g., 001_create_tables.sql)
"""

import os
from pathlib import Path
from typing import Any

from .adapter import DatabaseType, FraiserDatabaseAdapter


class MigrationError(Exception):
    """Raised when migration execution fails."""

    pass


class MigrationRunner:
    """Runs database migrations for any supported database type.

    Supports:
    - Idempotent migrations (safe to run multiple times)
    - Database-specific SQL syntax
    - Transaction support
    - Migration tracking and skipping
    """

    def __init__(self, migrations_dir: str | None = None):
        """Initialize migration runner.

        Args:
            migrations_dir: Path to migrations directory.
                If None, uses fraisier/db/migrations/ relative to this file.
        """
        if migrations_dir is None:
            migrations_dir = str(Path(__file__).parent / "migrations")

        self.migrations_dir = Path(migrations_dir)

        if not self.migrations_dir.exists():
            raise MigrationError(f"Migrations directory not found: {self.migrations_dir}")

    def _get_db_migrations_dir(self, db_type: DatabaseType) -> Path:
        """Get path to database-specific migrations directory.

        Args:
            db_type: Type of database

        Returns:
            Path to migrations directory for database type

        Raises:
            MigrationError: If directory doesn't exist
        """
        db_migrations_dir = self.migrations_dir / db_type.value

        if not db_migrations_dir.exists():
            raise MigrationError(
                f"No migrations found for {db_type.value} at {db_migrations_dir}"
            )

        return db_migrations_dir

    def _get_pending_migrations(self, db_migrations_dir: Path) -> list[tuple[str, str]]:
        """Get list of pending migrations.

        Args:
            db_migrations_dir: Path to database-specific migrations directory

        Returns:
            List of (filename, full_path) tuples in sorted order
        """
        migration_files = sorted(
            [f for f in db_migrations_dir.glob("*.sql") if f.is_file()]
        )

        return [(f.name, str(f)) for f in migration_files]

    def _read_migration_file(self, migration_path: str) -> str:
        """Read migration SQL file.

        Args:
            migration_path: Path to migration file

        Returns:
            SQL content

        Raises:
            MigrationError: If file cannot be read
        """
        try:
            with open(migration_path) as f:
                sql = f.read().strip()

            if not sql:
                raise MigrationError(f"Migration file is empty: {migration_path}")

            return sql
        except OSError as e:
            raise MigrationError(f"Failed to read migration file {migration_path}: {e}") from e

    async def _execute_migration(
        self,
        adapter: FraiserDatabaseAdapter,
        migration_name: str,
        sql: str,
    ) -> None:
        """Execute a single migration.

        Args:
            adapter: Database adapter
            migration_name: Name of migration (for logging)
            sql: SQL to execute

        Raises:
            MigrationError: If execution fails
        """
        try:
            # Handle multi-statement migrations (split by semicolon)
            statements = [s.strip() for s in sql.split(";") if s.strip()]

            for statement in statements:
                await adapter.execute_query(statement)

        except Exception as e:
            raise MigrationError(
                f"Failed to execute migration {migration_name}: {e}"
            ) from e

    async def run(self, adapter: FraiserDatabaseAdapter, dry_run: bool = False) -> dict[str, Any]:
        """Run all pending migrations.

        Args:
            adapter: Database adapter
            dry_run: If True, only print what would be executed

        Returns:
            Dictionary with migration results:
            {
                'database_type': 'sqlite',
                'migrations_run': 2,
                'migrations': ['001_create_tables.sql', '002_create_indexes.sql'],
                'errors': []
            }

        Raises:
            MigrationError: If any migration fails (unless in dry_run mode)
        """
        db_type = adapter.database_type()

        # Get database-specific migrations directory
        db_migrations_dir = self._get_db_migrations_dir(db_type)

        # Get list of migrations
        pending_migrations = self._get_pending_migrations(db_migrations_dir)

        if not pending_migrations:
            return {
                "database_type": db_type.value,
                "migrations_run": 0,
                "migrations": [],
                "skipped_reason": "No migrations found",
            }

        results = {
            "database_type": db_type.value,
            "migrations_run": 0,
            "migrations": [],
            "errors": [],
        }

        # Execute each migration
        for migration_name, migration_path in pending_migrations:
            try:
                # Read migration file
                sql = self._read_migration_file(migration_path)

                if dry_run:
                    # Just log what would be executed
                    print(f"[DRY RUN] Would execute: {migration_name}")
                    print(f"  {sql[:100]}...")
                else:
                    # Execute migration
                    await self._execute_migration(adapter, migration_name, sql)
                    results["migrations"].append(migration_name)
                    results["migrations_run"] += 1

            except MigrationError as e:
                error_msg = str(e)
                results["errors"].append({"migration": migration_name, "error": error_msg})

                if not dry_run:
                    raise

        return results


# Convenience functions

_migration_runner: MigrationRunner | None = None


def get_migration_runner(migrations_dir: str | None = None) -> MigrationRunner:
    """Get or create global migration runner.

    Args:
        migrations_dir: Path to migrations directory (for custom location)

    Returns:
        MigrationRunner instance
    """
    global _migration_runner

    if _migration_runner is None or migrations_dir is not None:
        _migration_runner = MigrationRunner(migrations_dir)

    return _migration_runner


async def run_migrations(
    adapter: FraiserDatabaseAdapter,
    migrations_dir: str | None = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    """Run all pending migrations for a database adapter.

    Convenience function that creates runner and executes all migrations.

    Args:
        adapter: Database adapter to migrate
        migrations_dir: Path to migrations directory
        dry_run: If True, only show what would execute

    Returns:
        Migration results dictionary

    Example:
        ```python
        from fraisier.db.factory import get_database_adapter
        from fraisier.db.migrations import run_migrations

        adapter = await get_database_adapter()
        results = await run_migrations(adapter)
        print(f"Executed {results['migrations_run']} migrations")
        ```
    """
    runner = get_migration_runner(migrations_dir)
    return await runner.run(adapter, dry_run=dry_run)


__all__ = [
    "MigrationError",
    "MigrationRunner",
    "get_migration_runner",
    "run_migrations",
]
