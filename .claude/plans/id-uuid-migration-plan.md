# FraiseQL ID Migration Plan - Opinionated `ID = UUID` Convention

**Date**: 2025-12-30
**Target**: FraiseQL v1.10.0 - v2.0.0
**Status**: Planning Phase
**Philosophy**: Opinionated defaults with escape hatches

---

## Executive Summary

**Goal**: Migrate FraiseQL from using `UUID` type annotations to `ID` type annotations, establishing the opinionated convention that `ID = UUID` in PostgreSQL.

**Benefits**:
- GraphQL standard compliance (`id: ID!` instead of `id: UUID!`)
- Better DX (matches Strawberry, Graphene, Apollo)
- Future-proof (can change UUID implementation later)
- Industry alignment (Stripe, GitHub, Hasura)

**Timeline**: 3 releases over ~6 months
- v1.10.0: Export `ID`, keep `UUID` default (non-breaking)
- v1.11.0-v1.x: Encourage migration, document transition
- v2.0.0: Make `ID` the default (breaking change)

---

## Phase 0: Immediate Documentation Fix (v1.9.0)

**Status**: Ready to implement
**Timeline**: This session (1 hour)
**Breaking**: No

### Objective

Fix the current documentation inconsistency where examples use `from uuid import UUID` (Python stdlib) instead of FraiseQL's types.

### Changes

**Update all documentation to use**:
```python
from fraiseql.types import UUID  # Shorter, already exported
```

**Files to update**: ~75 docs files, 150+ code blocks

### Implementation

```bash
# Automated search & replace
find docs/ -name "*.md" -exec sed -i \
  's/from uuid import UUID$/from fraiseql.types import UUID/g' {} +

# Handle mixed imports (uuid4, etc.)
# Manual review needed for these cases
grep -r "from uuid import.*UUID" docs/
```

### Verification

```bash
# Should return 0
grep -r "from uuid import UUID" docs/ | wc -l

# Should return ~150
grep -r "from fraiseql.types import UUID" docs/ | wc -l

# Run doc examples (spot check)
python -m pytest docs/examples/ --doctest-modules
```

### Commit

```bash
git add docs/
git commit -m "fix(docs): use fraiseql.types.UUID instead of stdlib uuid.UUID

- Replace 'from uuid import UUID' with 'from fraiseql.types import UUID'
- Affects ~75 files, 150+ code blocks
- Preparation for future ID type migration
- No functional changes, purely import consistency
"
```

---

## Phase 1: Export ID Type (v1.10.0)

**Status**: Ready to implement
**Timeline**: Next minor release (~2 weeks)
**Breaking**: No (additive only)

### Objective

Make `ID` type available for import from `fraiseql.types` while maintaining full backward compatibility with `UUID`.

### Changes

#### 1.1: Export ID from fraiseql.types

**File**: `src/fraiseql/types/__init__.py`

```python
# Add import
from .scalars.id_scalar import ID

# Add to __all__
__all__ = [
    "CIDR",
    "CUSIP",
    # ... existing exports ...
    "ID",      # ← NEW: GraphQL ID type
    "UUID",    # ← KEEP: Explicit UUID type
    # ... rest of exports ...
]
```

**Test**:
```python
# tests/types/test_id_export.py
def test_id_importable_from_types():
    from fraiseql.types import ID
    assert ID is not None

def test_uuid_still_importable():
    from fraiseql.types import UUID
    assert UUID is not None
```

#### 1.2: Create IDScalar GraphQL Type

**File**: `src/fraiseql/types/scalars/id_scalar.py`

```python
"""GraphQL ID scalar backed by UUID, used for opaque identifier representation."""

from __future__ import annotations

import uuid
from typing import Any

from graphql import GraphQLScalarType

# Reuse UUID serialization (since ID = UUID in PostgreSQL)
from .uuid import serialize_uuid, parse_uuid_value, parse_uuid_literal


# GraphQL Scalar (for schema generation)
IDScalar = GraphQLScalarType(
    name="ID",
    description="A globally unique identifier (UUID format).",
    serialize=serialize_uuid,
    parse_value=parse_uuid_value,
    parse_literal=parse_uuid_literal,
)


# Python Type Marker (for type annotations)
class ID:
    """A GraphQL-safe identifier backed internally by UUID.

    This type represents the opinionated convention that all IDs in FraiseQL
    are UUIDs in PostgreSQL, but exposed as GraphQL's standard ID type.

    Examples:
        from fraiseql.types import ID

        @fraiseql.type
        class User:
            id: ID  # Maps to UUID in PostgreSQL, ID in GraphQL

        @fraiseql.type
        class Post:
            id: ID
            author_id: ID  # Foreign key

    PostgreSQL mapping:
        CREATE TABLE tb_user (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid()
        );

    GraphQL schema:
        type User {
          id: ID!
        }
    """

    __slots__ = ("_value",)

    def __init__(self, value: Any) -> None:
        """Initialize an ID instance from a UUID or a valid UUID string."""
        if isinstance(value, uuid.UUID):
            self._value = value
        elif isinstance(value, str):
            try:
                self._value = uuid.UUID(value)
            except ValueError as exc:
                msg = f"Invalid UUID string: {value}"
                raise TypeError(msg) from exc
        else:
            msg = f"ID must be initialized with a UUID or str, not {type(value).__name__}"
            raise TypeError(msg)

    @classmethod
    def coerce(cls, value: object) -> ID:
        """Coerce a UUID, str, or ID into an ID instance."""
        if isinstance(value, ID):
            return value
        if isinstance(value, uuid.UUID):
            return cls(value)
        if isinstance(value, str):
            return cls(value)
        msg = f"Cannot coerce {type(value).__name__} to ID"
        raise TypeError(msg)

    def __str__(self) -> str:
        """Return the string representation of the UUID."""
        return str(self._value)

    def __repr__(self) -> str:
        """Return the debug representation of the ID."""
        return f"ID('{self._value}')"

    def __eq__(self, other: object) -> bool:
        """Check equality with another ID or UUID."""
        if isinstance(other, ID):
            return self._value == other._value
        if isinstance(other, uuid.UUID):
            return self._value == other
        return NotImplemented

    def __hash__(self) -> int:
        """Return a hash based on the underlying UUID."""
        return hash(self._value)

    @property
    def uuid(self) -> uuid.UUID:
        """Access the underlying UUID value."""
        return self._value
```

**Test**:
```python
# tests/types/scalars/test_id_scalar.py
import uuid
from fraiseql.types.scalars import IDScalar
from fraiseql.types import ID

def test_id_scalar_serialize():
    test_uuid = uuid.uuid4()
    assert IDScalar.serialize(test_uuid) == str(test_uuid)
    assert IDScalar.serialize(str(test_uuid)) == str(test_uuid)

def test_id_scalar_parse():
    test_uuid_str = "550e8400-e29b-41d4-a716-446655440000"
    parsed = IDScalar.parse_value(test_uuid_str)
    assert isinstance(parsed, uuid.UUID)
    assert str(parsed) == test_uuid_str

def test_id_class_initialization():
    test_uuid = uuid.uuid4()

    # From UUID
    id1 = ID(test_uuid)
    assert id1.uuid == test_uuid

    # From string
    id2 = ID(str(test_uuid))
    assert id2.uuid == test_uuid

    # Equality
    assert id1 == id2

def test_id_vs_uuid_type_compatibility():
    """Ensure ID and UUID types can be used interchangeably in DB operations."""
    from fraiseql.types import UUID

    test_uuid = uuid.uuid4()
    id_instance = ID(test_uuid)

    # Both should serialize the same
    assert IDScalar.serialize(id_instance) == str(test_uuid)
    # UUID type should work the same way (we're just adding ID as an alias)
```

#### 1.3: Export IDScalar from scalars package

**File**: `src/fraiseql/types/scalars/__init__.py`

```python
# Add import
from .id_scalar import IDScalar

# Add to __all__
__all__ = [
    "AirportCodeScalar",
    # ... existing scalars ...
    "IDScalar",    # ← NEW
    "UUIDScalar",  # ← KEEP
    # ... rest of scalars ...
]
```

#### 1.4: Register ID scalar in schema builder

**File**: `src/fraiseql/gql/schema_builder.py`

Find where scalars are registered and add:

```python
from fraiseql.types.scalars import (
    DateScalar,
    DateTimeScalar,
    IDScalar,      # ← NEW
    JSONScalar,
    UUIDScalar,    # ← KEEP
    # ... other scalars
)

# In the scalar registration logic
BUILTIN_SCALARS = {
    # ... existing scalars ...
    "ID": IDScalar,      # ← NEW
    "UUID": UUIDScalar,  # ← KEEP
    # ... rest of scalars ...
}
```

**Test**:
```python
# tests/gql/test_id_in_schema.py
import fraiseql
from fraiseql.types import ID

def test_id_type_in_schema():
    @fraiseql.type
    class User:
        id: ID
        name: str

    schema = fraiseql.build_fraiseql_schema(
        query_type=User,
        types=[User]
    )

    # Check that ID scalar is registered
    assert "ID" in schema.type_map
    assert schema.type_map["ID"].name == "ID"

    # Check that User.id field uses ID type
    user_type = schema.type_map["User"]
    assert user_type.fields["id"].type.name == "ID"
```

#### 1.5: Update introspection to handle ID type

**File**: `src/fraiseql/introspection/type_mapper.py`

```python
# Find the type mapping logic and ensure ID maps to UUID in PostgreSQL

def map_graphql_type_to_postgresql(graphql_type: str) -> str:
    """Map GraphQL type to PostgreSQL type."""
    type_mapping = {
        "String": "TEXT",
        "Int": "INTEGER",
        "Float": "DOUBLE PRECISION",
        "Boolean": "BOOLEAN",
        "ID": "UUID",      # ← NEW: ID maps to UUID
        "UUID": "UUID",    # ← KEEP: UUID maps to UUID
        # ... rest of mappings ...
    }
    return type_mapping.get(graphql_type, "TEXT")
```

**Test**:
```python
# tests/introspection/test_id_type_mapping.py
from fraiseql.introspection.type_mapper import map_graphql_type_to_postgresql

def test_id_maps_to_uuid():
    assert map_graphql_type_to_postgresql("ID") == "UUID"

def test_uuid_still_maps_to_uuid():
    assert map_graphql_type_to_postgresql("UUID") == "UUID"
```

### Documentation Updates (v1.10.0)

#### 1.6: Add ID type to documentation

**New file**: `docs/core/id-type.md`

```markdown
# ID Type - FraiseQL's Opinionated Identifier Convention

> **TL;DR**: Use `ID` for all entity identifiers. FraiseQL maps `ID` to `UUID` in PostgreSQL.

## Quick Start

\`\`\`python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID  # GraphQL sees "ID", PostgreSQL has UUID
    name: str

@fraiseql.type
class Post:
    id: ID          # Primary key
    author_id: ID   # Foreign key
    title: str
\`\`\`

## Why ID instead of UUID?

**GraphQL Standard**: Every major GraphQL framework uses `ID` for identifiers:
- Strawberry: `strawberry.ID`
- Graphene: `graphene.ID()`
- Apollo: `GraphQLID`

**Opaque Identifiers**: GraphQL clients should treat IDs as opaque (don't parse/manipulate)

**Future-Proof**: If FraiseQL later supports ULID, Snowflake, etc., no client changes needed

## PostgreSQL Mapping

FraiseQL makes the opinionated choice that `ID = UUID`:

\`\`\`sql
-- Python type annotation
id: ID

-- Maps to PostgreSQL
id UUID PRIMARY KEY DEFAULT gen_random_uuid()

-- GraphQL schema
id: ID!
\`\`\`

## When to use ID vs UUID

### Use ID (99% of cases)

\`\`\`python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID          # ✅ Primary key

@fraiseql.type
class Post:
    id: ID          # ✅ Primary key
    author_id: ID   # ✅ Foreign key
\`\`\`

### Use UUID explicitly (1% of cases)

\`\`\`python
from fraiseql.types import ID, UUID

@fraiseql.type
class AuditLog:
    id: ID                      # ✅ Standard identifier
    user_id: ID                 # ✅ Foreign key
    correlation_id: UUID        # ✅ Distributed tracing (UUID matters)
    request_id: UUID            # ✅ Compliance requirement
    idempotency_key: UUID       # ✅ Client-generated UUID
\`\`\`

**Use `UUID` explicitly when**:
- Compliance/audit requires documenting as UUID
- Client generates the UUID
- UUID format itself has meaning (correlation, tracing)
- Cross-service integration where UUID is in the contract

## Migration from UUID

\`\`\`python
# OLD (still supported)
from fraiseql.types import UUID

@fraiseql.type
class User:
    id: UUID

# NEW (recommended)
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
\`\`\`

**No database changes needed** - both map to PostgreSQL `UUID`.

## See Also

- [Types and Schema](./types-and-schema.md)
- [Database Patterns](../advanced/database-patterns.md)
- [Migration Guide](../migration/uuid-to-id.md)
\`\`\`
```

#### 1.7: Update existing docs to mention ID

**File**: `docs/core/types-and-schema.md`

Add section:

```markdown
## Identifier Types

FraiseQL provides two types for entity identifiers:

### ID (Recommended)

\`\`\`python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID  # Opaque identifier (UUID in PostgreSQL)
\`\`\`

- GraphQL standard type
- Opaque to clients
- Maps to UUID in PostgreSQL
- **Recommended for all entity identifiers**

### UUID (Explicit)

\`\`\`python
from fraiseql.types import UUID

@fraiseql.type
class AuditLog:
    correlation_id: UUID  # Explicit UUID requirement
\`\`\`

- Explicit UUID type
- Use when UUID format matters
- Compliance, tracing, client-generated

**Rule of thumb**: Use `ID` unless you have a specific reason to use `UUID`.
```

#### 1.8: Update API reference

**File**: `docs/reference/types-api.md`

Add:

```markdown
## ID Type

\`\`\`python
from fraiseql.types import ID
\`\`\`

**Description**: Opaque identifier type, maps to UUID in PostgreSQL.

**PostgreSQL**: `UUID`
**GraphQL**: `ID!`

**Example**:
\`\`\`python
@fraiseql.type
class User:
    id: ID
    email: str
\`\`\`

**See**: [ID Type Guide](../core/id-type.md)
```

### Testing (v1.10.0)

**New test file**: `tests/integration/test_id_type_end_to_end.py`

```python
"""Integration tests for ID type."""
import uuid
import pytest
import fraiseql
from fraiseql.types import ID


@pytest.fixture
async def schema_with_id_type(db_pool):
    """Create schema with ID type."""

    @fraiseql.type
    class User:
        id: ID
        name: str
        email: str

    @fraiseql.query
    class Query:
        users: list[User] = fraiseql.field()

    schema = fraiseql.build_fraiseql_schema(query_type=Query)

    # Create PostgreSQL table
    async with db_pool.connection() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS tb_user (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE
            )
        """)
        await conn.execute("TRUNCATE tb_user")

        # Insert test data
        await conn.execute("""
            INSERT INTO tb_user (name, email) VALUES
            ('Alice', 'alice@example.com'),
            ('Bob', 'bob@example.com')
        """)

    yield schema

    # Cleanup
    async with db_pool.connection() as conn:
        await conn.execute("DROP TABLE IF EXISTS tb_user")


async def test_id_type_in_graphql_query(schema_with_id_type):
    """Test that ID type works in GraphQL queries."""
    query = """
        query {
            users {
                id
                name
                email
            }
        }
    """

    result = await schema_with_id_type.execute(query)

    assert result.errors is None
    assert len(result.data["users"]) == 2

    # Check that IDs are valid UUIDs
    for user in result.data["users"]:
        user_id = user["id"]
        assert isinstance(user_id, str)
        # Should be parseable as UUID
        uuid.UUID(user_id)


async def test_id_type_in_graphql_argument(schema_with_id_type, db_pool):
    """Test that ID type works as query argument."""

    # Get a user ID first
    async with db_pool.connection() as conn:
        row = await conn.fetchrow("SELECT id FROM tb_user LIMIT 1")
        user_id = str(row["id"])

    @fraiseql.query
    class QueryWithArg:
        @fraiseql.field
        async def user(id: ID) -> User:
            async with db_pool.connection() as conn:
                row = await conn.fetchrow(
                    "SELECT id, name, email FROM tb_user WHERE id = $1",
                    uuid.UUID(str(id))
                )
                if not row:
                    return None
                return User(
                    id=ID(row["id"]),
                    name=row["name"],
                    email=row["email"]
                )

    schema = fraiseql.build_fraiseql_schema(query_type=QueryWithArg)

    query = f"""
        query {{
            user(id: "{user_id}") {{
                id
                name
            }}
        }}
    """

    result = await schema.execute(query)

    assert result.errors is None
    assert result.data["user"]["id"] == user_id


async def test_id_and_uuid_interoperability(db_pool):
    """Test that ID and UUID types are interoperable."""
    from fraiseql.types import UUID

    # Create two types - one with ID, one with UUID
    @fraiseql.type
    class UserWithID:
        id: ID
        name: str

    @fraiseql.type
    class UserWithUUID:
        id: UUID
        name: str

    # Both should work with the same PostgreSQL table
    async with db_pool.connection() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS tb_user_test (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL
            )
        """)

        row = await conn.fetchrow("""
            INSERT INTO tb_user_test (name)
            VALUES ('Test User')
            RETURNING id, name
        """)

        user_id = row["id"]

        # Create instances
        user_with_id = UserWithID(id=ID(user_id), name=row["name"])
        user_with_uuid = UserWithUUID(id=user_id, name=row["name"])

        # Both should serialize the same
        assert str(user_with_id.id) == str(user_id)
        assert str(user_with_uuid.id) == str(user_id)

        # Cleanup
        await conn.execute("DROP TABLE tb_user_test")
```

### Changelog (v1.10.0)

**File**: `CHANGELOG.md`

```markdown
## [1.10.0] - 2025-XX-XX

### Added

- **ID Type**: New `ID` type for opaque identifiers (maps to UUID in PostgreSQL)
  - Import: `from fraiseql.types import ID`
  - GraphQL standard compliance
  - Recommended for all entity identifiers
  - See [ID Type Guide](docs/core/id-type.md)
- `IDScalar` GraphQL scalar type
- Documentation for ID vs UUID usage patterns

### Compatibility

- **Non-breaking**: `UUID` type remains fully supported
- All existing code continues to work
- `ID` and `UUID` are interoperable (both map to PostgreSQL UUID)
```

### Release Notes (v1.10.0)

**File**: `docs/release-notes/v1.10.0.md`

```markdown
# FraiseQL v1.10.0 Release Notes

## New ID Type 🎉

FraiseQL now exports an `ID` type for GraphQL-standard identifier handling!

### Quick Example

\`\`\`python
from fraiseql.types import ID  # NEW!

@fraiseql.type
class User:
    id: ID  # GraphQL standard, maps to UUID in PostgreSQL
    name: str
\`\`\`

### Why?

- **GraphQL Standard**: Matches Strawberry, Graphene, Apollo
- **Better DX**: Shorter imports, clearer intent
- **Future-proof**: Can change UUID implementation later

### Migration

**No action required!** Your existing `UUID` code continues to work:

\`\`\`python
# OLD (still supported)
from fraiseql.types import UUID

@fraiseql.type
class User:
    id: UUID

# NEW (recommended)
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
\`\`\`

Both map to PostgreSQL `UUID` - no database changes needed.

### When to use ID vs UUID?

- **Use `ID`**: For all entity identifiers (recommended)
- **Use `UUID`**: When UUID format explicitly matters (compliance, tracing)

See [ID Type Guide](../core/id-type.md) for details.

## Other Changes

- Documentation improvements
- Bug fixes
- Performance optimizations

## Upgrade Guide

\`\`\`bash
pip install --upgrade fraiseql
\`\`\`

No breaking changes - fully backward compatible.
\`\`\`
```

### Commit (v1.10.0)

```bash
git checkout -b feat/export-id-type
git add src/fraiseql/types/__init__.py
git add src/fraiseql/types/scalars/id_scalar.py
git add src/fraiseql/types/scalars/__init__.py
git add src/fraiseql/gql/schema_builder.py
git add src/fraiseql/introspection/type_mapper.py
git add tests/
git add docs/

git commit -m "feat(types): export ID type for GraphQL-standard identifiers

BREAKING: None (additive only)

Added:
- ID type: from fraiseql.types import ID
- IDScalar: GraphQL scalar for ID type
- ID = UUID convention (opinionated PostgreSQL mapping)
- Documentation for ID vs UUID usage patterns

Details:
- ID type maps to UUID in PostgreSQL
- GraphQL standard compliance (id: ID!)
- Interoperable with UUID type (both map to PG UUID)
- Recommended for all entity identifiers
- UUID still available for explicit UUID requirements

Docs:
- New guide: docs/core/id-type.md
- Updated: docs/core/types-and-schema.md
- Updated: docs/reference/types-api.md
- Release notes: docs/release-notes/v1.10.0.md

Tests:
- Integration tests for ID type
- ID/UUID interoperability tests
- Type mapping tests
- Schema generation tests

Closes #XXX
"
```

---

## Phase 2: Encourage Migration (v1.11.0 - v1.x)

**Status**: Planning
**Timeline**: 1-3 releases after v1.10.0 (~2-4 months)
**Breaking**: No

### Objective

Encourage users to adopt `ID` type while maintaining full backward compatibility with `UUID`.

### Documentation Updates

#### 2.1: Update all docs to show ID as primary

**Strategy**: Show `ID` first, mention `UUID` as advanced

**Example pattern**:

```markdown
## Defining Types

\`\`\`python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID  # Recommended: GraphQL standard
    name: str
\`\`\`

<details>
<summary>Advanced: Using explicit UUID type</summary>

\`\`\`python
from fraiseql.types import UUID

@fraiseql.type
class AuditLog:
    correlation_id: UUID  # Explicit UUID requirement
\`\`\`

</details>
```

**Files to update**:
- `docs/getting-started/*.md` - All getting started guides
- `docs/core/*.md` - Core documentation
- `docs/guides/*.md` - All guides
- `docs/examples/*.md` - All examples
- `docs/tutorials/*.md` - All tutorials

**Method**: Use Task agent for bulk update with review

#### 2.2: Add migration guide

**New file**: `docs/migration/uuid-to-id.md`

```markdown
# Migration Guide: UUID → ID

This guide helps you migrate from `UUID` to `ID` type annotations.

## Why Migrate?

- ✅ GraphQL standard compliance
- ✅ Shorter imports
- ✅ Better DX
- ✅ Future-proof

## Quick Migration

### Step 1: Update imports

\`\`\`python
# BEFORE
from fraiseql.types import UUID

# AFTER
from fraiseql.types import ID
\`\`\`

### Step 2: Update type annotations

\`\`\`python
# BEFORE
@fraiseql.type
class User:
    id: UUID

@fraiseql.type
class Post:
    id: UUID
    author_id: UUID

# AFTER
@fraiseql.type
class User:
    id: ID

@fraiseql.type
class Post:
    id: ID
    author_id: ID
\`\`\`

### Step 3: Test

No database changes needed! Both `ID` and `UUID` map to PostgreSQL `UUID`.

\`\`\`bash
# Run your tests
pytest tests/

# Everything should pass
\`\`\`

## Automated Migration

\`\`\`bash
# Search & replace (review changes before committing!)
find src/ -name "*.py" -exec sed -i \\
  's/from fraiseql.types import UUID/from fraiseql.types import ID/g' {} +

find src/ -name "*.py" -exec sed -i \\
  's/\\bid: UUID/id: ID/g' {} +

find src/ -name "*.py" -exec sed -i \\
  's/_id: UUID/_id: ID/g' {} +
\`\`\`

## Edge Cases

### Mixed imports (UUID for other uses)

\`\`\`python
# BEFORE
from fraiseql.types import UUID
from uuid import uuid4

@fraiseql.type
class User:
    id: UUID
    correlation_id: UUID  # For distributed tracing

# AFTER
from fraiseql.types import ID, UUID
from uuid import uuid4

@fraiseql.type
class User:
    id: ID                      # ✅ Standard identifier
    correlation_id: UUID         # ✅ Explicit UUID (tracing)
\`\`\`

### Client-generated UUIDs

\`\`\`python
# Keep using UUID when clients generate IDs
from fraiseql.types import ID, UUID

@fraiseql.type
class IdempotencyKey:
    key: UUID  # ✅ Client-generated, UUID matters

@fraiseql.type
class User:
    id: ID  # ✅ Server-generated, opaque
\`\`\`

## Rollback

If needed, rollback is trivial:

\`\`\`bash
# Reverse the changes
find src/ -name "*.py" -exec sed -i \\
  's/from fraiseql.types import ID/from fraiseql.types import UUID/g' {} +
\`\`\`

No database changes needed - both types map to UUID.

## FAQ

**Q: Do I need to update my database?**
A: No! Both `ID` and `UUID` map to PostgreSQL `UUID`.

**Q: Will my GraphQL schema change?**
A: Yes - `id: UUID!` becomes `id: ID!` (GraphQL standard).

**Q: Will this break my clients?**
A: No - both serialize to UUID strings. Clients see the same values.

**Q: Can I use both ID and UUID?**
A: Yes! Use `ID` for identifiers, `UUID` when UUID format matters.

**Q: What if I want to keep using UUID?**
A: That's fine! `UUID` is fully supported and will remain so.
\`\`\`
```

#### 2.3: Add prominent notice in docs

**File**: `docs/index.md` or `docs/getting-started/quickstart.md`

Add info box:

```markdown
> 💡 **New in v1.10.0**: FraiseQL now exports `ID` type for GraphQL-standard identifiers!
>
> \`\`\`python
> from fraiseql.types import ID  # Recommended
>
> @fraiseql.type
> class User:
>     id: ID  # GraphQL standard, maps to UUID in PostgreSQL
> \`\`\`
>
> See [ID Type Guide](./core/id-type.md) for details.
```

### Blog Post / Announcement (v1.11.0)

**File**: `docs/blog/2025-xx-introducing-id-type.md`

```markdown
# Introducing the ID Type - GraphQL Standard Identifiers in FraiseQL

FraiseQL v1.10.0 introduced the `ID` type, and today we're encouraging all projects to adopt it!

## What Changed?

\`\`\`python
# OLD (still supported)
from fraiseql.types import UUID

@fraiseql.type
class User:
    id: UUID

# NEW (recommended)
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
\`\`\`

## Why ID?

1. **GraphQL Standard**: Matches Strawberry, Graphene, Apollo
2. **Better DX**: Shorter, clearer, more familiar
3. **Future-proof**: Can change UUID implementation later

## Migration

See our [Migration Guide](../migration/uuid-to-id.md).

**TL;DR**: Search & replace, no database changes needed!

## Backward Compatibility

`UUID` is fully supported and will remain so. Migrate at your own pace.
\`\`\`
```

### Commit (v1.11.0)

```bash
git checkout -b docs/encourage-id-migration
git add docs/

git commit -m "docs: encourage migration from UUID to ID type

Changes:
- Update all docs to show ID as primary example
- Add migration guide (docs/migration/uuid-to-id.md)
- Add prominent notice about ID type
- Blog post announcing ID type recommendation

Backward compatibility:
- UUID still fully supported
- No breaking changes
- Migration is optional but recommended
"
```

---

## Phase 3: CLI Default Change (v2.0.0)

**Status**: Planning
**Timeline**: Next major release (~6 months from v1.10.0)
**Breaking**: Yes (for new projects only)

### Objective

Make `ID` the default in CLI-generated code while maintaining full backward compatibility for existing projects.

### Changes

#### 3.1: Update CLI templates

**File**: `src/fraiseql/cli/commands/init.py`

```python
# Update all template strings to use ID instead of UUID

user_type = '''"""User type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types import ID  # ← CHANGED from UUID

@fraiseql.type
class User:
    """A blog author."""
    id: ID  # ← CHANGED from UUID
    username: str = fraise_field(description="Unique username")
    email: str = fraise_field(description="Email address")
    bio: str | None = fraise_field(description="User biography")
    avatar_url: str | None = fraise_field(description="Profile picture URL")
    created_at: str = fraise_field(description="Account creation date")
    posts: list["Post"] = fraise_field(description="Posts written by this user")
'''

# Similar changes for post_type, comment_type, etc.
```

**File**: `src/fraiseql/cli/commands/generate.py`

```python
# Update mutation templates

mutations_content = f'''"""CRUD mutations for {type_name}."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.mutations import CQRSRepository
from fraiseql.types import ID  # ← CHANGED from UUID

from ..types import {type_name}


@fraiseql.input
class Create{type_name}Input:
    """Input for creating a {type_name}."""
    # TODO: Add your fields here
    name: str = fraise_field(description="Name")


@fraiseql.input
class Update{type_name}Input:
    """Input for updating a {type_name}."""
    id: ID = fraise_field(description="{type_name} ID")  # ← CHANGED from UUID
    # TODO: Add your fields here
    name: str | None = fraise_field(description="Name")
'''

# Update resolver template
resolver_content = f'''
async def {type_name.lower()}(
    id: ID,  # ← CHANGED from UUID
    repository: CQRSRepository
) -> {type_name}:
    return await repository.get_by_id(id)
'''
```

#### 3.2: Add CLI flag for legacy behavior

**File**: `src/fraiseql/cli/commands/init.py`

```python
@click.command()
@click.argument("project_name")
@click.option(
    "--template",
    type=click.Choice(["minimal", "blog", "api"]),
    default="minimal",
    help="Project template to use",
)
@click.option(
    "--use-uuid",  # ← NEW FLAG
    is_flag=True,
    default=False,
    help="Use UUID type instead of ID (legacy)",
)
def init(project_name: str, template: str, use_uuid: bool):
    """Initialize a new FraiseQL project."""

    # Determine which ID type to use
    id_type = "UUID" if use_uuid else "ID"
    id_import = (
        "from fraiseql.types import UUID"
        if use_uuid
        else "from fraiseql.types import ID"
    )

    # Use id_type in template generation
    user_type = f'''"""User type definition."""

import fraiseql
from fraiseql import fraise_field
{id_import}

@fraiseql.type
class User:
    """A blog author."""
    id: {id_type}
    # ... rest of template
'''

    # Rest of init logic...
```

**Test**:
```bash
# Default: Uses ID
fraiseql init my-project

# Legacy: Uses UUID
fraiseql init my-project --use-uuid
```

#### 3.3: Update internal patterns/trinity

**File**: `src/fraiseql/patterns/trinity.py`

```python
# Update trinity pattern to use ID by default

from fraiseql.types import ID  # ← CHANGED from UUID

def trinity_field(**kwargs):
    """Trinity pattern field with ID."""
    return {
        "description": kwargs.get("description", ""),
        # ... field config
    }

# Example in docstring
class User:
    """
    Example:
        @fraiseql.type
        class User:
            id: ID = strawberry.field(**trinity_field(description="User ID"))
    """
    id: ID = strawberry.field(**trinity_field(description="User UUID"))  # ← CHANGED
```

### Documentation Updates (v2.0.0)

#### 3.4: Update all docs to use ID exclusively

**Files**: All documentation files

**Pattern**:
```python
# Remove UUID from examples (unless explicitly needed)
from fraiseql.types import ID  # Only show ID

@fraiseql.type
class User:
    id: ID  # Default, no explanation needed
```

**Only show UUID in advanced examples**:
```markdown
## Advanced: Explicit UUID Requirements

For specific use cases where UUID format matters:

\`\`\`python
from fraiseql.types import ID, UUID

@fraiseql.type
class AuditLog:
    id: ID                  # Standard identifier
    correlation_id: UUID    # Distributed tracing
\`\`\`
```

#### 3.5: Update migration guide

**File**: `docs/migration/v1-to-v2.md`

```markdown
# Migration Guide: FraiseQL v1.x → v2.0

## Breaking Changes

### CLI Default: ID instead of UUID

**Impact**: New projects only (existing projects unaffected)

\`\`\`bash
# v1.x generated:
from fraiseql.types import UUID

@fraiseql.type
class User:
    id: UUID

# v2.0 generates:
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
\`\`\`

**Action Required**: None for existing projects

**Action for new projects**: Use `fraiseql init --use-uuid` if you need UUID

### GraphQL Schema Change

**Impact**: GraphQL schema shows `ID!` instead of `UUID!`

\`\`\`graphql
# v1.x
type User {
  id: UUID!
}

# v2.0
type User {
  id: ID!
}
\`\`\`

**Client Impact**: None - both serialize to UUID strings

## Upgrade Steps

### 1. Update FraiseQL

\`\`\`bash
pip install --upgrade fraiseql
\`\`\`

### 2. (Optional) Migrate to ID type

See [UUID → ID Migration Guide](./uuid-to-id.md)

**Benefits**:
- GraphQL standard compliance
- Better DX
- Future-proof

**Not required** - UUID is still fully supported

## Rollback

If needed, you can downgrade:

\`\`\`bash
pip install fraiseql==1.x.x
\`\`\`

Or use `--use-uuid` flag:

\`\`\`bash
fraiseql init my-project --use-uuid
\`\`\`
\`\`\`
```

### Changelog (v2.0.0)

**File**: `CHANGELOG.md`

```markdown
## [2.0.0] - 2025-XX-XX

### Breaking Changes

#### CLI Default: ID instead of UUID

- `fraiseql init` now generates `id: ID` instead of `id: UUID`
- GraphQL schema shows `id: ID!` instead of `id: UUID!`
- **Impact**: New projects only
- **Existing projects**: Unaffected
- **Rollback**: Use `fraiseql init --use-uuid` flag

### Migration

- See [v1 → v2 Migration Guide](docs/migration/v1-to-v2.md)
- See [UUID → ID Migration Guide](docs/migration/uuid-to-id.md)

### Compatibility

- **UUID type**: Fully supported (no deprecation)
- **Existing code**: Works without changes
- **Database**: No changes needed (both map to UUID)

### Rationale

- GraphQL standard compliance
- Better DX (matches Strawberry, Graphene, Apollo)
- Future-proof (opaque identifiers)
```

### Release Notes (v2.0.0)

**File**: `docs/release-notes/v2.0.0.md`

```markdown
# FraiseQL v2.0.0 Release Notes

## 🎉 Major Release: ID Type is Now Default

FraiseQL v2.0 makes `ID` the default type for entity identifiers!

### What Changed?

**CLI-generated code now uses `ID`:**

\`\`\`bash
fraiseql init my-project
\`\`\`

\`\`\`python
# Generated code:
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID  # NEW default
    name: str
\`\`\`

### Migration Guide

**Existing projects**: No action required! Your code continues to work.

**New projects**: Use `ID` (default) or `--use-uuid` flag for legacy.

See [Migration Guide](../migration/v1-to-v2.md) for details.

### Why This Change?

- ✅ **GraphQL Standard**: `id: ID!` matches ecosystem expectations
- ✅ **Better DX**: Shorter imports, clearer intent
- ✅ **Future-proof**: Can change UUID implementation later

### Backward Compatibility

- `UUID` type is **fully supported** (no deprecation)
- Existing code **works without changes**
- No database migrations needed

### Upgrade

\`\`\`bash
pip install --upgrade fraiseql
\`\`\`

## Other Changes

- Bug fixes
- Performance improvements
- Documentation updates

## Breaking Changes

- CLI default changed from UUID to ID (new projects only)
- GraphQL schema change: `UUID!` → `ID!` (same values)

See [Changelog](../../CHANGELOG.md) for full details.
\`\`\`
```

### Commit (v2.0.0)

```bash
git checkout -b feat/id-default-v2
git add src/fraiseql/cli/
git add src/fraiseql/patterns/
git add docs/
git add CHANGELOG.md

git commit -m "feat!: make ID the default type for identifiers (v2.0)

BREAKING CHANGE: CLI now generates ID instead of UUID by default

Changed:
- fraiseql init: Uses ID type instead of UUID
- fraiseql generate: Uses ID type instead of UUID
- GraphQL schema: id: ID! instead of id: UUID!
- All documentation updated to use ID as primary

Backward compatibility:
- UUID type fully supported (no deprecation)
- Existing code works without changes
- CLI flag: --use-uuid for legacy behavior
- No database changes needed

Migration:
- See docs/migration/v1-to-v2.md
- See docs/migration/uuid-to-id.md
- Optional migration (UUID still supported)

Rationale:
- GraphQL standard compliance
- Better DX (matches Strawberry, Graphene, Apollo)
- Opaque identifiers (future-proof)

Closes #XXX
"
```

---

## Phase 4: Long-term Maintenance

**Status**: Ongoing
**Timeline**: v2.0.0+
**Breaking**: No

### Objectives

1. Monitor adoption of ID type
2. Support both ID and UUID indefinitely
3. Improve documentation based on feedback
4. Consider future identifier types (ULID, Snowflake, etc.)

### Monitoring

**Track metrics**:
- % of new projects using ID vs UUID
- GitHub issues related to ID/UUID confusion
- Community feedback on DX

**Tools**:
```python
# Optional telemetry (opt-in)
# Track which type users prefer
{
  "id_type_usage": {
    "ID": "75%",
    "UUID": "25%"
  }
}
```

### Support Policy

**UUID type**: Supported indefinitely
- No deprecation warnings
- Full feature parity with ID
- Documentation maintained

**ID type**: Recommended default
- Primary documentation examples
- CLI default
- Encouraged for new projects

### Future Enhancements

**Potential additions** (post-v2.0):

1. **ULID Support**:
   ```python
   from fraiseql.types import ID, ULID

   @fraiseql.type
   class User:
       id: ID  # Could be UUID or ULID (config-driven)
   ```

2. **Snowflake IDs**:
   ```python
   from fraiseql.types import ID, Snowflake

   @fraiseql.type
   class HighVolumeEntity:
       id: Snowflake  # Twitter-style IDs
   ```

3. **Custom ID Generators**:
   ```python
   # fraiseql.toml
   [id]
   type = "ulid"  # or "uuid", "snowflake", "custom"
   generator = "my_app.id_generator"
   ```

### Documentation Maintenance

**Ongoing updates**:
- Keep migration guides current
- Update examples with best practices
- Add case studies of ID usage
- FAQ updates based on issues

---

## Success Criteria

### Phase 0 (Immediate)
- ✅ All docs use `fraiseql.types import UUID` (not stdlib)
- ✅ Zero imports of `from uuid import UUID` in docs
- ✅ Commit merged to dev branch

### Phase 1 (v1.10.0)
- ✅ `ID` type exported from `fraiseql.types`
- ✅ `IDScalar` GraphQL type working
- ✅ Integration tests passing
- ✅ Documentation added for ID type
- ✅ Release notes published
- ✅ Zero regressions in existing UUID usage

### Phase 2 (v1.11.0-v1.x)
- ✅ All documentation shows ID as primary
- ✅ Migration guide published
- ✅ Blog post/announcement published
- ✅ Community awareness (GitHub, Discord, etc.)
- ✅ >50% of new projects using ID

### Phase 3 (v2.0.0)
- ✅ CLI generates ID by default
- ✅ `--use-uuid` flag works for legacy
- ✅ All docs updated to ID as default
- ✅ Migration guide for v1→v2
- ✅ Release notes published
- ✅ Zero breaking changes for existing projects

### Phase 4 (Ongoing)
- ✅ >75% of new projects using ID
- ✅ UUID still fully supported
- ✅ Community satisfaction high
- ✅ Zero confusion about ID vs UUID

---

## Risk Mitigation

### Risk 1: User Confusion

**Risk**: Users confused about ID vs UUID

**Mitigation**:
- Clear documentation
- Migration guides
- FAQ section
- Prominent announcements

### Risk 2: Breaking Existing Code

**Risk**: Changes break existing projects

**Mitigation**:
- Phase 1 & 2 are non-breaking
- Only Phase 3 (v2.0) has breaking changes (new projects only)
- Extensive testing
- Clear migration path

### Risk 3: GraphQL Client Issues

**Risk**: GraphQL clients break when schema changes UUID→ID

**Mitigation**:
- Both serialize to same string format
- No actual client impact (type name changes only)
- Document in release notes

### Risk 4: Performance Regression

**Risk**: ID type slower than UUID

**Mitigation**:
- ID wraps UUID (minimal overhead)
- Benchmark tests
- Monitor performance metrics

### Risk 5: Low Adoption

**Risk**: Users don't migrate to ID

**Mitigation**:
- Make migration optional (UUID still supported)
- Clear benefits communication
- Community education
- Gradual rollout (3 phases)

---

## Rollback Plan

### Phase 1 Rollback (v1.10.0)

If ID type has critical bugs:

```bash
# Revert the commit
git revert <commit-hash>

# Release hotfix
fraiseql==1.10.1  # Without ID type
```

Users can continue using UUID (no impact).

### Phase 2 Rollback (v1.11.0)

If documentation changes cause confusion:

```bash
# Revert docs to show UUID as primary
git revert <doc-commits>

# ID type still available, just not prominent
```

### Phase 3 Rollback (v2.0.0)

If CLI change causes issues:

```bash
# Revert CLI to use UUID by default
git revert <cli-commits>

# Release v2.0.1 with UUID default restored
# Or: make --use-uuid the default flag
```

Users can:
- Use `fraiseql init --use-id` for new behavior
- Downgrade to v1.x if needed

---

## Communication Plan

### Internal (FraiseQL Team)

**Before v1.10.0 release**:
- [ ] Review this plan with team
- [ ] Assign tasks for Phase 1
- [ ] Set release date
- [ ] Prepare announcement

**Before v2.0.0 release**:
- [ ] 3-month warning to community
- [ ] Beta release for testing
- [ ] Address feedback
- [ ] Final announcement

### External (Community)

**v1.10.0 Announcement**:
- GitHub release notes
- Discord/Slack announcement
- Twitter/social media
- Documentation banner

**v1.11.0+ Announcements**:
- Blog post about ID type
- Migration guide promotion
- Community examples
- Case studies

**v2.0.0 Announcement**:
- Major release announcement
- Migration webinar/tutorial
- FAQ updates
- Support channels ready

---

## Timeline Summary

```
Now (v1.9.0)
    │
    ├─ Phase 0: Fix docs (1 hour)
    │  └─ Use fraiseql.types.UUID
    │
    ↓
+2 weeks (v1.10.0)
    │
    ├─ Phase 1: Export ID type (non-breaking)
    │  ├─ ID type available
    │  ├─ Documentation added
    │  └─ UUID still default
    │
    ↓
+2 months (v1.11.0-v1.x)
    │
    ├─ Phase 2: Encourage migration (non-breaking)
    │  ├─ Docs show ID as primary
    │  ├─ Migration guide
    │  └─ Community education
    │
    ↓
+4 months (v2.0.0)
    │
    ├─ Phase 3: ID as default (breaking for new projects)
    │  ├─ CLI uses ID
    │  ├─ --use-uuid flag for legacy
    │  └─ Full docs migration
    │
    ↓
Ongoing (v2.0.0+)
    │
    └─ Phase 4: Long-term maintenance
       ├─ Support both ID and UUID
       ├─ Monitor adoption
       └─ Future enhancements (ULID, etc.)
```

---

## Conclusion

This migration plan provides a **safe, gradual path** from UUID to ID type over 3 releases:

1. **v1.10.0**: Export ID (non-breaking, additive)
2. **v1.11.0+**: Encourage migration (non-breaking, education)
3. **v2.0.0**: ID as default (breaking for new projects only)

**Key principles**:
- ✅ Backward compatibility (UUID never deprecated)
- ✅ Gradual adoption (6-month timeline)
- ✅ Clear communication (docs, guides, announcements)
- ✅ Low risk (each phase is reversible)

**Next step**: Execute Phase 0 (immediate documentation fix).
