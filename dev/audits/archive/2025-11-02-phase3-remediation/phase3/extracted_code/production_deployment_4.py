# Extracted from: docs/production/deployment.md
# Block number: 4
# migrations/rollback.py
from alembic import command
from alembic.config import Config


def rollback_migration(steps: int = 1):
    """Rollback database migrations."""
    alembic_cfg = Config("alembic.ini")
    command.downgrade(alembic_cfg, f"-{steps}")
    print(f"✓ Rolled back {steps} migration(s)")


# Rollback one migration
rollback_migration(1)
