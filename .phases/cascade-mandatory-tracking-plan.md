# Implementation Plan: Mandatory Cascade Tracking with Optional GraphQL Exposure

## Executive Summary

**Goal**: Make cascade tracking mandatory internally for all mutations, but keep GraphQL exposure optional and opt-in.

**Status**: Planning phase
**Estimated Complexity**: Medium (3-4 implementation units)
**Breaking Changes**: No (additive only)

## Problem Statement

Currently, FraiseQL has:
- ✅ Cascade infrastructure implemented (Rust layer, mutation_result_v2)
- ✅ Documentation and examples for cascade
- ❌ Cascade is opt-in via `enable_cascade=True`
- ❌ Inconsistent mutation return handling (v1 vs v2 format)
- ❌ Complex entity flattening logic to handle both formats

This creates:
- Unpredictable behavior (some mutations track cascade, others don't)
- Complex code paths for format detection
- Missed opportunities for audit trails and debugging

## Proposed Solution

### Core Principle
**All mutations track cascade data internally, but only expose it in GraphQL schema when explicitly requested.**

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ PostgreSQL Function Layer                                        │
│                                                                  │
│ ALL functions return mutation_result_v2 with cascade            │
│ - Even if cascade.updated = []                                  │
│ - Even if cascade.deleted = []                                  │
└─────────────────────────────────────────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ Rust Processing Layer                                           │
│                                                                  │
│ - Always parses mutation_result_v2                              │
│ - Always logs cascade data (for audit/debug)                    │
│ - Passes cascade to Python layer                                │
└─────────────────────────────────────────────────────────────────┘
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ Python GraphQL Schema Builder                                   │
│                                                                  │
│ @mutation(expose_cascade=False)  ← DEFAULT                      │
│   Success type: NO cascade field in schema                      │
│                                                                  │
│ @mutation(expose_cascade=True)   ← OPT-IN                       │
│   Success type: WITH cascade field in schema                    │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Python Decorator Changes (Minimal)

**Files to modify:**
- `src/fraiseql/mutations/mutation_decorator.py`

**Changes:**

1. Add `expose_cascade` parameter to `@mutation` decorator
   ```python
   def mutation(
       *,
       enable_cascade: bool = True,      # ALWAYS True (deprecated parameter)
       expose_cascade: bool = False,     # NEW: control GraphQL exposure
       description: str | None = None,
       deprecated: str | None = None,
   ):
       """
       Register a mutation with FraiseQL.

       Args:
           enable_cascade: DEPRECATED - Cascade tracking is now mandatory.
                          This parameter is ignored and will be removed in v2.0.
           expose_cascade: If True, adds cascade field to GraphQL Success type.
                          Default False for backward compatibility.
           description: GraphQL description for the mutation
           deprecated: Deprecation notice for the mutation
       """
       if not enable_cascade:
           warnings.warn(
               "enable_cascade=False is deprecated and ignored. "
               "Cascade tracking is now mandatory for all mutations. "
               "To hide cascade from GraphQL schema, use expose_cascade=False.",
               DeprecationWarning,
               stacklevel=2
           )

       # Store expose_cascade in mutation metadata
       ...
   ```

2. Pass `expose_cascade` to GraphQL schema builder
   ```python
   mutation_metadata = {
       'name': cls.__name__,
       'expose_cascade': expose_cascade,
       'input_type': input_type,
       'success_type': success_type,
       'error_type': error_type,
   }
   ```

**Verification:**
```bash
# Test that deprecation warning works
uv run pytest tests/unit/decorators/test_mutation_decorator.py -v -k deprecation

# Test that expose_cascade metadata is stored
uv run pytest tests/unit/decorators/test_mutation_decorator.py -v -k expose_cascade
```

**Acceptance Criteria:**
- [ ] `expose_cascade` parameter added with default `False`
- [ ] `enable_cascade=False` triggers deprecation warning
- [ ] Mutation metadata includes `expose_cascade` flag
- [ ] All existing tests pass (backward compatible)

---

### Phase 2: GraphQL Schema Builder Changes

**Files to modify:**
- `src/fraiseql/gql/builders/mutation_builder.py`

**Changes:**

1. Conditionally add cascade field to Success type based on `expose_cascade`
   ```python
   def build_mutation_success_type(
       success_class: Type,
       mutation_metadata: dict,
   ) -> GraphQLObjectType:
       """Build GraphQL Success type with optional cascade field."""

       fields = {}

       # Build fields from Success class annotations
       for field_name, field_type in get_type_hints(success_class).items():
           fields[field_name] = build_field(field_name, field_type)

       # Conditionally add cascade field
       if mutation_metadata.get('expose_cascade', False):
           fields['cascade'] = GraphQLField(
               CascadeDataType,  # Defined separately
               description="Entities updated or deleted by this mutation"
           )

       return GraphQLObjectType(
           name=f"{success_class.__name__}",
           fields=fields,
           description=success_class.__doc__
       )
   ```

2. Define `CascadeDataType` if not already defined
   ```python
   # Reusable cascade type for all mutations
   CascadeDataType = GraphQLObjectType(
       name="CascadeData",
       fields={
           'updated': GraphQLField(
               GraphQLList(GraphQLNonNull(EntityChangeType)),
               description="Entities that were updated as side effects"
           ),
           'deleted': GraphQLField(
               GraphQLList(GraphQLNonNull(EntityChangeType)),
               description="Entities that were deleted as side effects"
           ),
       },
       description="Side effect data from mutation execution"
   )

   EntityChangeType = GraphQLObjectType(
       name="EntityChange",
       fields={
           'type': GraphQLField(GraphQLString, description="Entity type name"),
           'id': GraphQLField(GraphQLString, description="Entity ID"),
           'data': GraphQLField(JSONScalar, description="Entity data"),
       },
       description="A single entity that was changed"
   )
   ```

**Verification:**
```bash
# Test cascade field not in schema by default
uv run pytest tests/integration/graphql/test_mutation_schema.py -v -k "not_exposed"

# Test cascade field in schema when expose_cascade=True
uv run pytest tests/integration/graphql/test_mutation_schema.py -v -k "exposed"
```

**Acceptance Criteria:**
- [ ] `expose_cascade=False`: No cascade field in GraphQL schema
- [ ] `expose_cascade=True`: Cascade field present in schema
- [ ] `CascadeDataType` defined and reusable
- [ ] Schema introspection tests pass

---

### Phase 3: Rust Layer - Always Parse Cascade

**Files to modify:**
- `fraiseql_rs/src/mutations/result_parser.rs` (or equivalent)

**Changes:**

1. Remove conditional cascade parsing - always parse it
   ```rust
   // OLD: Conditional parsing based on enable_cascade flag
   if mutation_config.enable_cascade {
       cascade = parse_cascade_data(&result_row)?;
   }

   // NEW: Always parse cascade (flag removed)
   let cascade = parse_cascade_data(&result_row)?;
   ```

2. Always log cascade for audit/debug purposes
   ```rust
   // Log cascade even if not exposed to GraphQL
   if !cascade.updated.is_empty() || !cascade.deleted.is_empty() {
       info!(
           "Mutation {} affected entities: updated={}, deleted={}",
           mutation_name,
           cascade.updated.len(),
           cascade.deleted.len()
       );
   }
   ```

3. Always include cascade in response JSON (Python can filter later)
   ```rust
   let response_json = json!({
       "status": status,
       "message": message,
       "entity": entity_data,
       "cascade": cascade,  // ALWAYS present
       "metadata": metadata,
   });
   ```

**Verification:**
```bash
# Run Rust tests
cd fraiseql_rs && cargo test mutation_result_parsing

# Run integration tests
uv run pytest tests/integration/rust/test_mutation_execution.py -v
```

**Acceptance Criteria:**
- [ ] Cascade data always parsed from mutation_result_v2
- [ ] Cascade logged for all mutations (even if empty)
- [ ] Cascade included in JSON response to Python layer
- [ ] No performance regression (cascade parsing is fast)

---

### Phase 4: Python Response Handler - Conditional Filtering

**Files to modify:**
- `src/fraiseql/mutations/response_handler.py` (or equivalent)

**Changes:**

1. Receive cascade data from Rust, but only include in GraphQL response if exposed
   ```python
   def build_graphql_response(
       mutation_result: dict,
       mutation_metadata: dict,
   ) -> dict:
       """Build GraphQL response, filtering cascade if not exposed."""

       response = {
           'status': mutation_result['status'],
           'message': mutation_result['message'],
       }

       # Entity data (if present)
       if 'entity' in mutation_result:
           response.update(mutation_result['entity'])

       # Cascade (only if exposed in schema)
       if mutation_metadata.get('expose_cascade', False):
           response['cascade'] = mutation_result['cascade']
       # else: cascade data is logged but not in GraphQL response

       return response
   ```

2. Keep cascade data in internal context for logging/audit
   ```python
   # Store cascade in request context for logging
   if mutation_result.get('cascade'):
       current_context().mutation_cascade = mutation_result['cascade']
   ```

**Verification:**
```bash
# Test that cascade not in response when expose_cascade=False
uv run pytest tests/integration/mutations/test_cascade_filtering.py -v

# Test that cascade in response when expose_cascade=True
uv run pytest tests/integration/mutations/test_cascade_exposure.py -v
```

**Acceptance Criteria:**
- [ ] Response includes cascade only when `expose_cascade=True`
- [ ] Cascade data available in logs regardless of exposure
- [ ] No cascade in GraphQL response when `expose_cascade=False`
- [ ] Tests verify both scenarios

---

### Phase 5: Documentation Updates

**Files to create/modify:**
- `docs/mutations/cascade-tracking.md` (NEW)
- `docs/mutations/status-strings.md` (UPDATE - add cascade section)
- `CHANGELOG.md` (UPDATE)
- `examples/cascade-tracking/` (NEW examples)

**Content:**

#### docs/mutations/cascade-tracking.md
```markdown
# Cascade Tracking in FraiseQL

## Overview

All FraiseQL mutations automatically track cascade effects:
- Entities updated as side effects
- Entities deleted as cascade deletes
- Logged for audit and debugging

## Internal vs External Cascade

### Internal Tracking (Always On)
All mutations track cascade data internally:
- Logged in application logs
- Available for audit trails
- Used for debugging
- No GraphQL schema changes

### External Exposure (Opt-In)
Mutations can optionally expose cascade in GraphQL:
```python
@mutation(expose_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

When exposed, Success type includes:
```graphql
type CreatePostSuccess {
    status: String!
    message: String!
    post: Post
    cascade: CascadeData!  # Only when expose_cascade=True
}
```

## When to Expose Cascade

**Expose cascade when:**
- Clients need to update UI for affected entities
- Building real-time features
- Implementing optimistic updates
- Advanced debugging tools

**Keep cascade internal when:**
- Simple CRUD operations
- Clients don't need side effect data
- Simpler GraphQL schema preferred

## PostgreSQL Function Format

All functions must return `mutation_result_v2` with cascade:

```sql
CREATE FUNCTION create_post(p_input JSONB)
RETURNS mutation_result_v2 AS $$
BEGIN
    -- Business logic
    INSERT INTO posts (title) VALUES (p_input->>'title');

    -- Return with cascade (even if empty)
    RETURN ROW(
        'created',
        'Post created',
        jsonb_build_object('post', row_to_json(posts.*)),
        jsonb_build_object('updated', '[]'::jsonb, 'deleted', '[]'::jsonb),
        'Post',
        post_id::text,
        '{}'::jsonb
    )::mutation_result_v2;
END;
$$ LANGUAGE plpgsql;
```

## Best Practices

### ✅ DO
- Always return cascade in PostgreSQL functions (even if empty)
- Log cascade data for audit trails
- Expose cascade for complex mutations with side effects
- Document when cascade is exposed in API docs

### ❌ DON'T
- Try to disable cascade tracking (it's mandatory)
- Expose cascade for every simple mutation (keep schemas simple)
- Forget to update GraphQL schema when exposing cascade
```

#### Update CHANGELOG.md
```markdown
## [Unreleased]

### Added
- **Mandatory Cascade Tracking**: All mutations now track cascade effects internally
  - Logged for audit and debugging
  - Optional GraphQL exposure via `expose_cascade=True`
  - No breaking changes (exposure defaults to `False`)

### Deprecated
- `enable_cascade` parameter in `@mutation` decorator
  - Cascade tracking is now mandatory
  - Parameter is ignored and will be removed in v2.0
  - Use `expose_cascade` to control GraphQL schema exposure

### Changed
- All mutations must return `mutation_result_v2` with cascade data
- Cascade data logged even when not exposed in GraphQL
```

**Verification:**
```bash
# Check docs exist and are properly formatted
ls -la docs/mutations/cascade-tracking.md
mdl docs/mutations/cascade-tracking.md  # markdown linter

# Check changelog updated
grep -i "cascade tracking" CHANGELOG.md
```

**Acceptance Criteria:**
- [ ] `cascade-tracking.md` created with comprehensive guide
- [ ] Examples show both exposed and non-exposed patterns
- [ ] CHANGELOG entry added
- [ ] Existing cascade docs updated for new behavior
- [ ] Migration guide for users upgrading

---

### Phase 6: Tests

**Files to create:**
- `tests/unit/decorators/test_cascade_exposure.py`
- `tests/integration/mutations/test_cascade_filtering.py`
- `tests/integration/graphql/test_cascade_schema.py`

#### Test 1: Decorator Parameter Tests
```python
# tests/unit/decorators/test_cascade_exposure.py

def test_expose_cascade_default_false():
    """Test that expose_cascade defaults to False."""
    @mutation
    class TestMutation:
        input: TestInput
        success: TestSuccess
        error: TestError

    metadata = get_mutation_metadata(TestMutation)
    assert metadata['expose_cascade'] is False

def test_expose_cascade_explicit_true():
    """Test that expose_cascade can be set to True."""
    @mutation(expose_cascade=True)
    class TestMutation:
        input: TestInput
        success: TestSuccess
        error: TestError

    metadata = get_mutation_metadata(TestMutation)
    assert metadata['expose_cascade'] is True

def test_enable_cascade_deprecation_warning():
    """Test that enable_cascade=False triggers warning."""
    with pytest.warns(DeprecationWarning, match="enable_cascade=False is deprecated"):
        @mutation(enable_cascade=False)
        class TestMutation:
            input: TestInput
            success: TestSuccess
            error: TestError
```

#### Test 2: GraphQL Schema Tests
```python
# tests/integration/graphql/test_cascade_schema.py

@pytest.mark.asyncio
async def test_cascade_not_in_schema_when_not_exposed(
    create_fraiseql_app_with_db,
    db_connection
):
    """Test cascade field not in schema when expose_cascade=False."""

    @mutation(expose_cascade=False)
    class CreatePost:
        input: CreatePostInput
        success: CreatePostSuccess
        error: CreatePostError

    app = create_fraiseql_app_with_db(mutations=[CreatePost])

    # Introspect schema
    schema = get_graphql_schema(app)
    success_type = schema.type_map['CreatePostSuccess']

    # Cascade field should NOT be present
    assert 'cascade' not in success_type.fields

@pytest.mark.asyncio
async def test_cascade_in_schema_when_exposed(
    create_fraiseql_app_with_db,
    db_connection
):
    """Test cascade field in schema when expose_cascade=True."""

    @mutation(expose_cascade=True)
    class CreatePost:
        input: CreatePostInput
        success: CreatePostSuccess
        error: CreatePostError

    app = create_fraiseql_app_with_db(mutations=[CreatePost])

    # Introspect schema
    schema = get_graphql_schema(app)
    success_type = schema.type_map['CreatePostSuccess']

    # Cascade field SHOULD be present
    assert 'cascade' in success_type.fields
    cascade_field = success_type.fields['cascade']
    assert cascade_field.type.name == 'CascadeData'
```

#### Test 3: Response Filtering Tests
```python
# tests/integration/mutations/test_cascade_filtering.py

@pytest.mark.asyncio
async def test_cascade_not_in_response_when_not_exposed(
    create_fraiseql_app_with_db,
    db_connection
):
    """Test cascade data not in GraphQL response when expose_cascade=False."""

    # Setup test data
    await db_connection.execute("""
        CREATE TABLE posts (id UUID PRIMARY KEY, title TEXT);
        CREATE FUNCTION create_post(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        BEGIN
            -- Return with cascade data
            RETURN ROW(
                'created',
                'Post created',
                '{"post": {"id": "123", "title": "Test"}}'::jsonb,
                '{"updated": [], "deleted": []}'::jsonb,  -- Cascade present
                'Post',
                '123',
                '{}'::jsonb
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    @mutation(expose_cascade=False)
    class CreatePost:
        input: CreatePostInput
        success: CreatePostSuccess
        error: CreatePostError

    app = create_fraiseql_app_with_db(mutations=[CreatePost])

    # Execute mutation
    response = await execute_graphql(app, """
        mutation {
            createPost(input: {title: "Test"}) {
                ... on CreatePostSuccess {
                    status
                    message
                    post { id title }
                }
            }
        }
    """)

    # Cascade should NOT be in response
    assert 'cascade' not in response['data']['createPost']
    # But cascade IS logged (check logs)

@pytest.mark.asyncio
async def test_cascade_in_response_when_exposed(
    create_fraiseql_app_with_db,
    db_connection
):
    """Test cascade data in GraphQL response when expose_cascade=True."""

    # Same setup as above

    @mutation(expose_cascade=True)
    class CreatePost:
        input: CreatePostInput
        success: CreatePostSuccess
        error: CreatePostError

    app = create_fraiseql_app_with_db(mutations=[CreatePost])

    # Execute mutation with cascade in query
    response = await execute_graphql(app, """
        mutation {
            createPost(input: {title: "Test"}) {
                ... on CreatePostSuccess {
                    status
                    message
                    post { id title }
                    cascade {
                        updated { type id }
                        deleted { type id }
                    }
                }
            }
        }
    """)

    # Cascade SHOULD be in response
    assert 'cascade' in response['data']['createPost']
    assert response['data']['createPost']['cascade'] == {
        'updated': [],
        'deleted': []
    }
```

**Verification:**
```bash
# Run all cascade tests
uv run pytest tests/ -v -k cascade

# Run specific test suites
uv run pytest tests/unit/decorators/test_cascade_exposure.py -v
uv run pytest tests/integration/graphql/test_cascade_schema.py -v
uv run pytest tests/integration/mutations/test_cascade_filtering.py -v
```

**Acceptance Criteria:**
- [ ] All decorator tests pass
- [ ] Schema introspection tests pass
- [ ] Response filtering tests pass
- [ ] Tests cover both expose_cascade=True and False
- [ ] Deprecation warning test passes

---

## Migration Path for Users

### Current State (Before)
```python
# Some mutations track cascade, others don't
@mutation(enable_cascade=True)  # Opt-in
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError

@mutation  # No cascade
class SimpleUpdate:
    input: SimpleInput
    success: SimpleSuccess
    error: SimpleError
```

### New State (After)
```python
# All mutations track cascade internally
# But GraphQL exposure is opt-in

@mutation(expose_cascade=True)  # Expose in GraphQL
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess  # Has cascade field
    error: CreatePostError

@mutation  # Cascade tracked but not exposed (default)
class SimpleUpdate:
    input: SimpleInput
    success: SimpleSuccess  # NO cascade field
    error: SimpleError
```

### Breaking Changes
**NONE** - This is a fully backward-compatible change:
- Existing mutations without `enable_cascade=True` work as before
- Cascade data is tracked internally but not exposed
- GraphQL schemas unchanged unless explicitly opted in

### Deprecation Timeline
- **v1.8.0**: Add `expose_cascade` parameter, deprecate `enable_cascade`
- **v1.9.0**: Warn if `enable_cascade=False` is used
- **v2.0.0**: Remove `enable_cascade` parameter entirely

---

## Rollout Strategy

### Week 1: Foundation
- Implement Phase 1 (Decorator changes)
- Implement Phase 2 (Schema builder)
- Write unit tests

### Week 2: Integration
- Implement Phase 3 (Rust layer)
- Implement Phase 4 (Response handler)
- Write integration tests

### Week 3: Documentation
- Implement Phase 5 (Docs)
- Write examples
- Update CHANGELOG

### Week 4: QA & Release
- Full test suite run
- Performance testing
- Documentation review
- Release v1.8.0

---

## Success Metrics

### Technical Metrics
- [ ] 100% of mutations return mutation_result_v2
- [ ] Zero additional latency from cascade tracking
- [ ] All tests pass (unit + integration)
- [ ] No breaking changes detected

### User Experience Metrics
- [ ] Clear documentation for expose_cascade
- [ ] Examples cover common use cases
- [ ] Migration path documented
- [ ] Deprecation warnings clear and actionable

### Code Quality Metrics
- [ ] Remove entity flattener complexity (if no longer needed)
- [ ] Single code path for mutation result handling
- [ ] Reduced conditional logic in schema builder

---

## Risk Assessment

### Low Risk
- ✅ Additive changes only (no breaking changes)
- ✅ Default behavior unchanged (expose_cascade=False)
- ✅ Existing cascade functionality already tested

### Medium Risk
- ⚠️ Rust layer changes (need careful testing)
- ⚠️ Schema builder changes (need introspection tests)

### Mitigation
- Comprehensive test coverage (unit + integration)
- Feature flag for rollout (if needed)
- Detailed logging for debugging

---

## Alternatives Considered

### Alternative 1: Make Cascade Always Exposed
**Pros**: Simpler (one code path)
**Cons**: Breaking change, clutters simple mutations
**Decision**: ❌ Rejected - too aggressive

### Alternative 2: Keep Cascade Fully Optional
**Pros**: No changes needed
**Cons**: Inconsistent behavior, complex code
**Decision**: ❌ Rejected - current problems persist

### Alternative 3: Two Mutation Types
**Pros**: Clear distinction
**Cons**: Confusing for users, duplication
**Decision**: ❌ Rejected - too complex

### Selected: Mandatory Tracking, Optional Exposure
**Pros**: Internal consistency + external simplicity
**Cons**: None significant
**Decision**: ✅ Selected - best of all worlds

---

## Open Questions

1. **Q**: Should we auto-expose cascade for mutations with `enable_cascade=True`?
   **A**: Yes, for backward compatibility during deprecation period.

2. **Q**: Should empty cascade arrays be logged?
   **A**: No, only log when cascade has data (avoid log spam).

3. **Q**: Should cascade be available in error responses?
   **A**: No, errors don't have cascade data (operation failed).

4. **Q**: Can we remove entity flattener after this?
   **A**: Maybe - needs separate analysis after implementation.

---

## Completion Checklist

### Implementation
- [ ] Phase 1: Decorator changes
- [ ] Phase 2: Schema builder
- [ ] Phase 3: Rust layer
- [ ] Phase 4: Response handler
- [ ] Phase 5: Documentation
- [ ] Phase 6: Tests

### Quality Assurance
- [ ] All tests pass
- [ ] No performance regression
- [ ] Documentation complete
- [ ] Examples working

### Release Preparation
- [ ] CHANGELOG updated
- [ ] Migration guide written
- [ ] Deprecation warnings tested
- [ ] Version number decided

---

## Appendix: File Checklist

### Python Files Modified
- [ ] `src/fraiseql/mutations/mutation_decorator.py`
- [ ] `src/fraiseql/gql/builders/mutation_builder.py`
- [ ] `src/fraiseql/mutations/response_handler.py`

### Rust Files Modified
- [ ] `fraiseql_rs/src/mutations/result_parser.rs`

### Documentation Files
- [ ] `docs/mutations/cascade-tracking.md` (NEW)
- [ ] `docs/mutations/status-strings.md` (UPDATE)
- [ ] `CHANGELOG.md` (UPDATE)

### Test Files
- [ ] `tests/unit/decorators/test_cascade_exposure.py` (NEW)
- [ ] `tests/integration/mutations/test_cascade_filtering.py` (NEW)
- [ ] `tests/integration/graphql/test_cascade_schema.py` (NEW)

### Example Files
- [ ] `examples/cascade-tracking/` (NEW directory)
- [ ] `examples/cascade-tracking/README.md` (NEW)
- [ ] `examples/cascade-tracking/main.py` (NEW)

---

**Plan Status**: ✅ Ready for Implementation
**Next Step**: Begin Phase 1 - Python Decorator Changes
