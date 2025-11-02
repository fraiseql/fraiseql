# Extracted from: docs/getting-started/first-hour.md
# Block number: 5
# app.py
@fraiseql.success
class DeleteNoteSuccess:
    """Successful deletion response."""

    message: str = "Note deleted successfully"


@fraiseql.failure
class DeleteNoteError:
    """Deletion error response."""

    message: str
    code: str = "NOT_FOUND"


@fraiseql.mutation
async def delete_note(info, id: UUID) -> DeleteNoteSuccess | DeleteNoteError:
    """Delete a note by ID with detailed error handling."""
    db = info.context["db"]
    # Call function that returns JSONB directly from database
    # FraiseQL automatically maps JSONB to the appropriate type
    result = await db.fetchval("SELECT fn_delete_note($1)", id)

    # Return the appropriate type based on success field
    if result.get("success"):
        return DeleteNoteSuccess(message=result["message"])
    return DeleteNoteError(message=result["message"], code=result.get("code", "UNKNOWN_ERROR"))
