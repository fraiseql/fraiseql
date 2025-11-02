# Extracted from: docs/core/project-structure.md
# Block number: 1
# src/types/user.py
from fraiseql import fraise_field, type
from fraiseql.types.scalars import UUID


@type
class User:
    """A user in the system."""

    id: UUID = fraise_field(description="User ID")
    username: str = fraise_field(description="Unique username")
    email: str = fraise_field(description="Email address")
    created_at: str = fraise_field(description="Account creation date")
