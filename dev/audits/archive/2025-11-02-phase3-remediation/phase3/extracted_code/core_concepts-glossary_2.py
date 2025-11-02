# Extracted from: docs/core/concepts-glossary.md
# Block number: 2
from uuid import UUID

import fraiseql


@fraiseql.type(sql_source="v_post")
class Post:
    id: UUID  # Public API - stable forever
    identifier: str  # Human-readable - can change
    title: str
    content: str
    # pk_post NOT exposed - internal only
