# Extracted from: docs/reference/quick-reference.md
# Block number: 7
from uuid import UUID

from fraiseql import mutation


class DeleteResult:
    success: bool
    error: str | None


@mutation
def delete_user(id: UUID) -> DeleteResult:
    """Delete user."""
    # Framework calls fn_delete_user
