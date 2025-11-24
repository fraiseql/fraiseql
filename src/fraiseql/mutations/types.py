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
        # Handle multiple formats:
        # 1. Legacy format: status, message, object_data
        # 2. New format: success, data, error
        # 3. Flat format (cascade): id, message, _cascade (top-level success fields)
        # 4. Wrapped format: Single key with function name wrapping the actual result

        # Check if this is a wrapped format (function_name: {actual_result})
        # This happens when SELECT * FROM function() returns a scalar JSONB
        if len(row) == 1:
            key = next(iter(row.keys()))
            value = row[key]
            # If the single value is a dict, unwrap it
            if isinstance(value, dict):
                row = value

        if "success" in row:
            # New format
            status = "success" if row.get("success") else "error"
            message = row.get("message", "")
            object_data = row.get("data")
            extra_metadata = row.get("extra_metadata", {})
            # Include _cascade in extra_metadata if present
            if "_cascade" in row:
                extra_metadata["_cascade"] = row["_cascade"]
        elif "status" in row or "object_data" in row:
            # Legacy format (explicit status or object_data key)
            status = row.get("status", "")
            message = row.get("message", "")
            object_data = row.get("object_data")
            extra_metadata = row.get("extra_metadata")
        else:
            # Flat format: success type fields at top level
            # e.g., {id, message, _cascade}
            # Common with cascade mutations returning success type directly
            status = "success"  # Assume success if we have flat fields
            message = row.get("message", "")

            # Don't extract _cascade - leave it in original result dict
            # for the resolver to access
            extra_metadata = None

            # All other fields (except system fields) go into object_data
            # This allows the parser to extract them as success type fields
            system_fields = {
                "message",
                "_cascade",
                "status",
                "object_data",
                "extra_metadata",
                "updated_fields",
            }
            object_data = {k: v for k, v in row.items() if k not in system_fields}

        return cls(
            id=row.get("id"),
            updated_fields=row.get("updated_fields"),
            status=status,
            message=message,
            object_data=object_data if object_data else None,
            extra_metadata=extra_metadata if extra_metadata else None,
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
