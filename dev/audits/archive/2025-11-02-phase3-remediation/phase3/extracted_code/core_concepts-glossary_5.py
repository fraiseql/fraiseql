# Extracted from: docs/core/concepts-glossary.md
# Block number: 5
from uuid import UUID

import fraiseql


@fraiseql.type(sql_source="v_user")
class User:
    """User type with trinity identifiers."""

    id: UUID  # Public API identifier (always exposed)
    identifier: str  # Human-readable slug (SEO-friendly)
    name: str
    email: str
    # pk_user is NOT exposed (internal only)
