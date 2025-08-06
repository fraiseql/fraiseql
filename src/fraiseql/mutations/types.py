"""Types for PostgreSQL function-based mutations."""

from dataclasses import dataclass
from typing import Any
from uuid import UUID


@dataclass
class MutationResult:
    """Standard result type returned by PostgreSQL mutation functions.

    This matches the PostgreSQL composite type:
    CREATE TYPE mutation_result AS (
        id UUID,
        updated_fields TEXT[],
        status TEXT,
        message TEXT,
        object_data JSONB,
        extra_metadata JSONB
    );
    """

    id: UUID | None = None
    updated_fields: list[str] | None = None
    status: str = ""
    message: str = ""
    object_data: dict[str, Any] | None = None
    extra_metadata: dict[str, Any] | None = None

    @classmethod
    def from_db_row(cls, row: dict[str, Any]) -> "MutationResult":
        """Create from database row result."""
        return cls(
            id=row.get("id"),
            updated_fields=row.get("updated_fields"),
            status=row.get("status", ""),
            message=row.get("message", ""),
            object_data=row.get("object_data"),
            extra_metadata=row.get("extra_metadata"),
        )
