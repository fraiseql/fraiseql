"""Types for PostgreSQL function-based mutations."""

from dataclasses import dataclass
from typing import Any, Dict, List
from uuid import UUID

from fraiseql.types import type as fraiseql_type


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
        # Handle both formats:
        # 1. Legacy format: status, message, object_data
        # 2. New format: success, data, error

        if "success" in row:
            # New format
            status = "success" if row.get("success") else "error"
            message = row.get("message", "")
            object_data = row.get("data")
            extra_metadata = row.get("extra_metadata", {})
            # Include _cascade in extra_metadata if present
            if "_cascade" in row:
                extra_metadata["_cascade"] = row["_cascade"]
        else:
            # Legacy format
            status = row.get("status", "")
            message = row.get("message", "")
            object_data = row.get("object_data")
            extra_metadata = row.get("extra_metadata")

        return cls(
            id=row.get("id"),
            updated_fields=row.get("updated_fields"),
            status=status,
            message=message,
            object_data=object_data,
            extra_metadata=extra_metadata,
        )


# Cascade types for GraphQL schema
@fraiseql_type
class CascadeEntity:
    """Represents an entity affected by the mutation."""

    __typename: str
    id: str
    operation: str
    entity: Dict[str, Any]


@fraiseql_type
class CascadeInvalidation:
    """Cache invalidation instruction."""

    query_name: str
    strategy: str
    scope: str


@fraiseql_type
class CascadeMetadata:
    """Metadata about the cascade operation."""

    timestamp: str
    affected_count: int


@fraiseql_type
class Cascade:
    """Complete cascade response with side effects."""

    updated: List[CascadeEntity]
    deleted: List[str]
    invalidations: List[CascadeInvalidation]
    metadata: CascadeMetadata
