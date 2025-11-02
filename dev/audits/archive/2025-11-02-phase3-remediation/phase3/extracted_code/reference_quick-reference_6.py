# Extracted from: docs/reference/quick-reference.md
# Block number: 6
from uuid import UUID

from fraiseql import input, mutation


@input
class UpdateUserInput:
    name: str | None = None
    email: str | None = None


@mutation
def update_user(id: UUID, input: UpdateUserInput) -> User:
    """Update user."""
    # Framework calls fn_update_user
