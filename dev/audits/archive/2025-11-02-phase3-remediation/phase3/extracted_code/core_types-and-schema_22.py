# Extracted from: docs/core/types-and-schema.md
# Block number: 22
from fraiseql import input
from fraiseql.types import UNSET


@input
class UpdateUserInput:
    id: UUID
    name: str | None = UNSET  # Not provided by default
    email: str | None = UNSET
    bio: str | None = UNSET
