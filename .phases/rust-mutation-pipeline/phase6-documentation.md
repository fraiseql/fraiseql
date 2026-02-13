# Phase 6: Documentation

**Duration**: 1-2 days
**Objective**: Document new architecture, migration guide, examples
**Status**: NOT STARTED

**Prerequisites**: Phase 5 complete (all tests passing, ready for release)

## Overview

Create comprehensive documentation:

1. Architecture documentation (how it works)
2. Migration guide (for users upgrading)
3. Examples (common patterns)
4. Release notes

## Tasks

### Task 6.1: Architecture Documentation

**File**: `docs/architecture/mutation_pipeline.md` (NEW)

**Outline**:

```markdown
# Mutation Pipeline Architecture

## Overview

The FraiseQL mutation pipeline transforms PostgreSQL function results
into GraphQL-compliant responses.

## Data Flow

PostgreSQL → Rust Parser → Rust Entity Processor → Rust Response Builder → JSON/Dict

## Two Formats

### Format 1: Simple (Entity-Only)

**When to use**: Simple queries/mutations that return entity data

**PostgreSQL**:
```sql
RETURN jsonb_build_object('id', user_id, 'name', 'John');
```

**Detection**: No `status` field OR invalid status value

**GraphQL Response**:

```json
{
  "data": {
    "getUser": {
      "__typename": "GetUserSuccess",
      "user": {
        "__typename": "User",
        "id": "123",
        "name": "John"
      },
      "message": "Success"
    }
  }
}
```

### Format 2: Full (Mutation Response)

**When to use**: Mutations with status/error handling, CASCADE

**PostgreSQL**:

```sql
RETURN ROW(
    'created',      -- status
    'User created', -- message
    'User',         -- entity_type (PascalCase!)
    user_data,      -- entity
    NULL,           -- updated_fields
    cascade_data,   -- cascade (optional)
    NULL            -- metadata
)::mutation_response;
```

**Detection**: Has valid `status` field

**GraphQL Response**:

```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "user": {
        "__typename": "User",
        "id": "123"
      },
      "message": "User created",
      "cascade": {
        "__typename": "Cascade",
        "updated": [...],
        "invalidations": [...]
      }
    }
  }
}
```

## CASCADE Placement

**CRITICAL**: CASCADE is placed at SUCCESS level, NOT nested in entity.

```json
{
  "data": {
    "createUser": {
      "user": { ... },        // ← Entity here
      "cascade": { ... }      // ← CASCADE here (sibling to user)
    }
  }
}
```

**NOT**:

```json
{
  "data": {
    "createUser": {
      "user": {
        "id": "123",
        "cascade": { ... }    // ❌ WRONG - never here!
      }
    }
  }
}
```

## Implementation Details

### Wrapper Detection

PostgreSQL functions can return entities wrapped in objects:

**Wrapper**: `{"post": {...}, "message": "Created"}`
**Direct**: `{"id": "123", "title": "..."}`

The pipeline automatically detects wrappers and extracts the entity.

### __typename Injection

Every GraphQL type must have `__typename`:

- Success response: `__typename: "CreateUserSuccess"`
- Entity: `__typename: "User"`
- CASCADE: `__typename: "Cascade"`

### camelCase Conversion

Snake_case field names converted to camelCase:

- `first_name` → `firstName`
- `created_at` → `createdAt`

Controlled by `auto_camel_case` config option.

## Status Classification

Status strings are classified into:

- **Success**: `success`, `created`, `updated`, `deleted`, `ok`, `new`
- **Error**: `failed:*`, `unauthorized:*`, `forbidden:*`, `not_found:*`, etc.
- **Noop**: `noop:*`

HTTP codes mapped from status:

- `failed:validation` → 422
- `not_found:*` → 404
- `unauthorized:*` → 401
- etc.

## Rust Modules

- `types.rs`: Core type definitions
- `parser.rs`: Format detection and JSON parsing
- `entity_processor.rs`: Entity extraction and __typename
- `response_builder.rs`: GraphQL response construction

## Performance

Typical mutation: <1ms overhead
With CASCADE: <2ms overhead

```

**Acceptance Criteria**:
- [ ] Clear explanation of two formats
- [ ] CASCADE placement documented
- [ ] Data flow diagram included
- [ ] Examples for each format

---

### Task 6.2: Migration Guide

**File**: `docs/guides/migrating_to_rust_pipeline.md` (NEW)

**Outline**:

```markdown
# Migrating to Unified Rust Pipeline

## Overview

FraiseQL v1.9 introduces a unified Rust mutation pipeline.

## For Users

### Breaking Changes

**None** - All changes are internal. Your mutations continue to work.

### Behavior Changes

#### 1. Non-HTTP Mode Returns Dicts

**Before (v1.8)**:
```python
result = await graphql_execute(mutation, variables)
user_id = result.user.id  # Typed object
```

**After (v1.9)**:

```python
result = await graphql_execute(mutation, variables)
user_id = result["user"]["id"]  # Dict
```

**Impact**: Update test assertions to use dict access.

**Migration**:

```python
# OLD
assert result.user.id == "123"
assert result.user.email == "test@example.com"

# NEW
assert result["user"]["id"] == "123"
assert result["user"]["email"] == "test@example.com"
```

#### 2. CASCADE at Success Level

CASCADE is now at success level (sibling to entity):

```python
# Access CASCADE
cascade = result["cascade"]

# NOT here
cascade = result["user"]["cascade"]  # ❌ Will be None/undefined
```

### Best Practices

1. **Use PascalCase for entity_type**: `'User'` not `'user'`
2. **Use direct entity format**: Don't wrap entity unnecessarily
3. **Enable strict mode in development**: `FRAISEQL_STRICT_MODE=1`

## For PostgreSQL Function Authors

### Two Formats Only

#### Simple Format (Recommended for simple operations)

```sql
CREATE FUNCTION get_user(input jsonb) RETURNS jsonb AS $$
BEGIN
    -- Just return entity JSONB
    RETURN (SELECT data FROM v_user WHERE id = (input->>'id')::uuid);
END;
$$ LANGUAGE plpgsql;
```

#### Full Format (For mutations with status/CASCADE)

```sql
CREATE FUNCTION create_user(input jsonb) RETURNS mutation_response AS $$
DECLARE
    new_user jsonb;
    cascade_data jsonb;
BEGIN
    -- Create user
    INSERT INTO users (name, email)
    VALUES (input->>'name', input->>'email')
    RETURNING to_jsonb(users.*) INTO new_user;

    -- Build CASCADE (if needed)
    cascade_data := jsonb_build_object(
        'updated', jsonb_build_array(...),
        'deleted', '[]'::jsonb,
        'invalidations', jsonb_build_array(...)
    );

    -- Return full format
    RETURN ROW(
        'created',      -- status
        'User created', -- message
        'User',         -- entity_type (PascalCase!)
        new_user,       -- entity
        NULL,           -- updated_fields
        cascade_data,   -- cascade
        NULL            -- metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql;
```

### Common Pitfalls

❌ **Wrong**: Lowercase entity_type

```sql
RETURN ROW(..., 'user', ...)::mutation_response;
```

✅ **Correct**: PascalCase entity_type

```sql
RETURN ROW(..., 'User', ...)::mutation_response;
```

## Testing Your Migration

### 1. Run Tests with Strict Mode

```bash
FRAISEQL_STRICT_MODE=1 pytest tests/
```

This will fail on validation errors (missing entity_type, etc.)

### 2. Update Test Assertions

Change typed object access to dict access:

```python
# Before
def test_create_user():
    result = await create_user(input)
    assert result.user.id == "123"
    if result.__cascade__:
        assert len(result.__cascade__.updated) > 0

# After
def test_create_user():
    result = await create_user(input)
    assert result["user"]["id"] == "123"
    if "cascade" in result:
        assert len(result["cascade"]["updated"]) > 0
```

### 3. Verify CASCADE Placement

```python
# Verify CASCADE is at success level
assert "cascade" in result
assert "cascade" not in result["user"]
```

## Rollback

If critical issues found, downgrade to v1.8.x:

```bash
pip install fraiseql==1.8.2
```

## Support

- Issues: https://github.com/fraiseql/fraiseql/issues
- Docs: https://fraiseql.dev/docs

```

**Acceptance Criteria**:
- [ ] Clear migration steps
- [ ] Breaking changes documented
- [ ] Common pitfalls covered
- [ ] Rollback instructions included

---

### Task 6.3: Examples

**File**: `examples/rust-pipeline/README.md` (NEW)

Create example directory and examples:

```bash
mkdir -p examples/rust-pipeline
```

**Content**:

```markdown
# Rust Pipeline Examples

## Simple Format Example

**Python Schema**:
```python
import fraiseql

@fraiseql.type
class User:
    id: str
    name: str
    email: str

@fraiseql.success
class GetUserSuccess:
    user: User
    message: str

@fraiseql.mutation(function="get_user")
class GetUser:
    input: GetUserInput
    success: GetUserSuccess
```

**PostgreSQL**:

```sql
CREATE FUNCTION get_user(input jsonb) RETURNS jsonb AS $$
BEGIN
    -- Simple format: just return entity
    RETURN (
        SELECT jsonb_build_object(
            'id', id::text,
            'name', name,
            'email', email
        )
        FROM users
        WHERE id = (input->>'id')::uuid
    );
END;
$$ LANGUAGE plpgsql;
```

**Usage**:

```python
result = await execute(schema, """
    mutation {
        getUser(input: {id: "123"}) {
            user {
                id
                name
                email
            }
            message
        }
    }
""")

# Access as dict
user = result.data["getUser"]["user"]
print(f"User: {user['name']}")
```

## Full Format with CASCADE Example

**Python Schema**:

```python
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str
    cascade: Cascade | None = None  # ← CASCADE field

@fraiseql.mutation(
    function="create_user",
    enable_cascade=True,  # ← Enable CASCADE
)
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserError
```

**PostgreSQL**:

```sql
CREATE FUNCTION create_user(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    new_user jsonb;
    cascade_data jsonb;
BEGIN
    -- Insert user
    INSERT INTO users (name, email)
    VALUES (input->>'name', input->>'email')
    RETURNING jsonb_build_object(
        'id', id::text,
        'name', name,
        'email', email
    ) INTO new_user;

    -- Build CASCADE
    cascade_data := jsonb_build_object(
        'updated', jsonb_build_array(
            jsonb_build_object(
                'type_name', 'User',
                'id', new_user->>'id',
                'operation', 'CREATED',
                'entity', new_user
            )
        ),
        'deleted', '[]'::jsonb,
        'invalidations', jsonb_build_array('users')
    );

    -- Return full format
    RETURN ROW(
        'created',           -- status
        'User created',      -- message
        'User',              -- entity_type
        new_user,            -- entity
        NULL,                -- updated_fields
        cascade_data,        -- cascade
        NULL                 -- metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql;
```

**Usage**:

```python
result = await execute(schema, """
    mutation {
        createUser(input: {name: "Alice", email: "alice@example.com"}) {
            user {
                id
                name
            }
            message
            cascade {
                updated {
                    typeName
                    id
                    operation
                }
                invalidations
            }
        }
    }
""")

# CASCADE at success level
cascade = result.data["createUser"]["cascade"]
print(f"Updated: {cascade['updated']}")
print(f"Invalidations: {cascade['invalidations']}")
```

## Error Handling Example

**PostgreSQL**:

```sql
CREATE FUNCTION create_user(input jsonb)
RETURNS mutation_response AS $$
BEGIN
    -- Validate email
    IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email') THEN
        RETURN ROW(
            'failed:validation',
            'Email already exists',
            NULL, NULL, NULL, NULL,
            jsonb_build_object(
                'errors', jsonb_build_array(
                    jsonb_build_object(
                        'field', 'email',
                        'code', 'duplicate',
                        'message', 'Email already exists'
                    )
                )
            )
        )::mutation_response;
    END IF;

    -- ... success path ...
END;
$$ LANGUAGE plpgsql;
```

**Usage**:

```python
result = await execute(schema, mutation)

# Check for errors
if result.data["createUser"]["__typename"] == "CreateUserError":
    error = result.data["createUser"]
    print(f"Error: {error['message']}")
    print(f"Code: {error['code']}")  # 422
    for err in error['errors']:
        print(f"  - {err['field']}: {err['message']}")
```

```

**Acceptance Criteria**:
- [ ] Simple format example complete
- [ ] Full format with CASCADE example complete
- [ ] Error handling example complete
- [ ] Examples are tested and working

---

### Task 6.4: Release Notes

**File**: `CHANGELOG.md` (UPDATE)

**Add section**:

```markdown
## [1.9.0] - 2025-XX-XX

### Changed

**Unified Rust Mutation Pipeline** - Major internal refactoring with no user-facing breaking changes.

#### Internal Changes

- Replaced 5-layer Python/Rust architecture with unified 2-layer Rust pipeline
- Deleted ~1300 lines of code (entity_flattener.py, parser.py)
- Single source of truth for mutation transformation in Rust
- Type-safe throughout Rust layer

#### Behavior Changes

- Non-HTTP mode now returns dicts instead of typed objects
  - Update tests: `result["user"]["id"]` instead of `result.user.id`
- CASCADE now at success level (sibling to entity, not nested)
  - Access: `result["cascade"]` not `result["user"]["cascade"]`

#### New Features

- Two simple formats: Simple (entity-only) and Full (mutation_response)
- Auto-detection of format based on status field
- Improved CASCADE handling (just another optional field)
- Better error messages and validation
- Strict mode for development: `FRAISEQL_STRICT_MODE=1`

#### Performance

- <1ms overhead for typical mutations
- <2ms overhead with CASCADE
- No performance regression

#### Migration

See `docs/guides/migrating_to_rust_pipeline.md` for full migration guide.

**For most users**: No changes needed, existing code works as-is.

**For tests**: Update to use dict access instead of object attributes.

### Fixed

- CASCADE placement bug (was sometimes nested in entity)
- __typename consistency issues
- Format detection edge cases
```

**Acceptance Criteria**:

- [ ] Release notes clear and concise
- [ ] Breaking changes highlighted
- [ ] Migration path documented

---

## Phase 6 Completion Checklist

- [ ] Task 6.1: Architecture docs complete
- [ ] Task 6.2: Migration guide complete
- [ ] Task 6.3: Examples complete and tested
- [ ] Task 6.4: Release notes written
- [ ] All docs reviewed for accuracy
- [ ] Examples tested and working
- [ ] Links to docs valid

**Verification**:

```bash
# Check docs exist
ls docs/architecture/mutation_pipeline.md
ls docs/guides/migrating_to_rust_pipeline.md
ls examples/rust-pipeline/README.md

# Test examples
cd examples/rust-pipeline
python3 test_examples.py  # If created

# Spell check (optional)
aspell check docs/architecture/mutation_pipeline.md
```

## Final Project Checklist

Before declaring the project complete:

- [ ] All 6 phases complete
- [ ] All tests passing (Rust + Python)
- [ ] Code coverage >95% Rust, >85% Python
- [ ] Documentation complete and accurate
- [ ] Examples tested and working
- [ ] Release notes written
- [ ] No known critical bugs
- [ ] Performance acceptable
- [ ] Ready for release

## Release Process

1. Create release branch: `git checkout -b release/v1.9.0`
2. Bump version in `pyproject.toml` and `Cargo.toml`
3. Run full test suite
4. Build and test package locally
5. Create PR for review
6. Merge to main
7. Tag release: `git tag v1.9.0`
8. Push tag: `git push origin v1.9.0`
9. Publish to PyPI
10. Announce release

Congratulations! 🎉
