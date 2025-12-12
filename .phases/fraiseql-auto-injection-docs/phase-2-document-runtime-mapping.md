# Phase 2: Document Runtime Field Mapping

**Objective**: Create comprehensive documentation about database → GraphQL field mappings

**Priority**: P0 - HIGH (Critical for understanding)
**Estimated Time**: 2 hours
**Dependencies**: Phase 1 complete
**Phase Type**: Documentation Update

---

## Context

**Current State**:
- No documentation about `entity_id` → `id` mapping
- Users don't understand when fields appear in responses vs when defined in types
- Confusion about snake_case → camelCase transformations

**User Impact**:
- Users define fields that shouldn't be defined (like `id`)
- Confusion about GraphQL schema vs runtime behavior
- Unexpected fields in responses (or expected fields missing)

**Target State**:
- Clear documentation of all runtime field mappings
- Visual examples showing database → Python → GraphQL transformation
- Guidelines on which fields to define vs which are runtime-mapped

---

## Files to Create

### Primary Documentation File

**Location**: `docs/mutations/runtime-mapping.md` (create new)

---

## Implementation Steps

### Step 1: Create Runtime Mapping Guide

**File**: `docs/mutations/runtime-mapping.md`

**Content**:
```markdown
# Runtime Field Mapping

FraiseQL's Rust layer automatically transforms certain database fields into
GraphQL-friendly formats when building mutation responses. This happens at
**runtime** in the response JSON, not in your Python type definitions.

## Overview

| Database Field | Python Type Field | GraphQL Response Field | Transformation |
|----------------|-------------------|------------------------|----------------|
| `entity_id` | ❌ Not defined | `id` | TEXT → String, renamed |
| `updated_fields` | ❌ Not defined | `updatedFields` | snake_case → camelCase |
| `entity` | Parsed to entity object | Entity object | JSONB → Python class |
| `cascade` | Parsed to Cascade object | `cascade` | JSONB → Cascade class |

**Key Point**: Fields marked "Not defined" should NOT be defined in your Python
types. They're added to the GraphQL response automatically.

## How It Works

### The Full Pipeline

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Database    │────>│  Python      │────>│  Rust        │────>│  GraphQL     │
│  Function    │     │  Resolver    │     │  Builder     │     │  Response    │
└──────────────┘     └──────────────┘     └──────────────┘     └──────────────┘
      │                      │                     │                    │
      v                      v                     v                    v
Returns:              Returns:             Transforms:          Client sees:
- status              - status             - entity_id→id       - status
- entity_id (UUID)    - message            - updated→Updated    - message
- entity (JSONB)      - entity obj         - snake→camel        - id (!)
- updated_fields      - cascade obj        - builds JSON        - updatedFields (!)
- cascade (JSONB)                                                - user/entity
```

### Stage 1: Database Returns Composite Type

Your PostgreSQL function returns `mutation_response`:

```sql
CREATE TYPE mutation_response AS (
    status          TEXT,
    message         TEXT,
    entity_id       TEXT,        -- ← UUID as TEXT
    entity_type     TEXT,
    entity          JSONB,       -- ← Entity data
    updated_fields  TEXT[],      -- ← Array of field names
    cascade         JSONB,       -- ← Cascade data
    metadata        JSONB
);

-- Example return value
RETURN ROW(
    'success',                   -- status
    'User created',              -- message
    '123e4567-...'::TEXT,        -- entity_id
    'User',                      -- entity_type
    '{"id": "...", "name": "John"}',  -- entity
    ARRAY['name', 'email'],      -- updated_fields
    NULL,                        -- cascade
    '{}'::JSONB                  -- metadata
)::mutation_response;
```

### Stage 2: Python Resolver Parses Data

Your resolver parses the composite type:

```python
@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
    cascade: Cascade | None = None

async def create_user_resolver(info, input):
    # Call database function
    result = await db.fetch_one("SELECT * FROM create_user(...)")

    # Parse composite type fields
    mutation_result = parse_mutation_response(result)

    # Build response object
    return CreateUserSuccess(
        status=mutation_result.status,        # "success"
        message=mutation_result.message,      # "User created"
        user=User(**mutation_result.entity),  # Parse JSONB to User
        cascade=parse_cascade(mutation_result.cascade)
    )
    # Note: entity_id and updated_fields NOT used here!
```

**Important**:
- `entity_id` is NOT passed to the success object
- `updated_fields` is NOT passed to the success object
- These will be added by Rust layer in next stage

### Stage 3: Rust Response Builder Transforms

FraiseQL's Rust layer receives the Python object and enhances it:

**Source Code Reference** (`fraiseql_rs/src/mutation/response_builder.rs`):

```rust
// Add id from entity_id if present
if let Some(ref entity_id) = result.entity_id {
    obj.insert("id".to_string(), json!(entity_id));
}

// Add updatedFields (convert to camelCase)
if let Some(fields) = &result.updated_fields {
    let transformed_fields: Vec<Value> = fields
        .iter()
        .map(|f| json!(to_camel_case(f)))
        .collect();
    obj.insert("updatedFields".to_string(), json!(transformed_fields));
}
```

**What Rust adds**:
- `id` field (from `entity_id`)
- `updatedFields` field (from `updated_fields`, camelCased)
- `__typename` field
- Proper field ordering

### Stage 4: GraphQL Response to Client

The final JSON response includes all fields:

```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "status": "success",
      "message": "User created",
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "updatedFields": ["name", "email"],
      "user": {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "name": "John",
        "email": "john@example.com"
      },
      "cascade": null,
      "errors": null
    }
  }
}
```

Notice:
- `id` field present (mapped from `entity_id`)
- `updatedFields` present (mapped from `updated_fields`)
- These weren't in Python type definition!

## Field-by-Field Reference

### entity_id → id

**Database**: Returns UUID as TEXT in `entity_id` field

**Python**: Does NOT define `id` field

**Rust**: Adds `id` field to response JSON

**GraphQL Schema**:
```graphql
type CreateUserSuccess {
  id: ID  # ← Appears in schema automatically
  # ...
}
```

**Why**:
- Database uses descriptive name `entity_id`
- GraphQL conventions prefer simple `id`
- Rust handles the transformation

### updated_fields → updatedFields

**Database**: Returns TEXT[] array in `updated_fields`

**Python**: Does NOT define `updatedFields` field

**Rust**: Adds `updatedFields` array with camelCased field names

**GraphQL Schema**:
```graphql
type CreateUserSuccess {
  updatedFields: [String]  # ← Appears automatically
  # ...
}
```

**Why**:
- Database uses snake_case: `updated_fields`
- GraphQL conventions prefer camelCase: `updatedFields`
- Rust handles both the rename and field name transformations

**Example**:
```
Database:  updated_fields = ['first_name', 'last_name', 'email_address']
GraphQL:   updatedFields = ["firstName", "lastName", "emailAddress"]
```

### entity → Entity Object

**Database**: Returns JSONB with entity data

**Python**: Parses JSONB and creates typed object

**Rust**: Serializes object to JSON

**GraphQL**: Returns entity as nested object

**This one IS handled in Python**:
```python
user = User(**mutation_result.entity)  # Parse JSONB to User object
```

### cascade → cascade

**Database**: Returns JSONB with cascade data

**Python**: Parses JSONB into Cascade object

**Rust**: Serializes Cascade object

**GraphQL**: Returns cascade as nested object

**This one IS also handled in Python**:
```python
cascade = Cascade(**mutation_result.cascade) if mutation_result.cascade else None
```

## What You Should Define in Python

### ✅ DO Define These Fields

**In your success/error types**:
```python
@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    # ✅ Entity field (your domain object)
    user: User

    # ✅ Cascade field (if using cascade)
    cascade: Cascade | None = None

    # ✅ Error-specific fields
    # (for error types only)
```

**In your base class** (if using one):
```python
class MutationResultBase:
    # ✅ Standard mutation fields
    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None
```

### ❌ DO NOT Define These Fields

**Never define these** (Rust adds them):
```python
@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    id: UUID  # ❌ DON'T DEFINE - runtime mapped
    entity_id: UUID  # ❌ DON'T DEFINE - internal field
    updatedFields: list[str]  # ❌ DON'T DEFINE - runtime mapped
    updated_fields: list[str]  # ❌ DON'T DEFINE - internal field
```

**Why**: These fields are added by FraiseQL's response builder from the database
`mutation_response` composite type. Defining them in Python will cause conflicts.

## Debugging Runtime Mappings

### Verify Response Structure

**Test your mutation**:
```graphql
mutation TestCreate($input: CreateUserInput!) {
  createUser(input: $input) {
    __typename
    status
    message
    id                 # ← Should appear (runtime-mapped)
    updatedFields      # ← Should appear (runtime-mapped)
    user {
      id
      name
    }
    errors { code message }
  }
}
```

**Check response JSON**:
- `id` field should be present (from database `entity_id`)
- `updatedFields` should be present (from database `updated_fields`)
- Field names should be camelCase in GraphQL response

### Common Issues

**Issue**: `id` field is missing from response

**Possible Causes**:
1. Database function didn't return `entity_id` in `mutation_response`
2. `entity_id` is NULL (should be populated for success cases)
3. FraiseQL version doesn't support runtime mapping (upgrade to v1.8.0+)

**Fix**: Ensure database function populates `entity_id` field

---

**Issue**: `updatedFields` is missing from response

**Possible Causes**:
1. Database function didn't return `updated_fields`
2. `updated_fields` is NULL or empty array
3. This is a create mutation (updated_fields only relevant for updates)

**Fix**: For update mutations, populate `updated_fields` in database function

---

**Issue**: Fields appear in Python but not in GraphQL

**Cause**: Fields defined in Python but not populated by resolver

**Fix**: Ensure resolver populates all defined fields

## GraphQL Schema Generation

The GraphQL schema automatically includes runtime-mapped fields:

```graphql
type CreateUserSuccess {
  # From MutationResultBase (explicit)
  status: String!
  message: String
  errors: [Error]

  # Runtime-mapped (not in Python definition)
  id: ID
  updatedFields: [String]

  # From CreateUserSuccess (explicit)
  user: User!
  cascade: Cascade
}
```

**Note**: Schema introspection will show `id` and `updatedFields` even though
they're not defined in Python types. This is correct behavior.

## See Also

- [Auto-Injection Guide](./auto-injection.md) - What decorators auto-inject
- [Mutation Patterns](./patterns.md) - Complete examples
- [Database Integration](./database-integration.md) - mutation_response type
```

---

### Step 2: Add Visual Diagrams

**File**: `docs/mutations/runtime-mapping-diagrams.md`

**Content**:
```markdown
# Runtime Mapping Visual Guide

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     PostgreSQL Function                         │
│                                                                 │
│  CREATE FUNCTION create_user(...) RETURNS mutation_response    │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ Returns composite type:
                         │ (status, message, entity_id, entity_type,
                         │  entity, updated_fields, cascade, metadata)
                         │
                         v
┌─────────────────────────────────────────────────────────────────┐
│                     Python Resolver                             │
│                                                                 │
│  mutation_result = parse_mutation_response(db_row)             │
│                                                                 │
│  return CreateUserSuccess(                                     │
│      status=mutation_result.status,                            │
│      message=mutation_result.message,                          │
│      user=User(**mutation_result.entity),                      │
│      cascade=Cascade(**mutation_result.cascade)                │
│  )                                                             │
│                                                                 │
│  Note: entity_id and updated_fields NOT passed!                │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ Python object passed to Rust
                         │
                         v
┌─────────────────────────────────────────────────────────────────┐
│                  Rust Response Builder                          │
│                                                                 │
│  obj = serialize_python_object(success_obj)                    │
│                                                                 │
│  // Add id from entity_id                                      │
│  obj.insert("id", entity_id)                                   │
│                                                                 │
│  // Add updatedFields from updated_fields (camelCase)          │
│  obj.insert("updatedFields", camel_case(updated_fields))       │
│                                                                 │
│  // Add __typename                                             │
│  obj.insert("__typename", "CreateUserSuccess")                 │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ JSON response
                         │
                         v
┌─────────────────────────────────────────────────────────────────┐
│                     GraphQL Response                            │
│                                                                 │
│  {                                                             │
│    "createUser": {                                             │
│      "__typename": "CreateUserSuccess",                        │
│      "status": "success",                                      │
│      "message": "User created",                                │
│      "id": "123e4567...",           ← Added by Rust           │
│      "updatedFields": ["name"],     ← Added by Rust           │
│      "user": { ... },                                         │
│      "cascade": null                                           │
│    }                                                           │
│  }                                                             │
└─────────────────────────────────────────────────────────────────┘
```

## Field Transformation Examples

### Example 1: Create Mutation (Success)

**Database Output**:
```
status: "success"
message: "User created successfully"
entity_id: "a1b2c3d4-..."
entity: {"id": "a1b2c3d4...", "name": "John", "email": "john@example.com"}
updated_fields: NULL  (no fields updated, this is a create)
cascade: NULL
```

**Python Type Definition**:
```python
@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
    cascade: Cascade | None = None
```

**Python Resolver Returns**:
```python
CreateUserSuccess(
    status="success",
    message="User created successfully",
    user=User(id="a1b2c3d4...", name="John", email="john@example.com"),
    cascade=None
)
```

**Rust Transforms To**:
```json
{
  "__typename": "CreateUserSuccess",
  "status": "success",
  "message": "User created successfully",
  "id": "a1b2c3d4-...",      ← Added from entity_id
  "updatedFields": null,      ← Null (create, not update)
  "user": {
    "id": "a1b2c3d4-...",
    "name": "John",
    "email": "john@example.com"
  },
  "cascade": null,
  "errors": null
}
```

### Example 2: Update Mutation (Success)

**Database Output**:
```
status: "success"
message: "User updated"
entity_id: "a1b2c3d4-..."
entity: {"id": "a1b2c3d4...", "name": "Jane", "email": "jane@example.com"}
updated_fields: ["name", "email_address"]
cascade: {"updated": [...]}
```

**Python Resolver Returns**:
```python
UpdateUserSuccess(
    status="success",
    message="User updated",
    user=User(id="a1b2c3d4...", name="Jane", email="jane@example.com"),
    cascade=Cascade(updated=[...])
)
```

**Rust Transforms To**:
```json
{
  "__typename": "UpdateUserSuccess",
  "status": "success",
  "message": "User updated",
  "id": "a1b2c3d4-...",
  "updatedFields": ["name", "emailAddress"],  ← Camel-cased!
  "user": {
    "id": "a1b2c3d4-...",
    "name": "Jane",
    "email": "jane@example.com"
  },
  "cascade": {
    "updated": [...]
  },
  "errors": null
}
```

**Note the transformation**:
- `"name"` → `"name"` (already camelCase)
- `"email_address"` → `"emailAddress"` (snake_case → camelCase)

### Example 3: Error Response

**Database Output**:
```
status: "noop:already_exists"
message: "User with this email already exists"
entity_id: "existing-user-id"  (conflict entity ID)
entity: NULL
updated_fields: NULL
cascade: NULL
```

**Python Resolver Returns**:
```python
CreateUserError(
    status="noop:already_exists",
    message="User with this email already exists",
    errors=[Error(code=409, message="Email conflict", identifier="email")],
    conflict_user=User(...)  # Retrieved separately
)
```

**Rust Transforms To**:
```json
{
  "__typename": "CreateUserError",
  "status": "noop:already_exists",
  "message": "User with this email already exists",
  "id": "existing-user-id",  ← ID of conflicting entity
  "errors": [{
    "code": 409,
    "message": "Email conflict",
    "identifier": "email"
  }],
  "conflictUser": {
    "id": "existing-user-id",
    "name": "Existing User",
    "email": "conflict@example.com"
  }
}
```
```

---

## Verification Steps

### Verification 1: Documentation Build

**Command**:
```bash
mkdocs build --strict
```

**Expected Output**: Build completes without errors

---

### Verification 2: Diagram Rendering

**Manual Review**:
- [ ] ASCII diagrams render correctly in documentation
- [ ] Code examples have proper syntax highlighting
- [ ] Tables are formatted correctly
- [ ] All links work

---

### Verification 3: Technical Accuracy

**Review with Framework Maintainer**:
- [ ] Rust code references are accurate
- [ ] Field transformations are correct
- [ ] Examples match actual behavior
- [ ] No misleading statements

---

## Acceptance Criteria

**Must Have**:
- [ ] Runtime mapping documentation page created
- [ ] All field transformations documented
- [ ] Visual data flow diagram included
- [ ] Field-by-field reference complete
- [ ] Multiple examples provided
- [ ] "Do define" vs "Don't define" lists clear
- [ ] Debugging section included

**Success Metrics**:
- Users understand why NOT to define `id` field
- Clear distinction between Python types and runtime behavior
- Visual diagrams enhance understanding

---

## Next Phase

- **Phase 3**: Provide complete reference implementation
- **Phase 4**: Document base class patterns

---

**Phase Owner**: FraiseQL Documentation Team
**Estimated Completion**: 2 hours
**Status**: Ready for Implementation (after Phase 1)
