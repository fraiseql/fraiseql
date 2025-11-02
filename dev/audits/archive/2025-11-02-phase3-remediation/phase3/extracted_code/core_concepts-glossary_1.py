# Extracted from: docs/core/concepts-glossary.md
# Block number: 1
from uuid import UUID

import fraiseql


@fraiseql.type(sql_source="v_user")
class User:
    """User with trinity identifiers."""

    id: UUID  # Public API identifier (stable, secure)
    identifier: str  # Human-readable slug (SEO-friendly)
    name: str
    email: str
    # Note: pk_user is NOT exposed in GraphQL type
