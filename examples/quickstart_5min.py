"""

5-Minute Quickstart Example

This is the complete code from the 5-minute quickstart guide.

Setup instructions:

1. Install FraiseQL: pip install fraiseql

2. Create database: createdb quickstart_notes

3. Run schema: psql quickstart_notes < quickstart_5min_schema.sql

4. Run this file: python quickstart_5min.py

Database Schema:

```sql

-- Simple notes table

CREATE TABLE tb_note (

    id SERIAL PRIMARY KEY,

    title VARCHAR(200) NOT NULL,

    content TEXT,

    created_at TIMESTAMP DEFAULT NOW()

);

-- Notes view for GraphQL queries

CREATE VIEW v_note AS

SELECT

    id,

    jsonb_build_object(

        'id', id,

        'title', title,

        'content', content,

        'created_at', created_at

    ) AS data

FROM tb_note;

-- Sample data

INSERT INTO tb_note (title, content) VALUES

    ('Welcome to FraiseQL', 'This is your first note!'),

    ('GraphQL is awesome', 'Queries and mutations made simple'),

    ('Database-first design', 'Views compose data for optimal performance');

```

"""

from datetime import datetime

import uvicorn

import fraiseql
from fraiseql.fastapi import create_fraiseql_app


# Define GraphQL types
@fraiseql.type(sql_source="v_note", jsonb_column="data")
class Note:
    """A simple note with title and content."""

    id: int
    title: str
    content: str | None
    created_at: datetime


# Define input types
@fraiseql.input
class CreateNoteInput:
    """Input for creating a new note."""

    title: str
    content: str | None = None


# Define success/failure types
@fraiseql.success
class CreateNoteSuccess:
    """Success response for note creation."""

    note: Note
    message: str = "Note created successfully"


@fraiseql.failure
class ValidationError:
    """Validation error."""

    message: str
    code: str = "VALIDATION_ERROR"


# Queries
@fraiseql.query
async def notes(info) -> list[Note]:
    """Get all notes."""
    db = info.context["db"]
    from fraiseql.db import DatabaseQuery

    query = DatabaseQuery(
        "SELECT data FROM v_note ORDER BY (data->>'created_at')::timestamp DESC", []
    )
    result = await db.run(query)
    return [Note(**row["data"]) for row in result]


@fraiseql.query
async def notes_filtered(info, title_contains: str | None = None) -> list[Note]:
    """Get notes with optional title filtering."""
    db = info.context["db"]
    from fraiseql.db import DatabaseQuery

    if title_contains:
        query = DatabaseQuery(
            "SELECT data FROM v_note WHERE data->>'title' ILIKE %s ORDER BY (data->>'created_at')::timestamp DESC",
            [f"%{title_contains}%"],
        )
    else:
        query = DatabaseQuery(
            "SELECT data FROM v_note ORDER BY (data->>'created_at')::timestamp DESC", []
        )

    result = await db.run(query)
    return [Note(**row["data"]) for row in result]


@fraiseql.query
async def note(info, id: int) -> Note | None:
    """Get a single note by ID."""
    db = info.context["db"]
    from fraiseql.db import DatabaseQuery

    query = DatabaseQuery("SELECT data FROM v_note WHERE (data->>'id')::int = %s", [id])
    result = await db.run(query)
    if result:
        return Note(**result[0]["data"])
    return None


# Mutations
@fraiseql.mutation
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
                "SELECT data FROM v_note WHERE (data->>'id')::int = %s", [result["id"]]
            )
            note_result = await db.run(query)
            if note_result:
                created_note = Note(**note_result[0]["data"])
                return CreateNoteSuccess(note=created_note)
            else:
                return ValidationError(message="Failed to retrieve created note")

        except Exception as e:
            return ValidationError(message=f"Failed to create note: {e!s}")


# Collect types, queries, and mutations for app creation
QUICKSTART_TYPES = [Note]
QUICKSTART_QUERIES = [notes, notes_filtered, note]
QUICKSTART_MUTATIONS = [CreateNote]


# Create and run the app
if __name__ == "__main__":
    import os

    # Allow database URL to be overridden via environment variable
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
