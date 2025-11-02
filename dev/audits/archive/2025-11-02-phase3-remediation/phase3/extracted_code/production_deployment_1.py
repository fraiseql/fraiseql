# Extracted from: docs/production/deployment.md
# Block number: 1
# migrations/run_migrations.py
import asyncio
import sys

from alembic import command
from alembic.config import Config


async def run_migrations():
    """Run database migrations before deployment."""
    alembic_cfg = Config("alembic.ini")

    try:
        # Check current version
        command.current(alembic_cfg)

        # Run migrations
        command.upgrade(alembic_cfg, "head")

        print("✓ Migrations completed successfully")
        return 0

    except Exception as e:
        print(f"✗ Migration failed: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(asyncio.run(run_migrations()))
