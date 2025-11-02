# Extracted from: docs/getting-started/first-hour.md
# Block number: 3
# app.py
# Generate automatic Where input type for Note
NoteWhereInput = create_graphql_where_input(Note)


@fraiseql.query
async def notes(info, where: NoteWhereInput | None = None) -> list[Note]:
    """Get notes with optional filtering."""
    db = info.context["db"]
    # Use repository's find method with where parameter
    return await db.find("v_note", where=where)
