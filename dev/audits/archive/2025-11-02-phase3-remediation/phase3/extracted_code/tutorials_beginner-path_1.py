# Extracted from: docs/tutorials/beginner-path.md
# Block number: 1
from fraiseql import query, type


# Create a simple Note API
@type(sql_source="v_note")
class Note:
    id: UUID
    title: str
    content: str
    created_at: datetime


@query
def notes() -> list[Note]:
    """Get all notes."""
    # Implementation handled by framework
