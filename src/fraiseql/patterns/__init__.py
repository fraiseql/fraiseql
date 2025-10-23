"""Advanced patterns for FraiseQL.

This module provides optional advanced patterns that can be layered on top
of the core FraiseQL Rust-first architecture.
"""

from fraiseql.patterns.trinity import (
    TrinityMixin,
    get_identifier_from_slug,
    get_pk_column_name,
    trinity_field,
)

__all__ = [
    "TrinityMixin",
    "get_identifier_from_slug",
    "get_pk_column_name",
    "trinity_field",
]
