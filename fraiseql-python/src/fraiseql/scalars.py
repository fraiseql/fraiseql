"""FraiseQL scalar type markers for schema authoring.

These are type markers used in Python type annotations to generate the correct
GraphQL scalar types in schema.json. They have no runtime behavior - validation
and serialization happen in the Rust runtime after compilation.

Architecture:
    Python type annotation → schema.json type string → Rust FieldType → codegen/introspection

Example:
    ```python
    import fraiseql
    from fraiseql.scalars import ID, DateTime, Json

    @fraiseql.type
    class User:
        id: ID                    # → "ID" in schema.json → FieldType::Id
        name: str                 # → "String"
        created_at: DateTime      # → "DateTime" → FieldType::DateTime
        metadata: Json | None     # → "Json" (nullable)
    ```

FraiseQL Convention:
    - `id` fields should ALWAYS use `ID` type (UUID v4 at runtime)
    - Foreign keys (e.g., `author_id`) should also use `ID`
"""

from typing import NewType

# =============================================================================
# Core GraphQL Scalars
# =============================================================================

ID = NewType("ID", str)
"""GraphQL ID scalar - used for unique identifiers.

FraiseQL enforces UUID v4 format for all ID fields at runtime.
This is the REQUIRED type for `id` fields and foreign key references.

Example:
    id: ID
    author_id: ID
"""

# =============================================================================
# Date/Time Scalars
# =============================================================================

DateTime = NewType("DateTime", str)
"""ISO 8601 DateTime scalar (e.g., "2025-01-10T12:00:00Z").

Maps to PostgreSQL `TIMESTAMPTZ` / `TIMESTAMP WITH TIME ZONE`.
"""

Date = NewType("Date", str)
"""ISO 8601 Date scalar (e.g., "2025-01-10").

Maps to PostgreSQL `DATE`.
"""

Time = NewType("Time", str)
"""ISO 8601 Time scalar (e.g., "12:00:00").

Maps to PostgreSQL `TIME`.
"""

# =============================================================================
# Complex Scalars
# =============================================================================

Json = NewType("Json", object)
"""Arbitrary JSON value scalar.

Maps to PostgreSQL `JSONB`. Accepts any valid JSON value.
"""

UUID = NewType("UUID", str)
"""UUID scalar (explicit UUID type, distinct from ID).

Use `ID` for entity identifiers. Use `UUID` only when you need
an explicit UUID field that is NOT an identifier.
"""

Decimal = NewType("Decimal", str)
"""Decimal/BigDecimal scalar for precise numeric values.

Serialized as string to preserve precision. Maps to PostgreSQL `NUMERIC`.
Use for monetary values or other precision-critical numbers.
"""

# =============================================================================
# Vector Scalars (pgvector)
# =============================================================================

Vector = NewType("Vector", list)
"""Vector scalar for pgvector embeddings.

Serialized as `[Float!]!` in GraphQL, stored as `vector(N)` in PostgreSQL.
Used for similarity search with pgvector extension.
"""

# =============================================================================
# All exports
# =============================================================================

__all__ = [
    "ID",
    "DateTime",
    "Date",
    "Time",
    "Json",
    "UUID",
    "Decimal",
    "Vector",
]
