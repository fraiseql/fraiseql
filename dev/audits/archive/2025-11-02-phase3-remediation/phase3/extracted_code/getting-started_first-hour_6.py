# Extracted from: docs/getting-started/first-hour.md
# Block number: 6
# app.py
@fraiseql.type(sql_source="v_note")
class Note:
    id: UUID
    title: str
    content: str
    tags: list[str]
    created_at: datetime  # Add this
    updated_at: datetime  # Add this
