# Immediate ID Migration Plan - No Users, Full Migration Now

**Date**: 2025-12-30
**Status**: Ready to implement
**Timeline**: This session (2-3 hours)
**Breaking**: Yes, but no users affected

---

## Strategy

Since there are **no users yet**, we can:

1. **Skip gradual migration** - Go straight to final state
2. **Remove UUID entirely** from public API (keep internal for compatibility)
3. **Make ID the only documented type** for identifiers
4. **Simplify** - No migration guides, no dual documentation

---

## Implementation Order

### 1. Export ID Type (30 min)

**File**: `src/fraiseql/types/scalars/id_scalar.py`

Update to add GraphQL scalar:

```python
"""GraphQL ID scalar backed by UUID."""

from __future__ import annotations

import uuid
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode, ValueNode

from fraiseql.types.definitions import ScalarMarker


# Serialization functions (reuse UUID logic since ID = UUID)
def serialize_id(value: Any) -> str:
    """Serialize an ID (UUID) to string."""
    if isinstance(value, uuid.UUID):
        return str(value)
    if isinstance(value, str):
        try:
            uuid.UUID(value)
            return value
        except ValueError:
            pass
    msg = f"ID cannot represent non-UUID value: {value!r}"
    raise GraphQLError(msg)


def parse_id_value(value: Any) -> uuid.UUID:
    """Parse an ID string into a UUID object."""
    if isinstance(value, str):
        try:
            return uuid.UUID(value)
        except ValueError:
            msg = f"Invalid ID string provided: {value!r}"
            raise GraphQLError(msg) from None
    msg = f"ID cannot represent non-string value: {value!r}"
    raise GraphQLError(msg)


def parse_id_literal(ast: ValueNode, variables: dict[str, object] | None = None) -> uuid.UUID:
    """Parse an ID literal from GraphQL AST."""
    _ = variables
    if isinstance(ast, StringValueNode):
        return parse_id_value(ast.value)
    msg = f"ID cannot represent non-string literal: {getattr(ast, 'value', None)!r}"
    raise GraphQLError(msg)


# GraphQL Scalar
IDScalar = GraphQLScalarType(
    name="ID",
    description="A globally unique identifier in UUID format.",
    serialize=serialize_id,
    parse_value=parse_id_value,
    parse_literal=parse_id_literal,
)


# Python Type Marker
class IDField(str, ScalarMarker):
    """FraiseQL ID marker used for Python-side typing and introspection.

    Represents opaque identifiers, backed by UUID in PostgreSQL.
    """

    __slots__ = ()

    def __repr__(self) -> str:
        """Return a user-friendly type name for introspection and debugging."""
        return "ID"
```

**File**: `src/fraiseql/types/__init__.py`

```python
# Add import
from .scalars.id_scalar import IDField as ID

# Add to __all__
__all__ = [
    "CIDR",
    # ... existing exports ...
    "ID",      # ← NEW: Primary identifier type
    # Remove UUID from __all__ or keep for internal compatibility
    # ... rest of exports ...
]
```

**File**: `src/fraiseql/types/scalars/__init__.py`

```python
# Add import
from .id_scalar import IDScalar

# Add to __all__
__all__ = [
    # ... existing scalars ...
    "IDScalar",    # ← NEW
    # Keep UUIDScalar for internal use
    # ... rest of scalars ...
]
```

---

### 2. Update CLI Templates (20 min)

**File**: `src/fraiseql/cli/commands/init.py`

Replace all `UUID` with `ID`:

```python
user_type = '''"""User type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types import ID  # ← CHANGED

@fraiseql.type
class User:
    """A blog author."""
    id: ID  # ← CHANGED
    username: str = fraise_field(description="Unique username")
    email: str = fraise_field(description="Email address")
    bio: str | None = fraise_field(description="User biography")
    avatar_url: str | None = fraise_field(description="Profile picture URL")
    created_at: str = fraise_field(description="Account creation date")
    posts: list["Post"] = fraise_field(description="Posts written by this user")
'''

post_type = '''"""Post type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types import ID  # ← CHANGED

from .user import User
from .comment import Comment

@fraiseql.type
class Post:
    """A blog post."""
    id: ID  # ← CHANGED
    title: str = fraise_field(description="Post title")
    slug: str = fraise_field(description="URL-friendly slug")
    content: str = fraise_field(description="Post content in Markdown")
    excerpt: str | None = fraise_field(description="Short summary")
    author: User = fraise_field(description="Post author")
    published_at: str | None = fraise_field(description="Publication date")
    updated_at: str = fraise_field(description="Last update date")
    tags: list[str] = fraise_field(description="Post tags")
    comments: list[Comment] = fraise_field(description="Post comments")
    is_published: bool = fraise_field(description="Whether post is published")
'''

comment_type = '''"""Comment type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types import ID  # ← CHANGED

from .user import User

@fraiseql.type
class Comment:
    """A comment on a blog post."""
    id: ID  # ← CHANGED
    content: str = fraise_field(description="Comment text")
    author: User = fraise_field(description="Comment author")
    created_at: str = fraise_field(description="When comment was posted")
    updated_at: str = fraise_field(description="Last edit time")
    is_approved: bool = fraise_field(description="Whether comment is approved")
'''
```

**File**: `src/fraiseql/cli/commands/generate.py`

```python
mutations_content = f'''"""CRUD mutations for {type_name}."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.mutations import CQRSRepository
from fraiseql.types import ID  # ← CHANGED

from ..types import {type_name}


@fraiseql.input
class Create{type_name}Input:
    """Input for creating a {type_name}."""
    # TODO: Add your fields here
    name: str = fraise_field(description="Name")


@fraiseql.input
class Update{type_name}Input:
    """Input for updating a {type_name}."""
    id: ID = fraise_field(description="{type_name} ID")  # ← CHANGED
    # TODO: Add your fields here
    name: str | None = fraise_field(description="Name")
'''
```

---

### 3. Update Schema Builder & Introspection (30 min)

**File**: `src/fraiseql/gql/schema_builder.py`

Find scalar registration and add IDScalar:

```python
from fraiseql.types.scalars import (
    DateScalar,
    DateTimeScalar,
    IDScalar,      # ← NEW
    JSONScalar,
    # ... other scalars
)

# In scalar registration
BUILTIN_SCALARS = {
    # ... existing ...
    "ID": IDScalar,      # ← NEW
    # ... rest ...
}
```

**File**: `src/fraiseql/introspection/type_mapper.py`

Ensure ID maps to UUID in PostgreSQL:

```python
def map_graphql_type_to_postgresql(graphql_type: str) -> str:
    """Map GraphQL type to PostgreSQL type."""
    type_mapping = {
        "String": "TEXT",
        "Int": "INTEGER",
        "Float": "DOUBLE PRECISION",
        "Boolean": "BOOLEAN",
        "ID": "UUID",      # ← NEW
        # ... rest ...
    }
    return type_mapping.get(graphql_type, "TEXT")
```

---

### 4. Update Internal Code (15 min)

**File**: `src/fraiseql/patterns/trinity.py`

```python
# Update example to use ID
from fraiseql.types import ID  # ← CHANGED

# In docstring example
class User:
    """
    Example:
        @fraiseql.type
        class User:
            id: ID = strawberry.field(**trinity_field(description="User ID"))
    """
    id: ID = strawberry.field(**trinity_field(description="User ID"))  # ← CHANGED
```

**Check for other internal uses**:

```bash
# Find all internal UUID usage that should be ID
grep -r "from fraiseql.types.scalars import UUID" src/fraiseql/
grep -r "id: UUID" src/fraiseql/
```

Replace with `from fraiseql.types import ID` and `id: ID`.

---

### 5. Update All Documentation (45 min)

**Automated search & replace**:

```bash
# Replace Python stdlib UUID imports
find docs/ -name "*.md" -exec sed -i \
  's/from uuid import UUID$/from fraiseql.types import ID/g' {} +

# Replace fraiseql UUID imports
find docs/ -name "*.md" -exec sed -i \
  's/from fraiseql.types.scalars import UUID/from fraiseql.types import ID/g' {} +

find docs/ -name "*.md" -exec sed -i \
  's/from fraiseql.types import UUID/from fraiseql.types import ID/g' {} +

# Replace type annotations (careful with this - review changes)
find docs/ -name "*.md" -exec sed -i \
  's/id: UUID/id: ID/g' {} +

find docs/ -name "*.md" -exec sed -i \
  's/_id: UUID/_id: ID/g' {} +
```

**Manual review needed**:

- Check for cases where UUID is explicitly needed (correlation_id, etc.)
- Update GraphQL schema examples: `id: UUID!` → `id: ID!`

**New documentation file**: `docs/core/id-type.md`

```markdown
# ID Type

FraiseQL uses `ID` for all entity identifiers.

## Quick Start

\`\`\`python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
    name: str
\`\`\`

## PostgreSQL Mapping

`ID` maps to `UUID` in PostgreSQL:

\`\`\`sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL
);
\`\`\`

## GraphQL Schema

\`\`\`graphql
type User {
  id: ID!
  name: String!
}
\`\`\`

## Why UUID?

FraiseQL makes the opinionated choice to use UUIDs for all IDs because:

- **Security**: No enumeration attacks
- **Distribution**: Generate IDs anywhere (client, server, offline)
- **Scalability**: No coordination needed
- **Modern**: Industry standard (Stripe, GitHub, Hasura)

## Best Practices

\`\`\`python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID  # Primary key

@fraiseql.type
class Post:
    id: ID          # Primary key
    author_id: ID   # Foreign key reference
\`\`\`
```

---

### 6. Write Tests (30 min)

**New file**: `tests/types/test_id_type.py`

```python
"""Tests for ID type."""
import uuid
import pytest
from fraiseql.types import ID
from fraiseql.types.scalars import IDScalar


def test_id_importable():
    """Test that ID is importable from fraiseql.types."""
    assert ID is not None


def test_id_scalar_exists():
    """Test that IDScalar exists."""
    assert IDScalar is not None
    assert IDScalar.name == "ID"


def test_id_scalar_serialize():
    """Test ID serialization."""
    test_uuid = uuid.uuid4()

    # Serialize UUID
    assert IDScalar.serialize(test_uuid) == str(test_uuid)

    # Serialize string
    assert IDScalar.serialize(str(test_uuid)) == str(test_uuid)


def test_id_scalar_parse():
    """Test ID parsing."""
    test_uuid_str = "550e8400-e29b-41d4-a716-446655440000"

    parsed = IDScalar.parse_value(test_uuid_str)
    assert isinstance(parsed, uuid.UUID)
    assert str(parsed) == test_uuid_str


def test_id_scalar_parse_invalid():
    """Test ID parsing with invalid value."""
    from graphql import GraphQLError

    with pytest.raises(GraphQLError):
        IDScalar.parse_value("not-a-uuid")

    with pytest.raises(GraphQLError):
        IDScalar.parse_value(123)
```

**New file**: `tests/integration/test_id_in_schema.py`

```python
"""Integration tests for ID type in schema."""
import uuid
import pytest
import fraiseql
from fraiseql.types import ID


@pytest.fixture
async def schema_with_id(db_pool):
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

    # Create table
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


async def test_id_in_graphql_query(schema_with_id):
    """Test ID type in GraphQL query."""
    query = """
        query {
            users {
                id
                name
                email
            }
        }
    """

    result = await schema_with_id.execute(query)

    assert result.errors is None
    assert len(result.data["users"]) == 2

    # Verify IDs are valid UUIDs
    for user in result.data["users"]:
        user_id = user["id"]
        assert isinstance(user_id, str)
        uuid.UUID(user_id)  # Should not raise


async def test_id_type_in_schema_introspection(schema_with_id):
    """Test that ID appears correctly in schema introspection."""
    introspection_query = """
        query {
            __type(name: "User") {
                fields {
                    name
                    type {
                        name
                        kind
                    }
                }
            }
        }
    """

    result = await schema_with_id.execute(introspection_query)

    assert result.errors is None

    # Find id field
    fields = result.data["__type"]["fields"]
    id_field = next(f for f in fields if f["name"] == "id")

    # Check that type is ID (not UUID)
    assert id_field["type"]["name"] == "ID"
```

---

### 7. Update **init**.py Exports (5 min)

**File**: `src/fraiseql/__init__.py`

Check if UUID is exported from main package, if so consider removing or keeping for backward compat:

```python
# If UUID is currently exported, decide:

# Option 1: Remove UUID from public API
# (Recommended since no users)
__all__ = [
    # ... other exports ...
    # Remove "UUID" if it's here
]

# Option 2: Keep UUID as alias to ID
# (More conservative)
from .types import ID as UUID  # UUID = ID internally
```

---

### 8. Run Tests (15 min)

```bash
# Run full test suite
make test

# Or
pytest tests/ -v

# Specifically test ID type
pytest tests/types/test_id_type.py -v
pytest tests/integration/test_id_in_schema.py -v

# Run linting
make lint

# Run formatting
make format
```

---

### 9. Verify Documentation (10 min)

```bash
# Check that no old UUID imports remain
grep -r "from uuid import UUID" docs/
# Should return 0 results

grep -r "from fraiseql.types.scalars import UUID" docs/
# Should return 0 results

# Check that ID is used
grep -r "from fraiseql.types import ID" docs/ | wc -l
# Should return ~150+ results

# Check GraphQL schema examples
grep -r "id: UUID!" docs/
# Should return 0 results (or only in migration guides)

grep -r "id: ID!" docs/
# Should return many results
```

---

### 10. Create Commits (10 min)

**Commit 1: Core ID type implementation**

```bash
git add src/fraiseql/types/
git add src/fraiseql/gql/schema_builder.py
git add src/fraiseql/introspection/type_mapper.py
git add tests/types/test_id_type.py
git add tests/integration/test_id_in_schema.py

git commit -m "feat(types): add ID type for GraphQL-standard identifiers

BREAKING CHANGE: Primary identifier type is now ID instead of UUID

Added:
- ID type: from fraiseql.types import ID
- IDScalar: GraphQL scalar type (maps to UUID in PostgreSQL)
- Type mapping: ID → PostgreSQL UUID
- Integration tests for ID type

Details:
- ID is the opinionated default for all identifiers
- Backed by UUID in PostgreSQL (gen_random_uuid())
- GraphQL schema shows 'id: ID!' (standard)
- Follows Strawberry, Graphene, Apollo patterns

Rationale:
- GraphQL standard compliance
- Better DX (shorter, clearer)
- Future-proof (opaque identifiers)
- No users yet, clean break from UUID
"
```

**Commit 2: CLI templates**

```bash
git add src/fraiseql/cli/
git add src/fraiseql/patterns/

git commit -m "feat(cli): use ID type in generated code

Changed:
- fraiseql init: Generates 'id: ID' instead of 'id: UUID'
- fraiseql generate: Uses ID for all identifiers
- Trinity pattern: Updated examples to use ID

All generated code now uses GraphQL-standard ID type.
"
```

**Commit 3: Documentation**

```bash
git add docs/

git commit -m "docs: migrate all examples from UUID to ID

Changed:
- Replace 'from uuid import UUID' with 'from fraiseql.types import ID'
- Replace 'from fraiseql.types import UUID' with 'ID'
- Update all type annotations: 'id: UUID' → 'id: ID'
- Update GraphQL schemas: 'id: UUID!' → 'id: ID!'
- Add docs/core/id-type.md explaining ID type

Files affected: ~75 files, 150+ code blocks

All documentation now uses ID as the standard identifier type.
"
```

---

## Verification Checklist

Before pushing:

- [ ] `make test` passes (5991+ tests)
- [ ] `make lint` passes
- [ ] `make format` applied
- [ ] No `from uuid import UUID` in docs/ directory
- [ ] No `id: UUID` in docs/ examples (except edge cases)
- [ ] `fraiseql init test-project` generates ID types
- [ ] `fraiseql generate User` uses ID types
- [ ] All integration tests pass
- [ ] Documentation builds successfully
- [ ] GraphQL introspection shows ID type

---

## Expected Impact

### Code Changes

**Files modified**: ~90 files

- `src/fraiseql/types/`: 3 files
- `src/fraiseql/cli/`: 2 files
- `src/fraiseql/gql/`: 1 file
- `src/fraiseql/introspection/`: 1 file
- `src/fraiseql/patterns/`: 1 file
- `tests/`: 2 new files
- `docs/`: ~80 files

**Lines changed**: ~300 additions, ~150 deletions

### API Changes

**Before**:

```python
from fraiseql.types import UUID

@fraiseql.type
class User:
    id: UUID
```

**After**:

```python
from fraiseql.types import ID

@fraiseql.type
class User:
    id: ID
```

### GraphQL Schema Changes

**Before**:

```graphql
type User {
  id: UUID!
  name: String!
}
```

**After**:

```graphql
type User {
  id: ID!
  name: String!
}
```

### Database Schema

**No changes** - both map to PostgreSQL UUID:

```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL
);
```

---

## Timeline Estimate

| Task | Time |
|------|------|
| 1. Export ID type | 30 min |
| 2. Update CLI templates | 20 min |
| 3. Update schema builder | 30 min |
| 4. Update internal code | 15 min |
| 5. Update documentation | 45 min |
| 6. Write tests | 30 min |
| 7. Update exports | 5 min |
| 8. Run tests | 15 min |
| 9. Verify docs | 10 min |
| 10. Create commits | 10 min |

**Total**: ~3 hours

---

## Success Criteria

✅ ID type exported from `fraiseql.types`
✅ `fraiseql init` generates ID types
✅ All docs use ID instead of UUID
✅ All tests pass (5991+)
✅ GraphQL schema shows `id: ID!`
✅ No breaking changes for non-existent users
✅ Clean, simple API (one identifier type)

---

## Next Steps

After this migration:

1. **Test thoroughly** - Run full test suite multiple times
2. **Update CHANGELOG.md** - Document the change
3. **Update version** - Bump to v1.10.0 or v2.0.0 (your choice)
4. **Push to GitHub** - Create PR or merge directly
5. **Update README** - Ensure examples use ID

---

**Ready to proceed?** Let's start with step 1: Export ID type.
