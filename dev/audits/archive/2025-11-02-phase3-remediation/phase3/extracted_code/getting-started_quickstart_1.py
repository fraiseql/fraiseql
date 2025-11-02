# Extracted from: docs/getting-started/quickstart.md
# Block number: 1
import uuid
from datetime import datetime

import uvicorn

from fraiseql import failure, input, mutation, query, success, type
from fraiseql.fastapi import create_fraiseql_app


# Define GraphQL types
@type(sql_source="v_note", jsonb_column="data")
class Note:
    """A simple note with title and content."""

    id: uuid.UUID
    title: str
    content: str | None
    created_at: datetime


# Define input types
@input
class CreateNoteInput:
    """Input for creating a new note."""

    title: str
    content: str | None = None


# Define success/failure types
@success
class CreateNoteSuccess:
    """Success response for note creation."""

    note: Note
    message: str = "Note created successfully"


@failure
class ValidationError:
    """Validation error."""

    message: str
    code: str = "VALIDATION_ERROR"


# Queries
@query
async def notes(info) -> list[Note]:
    """Get all notes."""
    db = info.context["db"]
    from fraiseql.db import DatabaseQuery

    query = DatabaseQuery(
        "SELECT data FROM v_note ORDER BY (data->>'created_at')::timestamp DESC", []
    )
    result = await db.run(query)
    return [Note(**row["data"]) for row in result]


@query
async def note(info, id: uuid.UUID) -> Note | None:
    """Get a single note by ID."""
    db = info.context["db"]
    from fraiseql.db import DatabaseQuery

    query = DatabaseQuery("SELECT data FROM v_note WHERE (data->>'id')::uuid = %s", [id])
    result = await db.run(query)
    if result:
        return Note(**result[0]["data"])
    return None


# Mutations
@mutation
class CreateNote:
    """Create a new note."""

    input: CreateNoteInput
    success: CreateNoteSuccess
    failure: ValidationError

    async def resolve(self, info) -> CreateNoteSuccess | ValidationError:
        db = info.context["db"]

        try:
            note_data = {"title": self.input.title}
            if self.input.content is not None:
                note_data["content"] = self.input.content

            result = await db.insert("tb_note", note_data, returning="id")

            # Get the created note from the view
            from fraiseql.db import DatabaseQuery

            query = DatabaseQuery(
                "SELECT data FROM v_note WHERE (data->>'id')::uuid = %s", [result["id"]]
            )
            note_result = await db.run(query)
            if note_result:
                created_note = Note(**note_result[0]["data"])
                return CreateNoteSuccess(note=created_note)
            return ValidationError(message="Failed to retrieve created note")

        except Exception as e:
            return ValidationError(message=f"Failed to create note: {e!s}")


# Create the app
QUICKSTART_TYPES = [Note]
QUICKSTART_QUERIES = [notes, note]
QUICKSTART_MUTATIONS = [CreateNote]

if __name__ == "__main__":
    import os

    # Database URL (override with DATABASE_URL environment variable)
    database_url = os.getenv("DATABASE_URL", "postgresql://localhost/quickstart_notes")

    app = create_fraiseql_app(
        database_url=database_url,
        types=QUICKSTART_TYPES,
        queries=QUICKSTART_QUERIES,
        mutations=QUICKSTART_MUTATIONS,
        title="Notes API",
        description="Simple note-taking GraphQL API",
        production=False,  # Enable GraphQL playground
    )

    print("🚀 Notes API running at http://localhost:8000/graphql")
    print("📖 GraphQL Playground: http://localhost:8000/graphql")

    uvicorn.run(app, host="0.0.0.0", port=8000)
