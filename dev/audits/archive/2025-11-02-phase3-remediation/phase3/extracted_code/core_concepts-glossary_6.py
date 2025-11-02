# Extracted from: docs/core/concepts-glossary.md
# Block number: 6
import fraiseql


@fraiseql.type(sql_source="v_note")
class Note:
    """Simple note without slug."""

    id: int  # Can use simple int if no public API needed
    title: str
    content: str
