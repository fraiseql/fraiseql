#!/usr/bin/env python3
"""Simple migration runner for the blog example."""

import asyncio
import os
from pathlib import Path

import asyncpg


async def run_migrations():
    """Run all SQL migrations in order."""
    database_url = os.getenv("DATABASE_URL", "postgresql://fraiseql:fraiseql@localhost:5432/blog_demo")

    print(f"Connecting to database: {database_url}")

    # Connect to database
    conn = await asyncpg.connect(database_url)

    try:
        # Get migration files
        migrations_dir = Path(__file__).parent
        migration_files = sorted(migrations_dir.glob("*.sql"))

        print(f"\nFound {len(migration_files)} migration files:")
        for mig_file in migration_files:
            print(f"  - {mig_file.name}")

        # Run each migration
        for mig_file in migration_files:
            print(f"\nRunning migration: {mig_file.name}")
            sql = mig_file.read_text()

            try:
                await conn.execute(sql)
                print(f"  ✓ {mig_file.name} completed successfully")
            except asyncpg.exceptions.DuplicateObjectError as e:
                print(f"  ⚠ {mig_file.name} already applied (skipping)")
            except Exception as e:
                print(f"  ✗ {mig_file.name} failed: {e}")
                raise

        print("\n✓ All migrations completed successfully!")

    finally:
        await conn.close()


if __name__ == "__main__":
    asyncio.run(run_migrations())
