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
        elif "object_data" in row:
            # Legacy format with explicit object_data key (NOT flattened)
            status = row.get("status", "")
            message = row.get("message", "")
            object_data = row.get("object_data")
            extra_metadata = row.get("extra_metadata")
        else:
            # Flat/Flattened format: success type fields at top level
            # e.g., {id, message, _cascade} OR {status, message, machine: {...}, entity_id, ...}
            # Common with cascade mutations or flattened entity mutations
            status = row.get("status", "success")  # Use status if present, otherwise assume success
            message = row.get("message", "")

            # Don't extract _cascade - leave it in original result dict
            # for the resolver to access
            extra_metadata = row.get("extra_metadata") or row.get("metadata")

            # All other fields (except system fields) go into object_data
            # This allows the parser to extract them as success type fields
            system_fields = {
                "message",
                "_cascade",
                "status",
                "object_data",
                "extra_metadata",
                "metadata",
                "updated_fields",
                "entity_id",
                "entity_type",
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
    """Metadata about a GraphQL Cascade operation.

    Compliant with graphql-cascade specification v1:
    https://github.com/graphql-cascade/graphql-cascade

    Attributes:
        timestamp: Server timestamp when mutation executed (ISO 8601 format).
        affected_count: Total number of entities affected by the cascade.
        depth: Maximum relationship depth traversed during cascade.
        transaction_id: PostgreSQL transaction ID for debugging and correlation.
            Use txid_current()::text in your PostgreSQL function.
    """

    timestamp: str
    affected_count: int
    depth: int  # Required per spec
    transaction_id: str | None = None  # Optional per spec


@fraiseql_type
class Cascade:
    """Complete cascade response with side effects."""

    updated: List[CascadeEntity]  # List of updated entities
    deleted: List[CascadeEntity]  # List of deleted entities
    invalidations: List[CascadeInvalidation]
    metadata: CascadeMetadata


@dataclass
class MutationError:
    """Error response for mutations.

    Attributes:
        code: Application-level error code (422, 404, 409, 500, etc.)
              This is NOT an HTTP status code. HTTP is always 200 OK.
              The code field provides REST-like semantics for DX.
        status: Domain-specific status string (e.g., "noop:invalid_contract_id")
        message: Human-readable error message
        cascade: Optional cascade metadata (if enable_cascade=True)
        errors: Optional detailed error list (legacy compatibility)
    """

    code: int
    status: str
    message: str
    cascade: dict[str, Any] | None = None
    errors: list[dict[str, Any]] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {
            "code": self.code,
            "status": self.status,
            "message": self.message,
        }
        if self.cascade is not None:
            result["cascade"] = self.cascade
        if self.errors:
            result["errors"] = self.errors
        return result


@dataclass
class MutationSuccess:
    """Success response for mutations.

    Success type ALWAYS has non-null entity.
    If entity is None, the mutation should return MutationError instead.

    Attributes:
        entity: The created/updated/deleted entity (REQUIRED)
        cascade: Optional cascade metadata (if enable_cascade=True)
        message: Optional success message
        updated_fields: Optional list of updated field names
    """

    entity: Any  # REQUIRED - never None
    cascade: dict[str, Any] | None = None
    message: str | None = None
    updated_fields: list[str] | None = None

    def __post_init__(self):
        """Validate that entity is not None."""
        if self.entity is None:
            raise ValueError(
                "MutationSuccess requires non-null entity. "
                "For validation failures or errors, use MutationError instead."
            )

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {"entity": self.entity}
        if self.cascade is not None:
            result["cascade"] = self.cascade
        if self.message is not None:
            result["message"] = self.message
        if self.updated_fields is not None:
            result["updated_fields"] = self.updated_fields
        return result
