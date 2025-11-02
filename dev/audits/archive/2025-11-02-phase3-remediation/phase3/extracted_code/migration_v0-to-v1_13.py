# Extracted from: docs/migration/v0-to-v1.md
# Block number: 13
from datetime import datetime

from fraiseql import type


@type
class User:
    created_at: datetime  # Not 'date'
    middle_name: str | None = None  # Explicit optional
