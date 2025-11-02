# Extracted from: docs/advanced/database-patterns.md
# Block number: 4
@fraise_type
class MutationResultBase:
    """Standardized result for all mutations."""

    status: str
    id: UUID | None = None
    updated_fields: list[str] | None = None
    message: str | None = None
    errors: list[dict[str, Any]] | None = None


@fraise_type
class MutationLogResult:
    """Detailed mutation result with change tracking."""

    status: str
    message: str | None = None
    reason: str | None = None
    op: str | None = None  # insert, update, delete
    entity: str | None = None
    extra_metadata: dict[str, Any] | None = None
    payload_before: dict[str, Any] | None = None
    payload_after: dict[str, Any] | None = None
