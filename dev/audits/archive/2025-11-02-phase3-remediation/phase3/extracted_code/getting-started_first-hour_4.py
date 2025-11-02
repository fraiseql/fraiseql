# Extracted from: docs/getting-started/first-hour.md
# Block number: 4
# app.py
@fraiseql.mutation
async def delete_note(info, id: UUID) -> bool:
    """Delete a note by ID (returns true if deleted, false if not found)."""
    db = info.context["db"]
    return await db.fetchval("SELECT fn_delete_note($1)", id)
