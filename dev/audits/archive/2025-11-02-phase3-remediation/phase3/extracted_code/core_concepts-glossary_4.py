# Extracted from: docs/core/concepts-glossary.md
# Block number: 4
from typing import Annotated
from uuid import UUID

import fraiseql


@fraiseql.type(sql_source="v_user")
class User:
    """User account model.

    Fields:
        created_at: Account creation timestamp
    """

    id: UUID  # Public API identifier (inline comment - highest priority)
    identifier: str  # Human-readable username
    name: Annotated[str, "User's full name"]  # Annotated type
    email: str
    created_at: datetime  # Uses docstring description
