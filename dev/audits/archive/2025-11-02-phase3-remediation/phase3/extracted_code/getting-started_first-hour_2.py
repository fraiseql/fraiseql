# Extracted from: docs/getting-started/first-hour.md
# Block number: 2
# app.py
@fraiseql.type
class Note:
    id: UUID
    title: str
    content: str
    tags: list[str]  # Add this line
