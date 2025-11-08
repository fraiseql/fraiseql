# PostgreSQL Comments → GraphQL Descriptions - Implementation Plan

**Complexity**: Complex | **Phased TDD Approach**

## Executive Summary

Enhance AutoFraiseQL to automatically use PostgreSQL table/view/function comments and column comments as GraphQL schema descriptions. This eliminates documentation duplication and establishes the database as the single source of truth for type/field documentation.

**Current State**: PostgreSQL comments are captured during introspection but not used in GraphQL schema generation.

**Goal**: Automatically populate GraphQL type descriptions and field descriptions from PostgreSQL `COMMENT ON` statements.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ PostgreSQL Database                                              │
│                                                                  │
│ COMMENT ON VIEW app.v_users IS 'User profile data';            │
│ COMMENT ON COLUMN app.v_users.email IS 'Primary email';        │
│ COMMENT ON FUNCTION app.fn_create_user(...) IS 'Creates user'; │
│ COMMENT ON TYPE app.type_create_user_input IS 'Input params';  │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ PostgresIntrospector (ALREADY CAPTURES)                         │
│ - ViewMetadata.comment                                          │
│ - ColumnInfo.comment                                            │
│ - FunctionMetadata.comment                                      │
│ - CompositeAttribute.comment                                    │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ Generators (NEW: USE COMMENTS)                                  │
│ - TypeGenerator: view_metadata.comment → __doc__               │
│ - TypeGenerator: column.comment → field.description            │
│ - MutationGenerator: function_metadata.comment → __doc__       │
│ - InputGenerator: composite_attr.comment → field.description   │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ GraphQL Schema (ALREADY SUPPORTS)                               │
│ - GraphQLObjectType(description=typ.__doc__)                    │
│ - GraphQLField(description=field.description)                   │
└─────────────────────────────────────────────────────────────────┘
```

## Priority Hierarchy (All Phases)

For all description fields, use this precedence:

1. **Explicit annotation** (e.g., `@fraiseql:type description="Manual override"`)
2. **PostgreSQL comment** (e.g., `COMMENT ON VIEW ...`)
3. **Auto-generated fallback** (e.g., "Auto-generated from v_users")

## PHASES

---

### Phase 1: Type-Level Descriptions (Views → GraphQL Types)

**Objective**: Use PostgreSQL view comments as GraphQL type descriptions

**Scope**: `TypeGenerator` only - simplest case with no field-level complexity

#### TDD Cycle 1.1: Add Test for View Comment → Type Description

1. **RED**: Write failing test for view comment usage
   - Test file: `tests/unit/introspection/test_type_generator.py`
   - Expected failure: Test expects `view_metadata.comment` to be used in `__doc__`

2. **GREEN**: Implement minimal code to pass
   - Files to modify: `src/fraiseql/introspection/type_generator.py` (lines 66-67)
   - Minimal implementation: Change `__doc__` assignment to use `view_metadata.comment`

3. **REFACTOR**: Ensure priority hierarchy is correct
   - Code improvements: Add proper fallback chain (annotation → comment → auto-generated)
   - Pattern compliance: Follow existing code style

4. **QA**: Verify phase completion
   - [ ] Unit test passes
   - [ ] Integration test with real database
   - [ ] No regression in existing tests
   - [ ] Documentation string cleaned properly (whitespace, indentation)

**Test Code** (RED):
```python
def test_view_comment_used_as_type_description(self, type_generator):
    """Test that PostgreSQL view comments become GraphQL type descriptions."""
    # Arrange
    view_metadata = ViewMetadata(
        schema_name="public",
        view_name="v_users",
        definition="SELECT * FROM users",
        comment="User profile data with contact information",  # PostgreSQL comment
        columns={}
    )
    annotation = TypeAnnotation()  # No explicit description

    # Act
    cls = type_generator._create_class(view_metadata, annotation, {})

    # Assert
    assert cls.__doc__ == "User profile data with contact information"
```

**Implementation** (GREEN):
```python
# src/fraiseql/introspection/type_generator.py (lines 66-67)
"__doc__": (
    annotation.description  # Priority 1: Explicit annotation
    or view_metadata.comment  # Priority 2: PostgreSQL comment (NEW)
    or f"Auto-generated from {view_metadata.view_name}"  # Priority 3: Fallback
),
```

---

### Phase 2: Mutation-Level Descriptions (Functions → GraphQL Mutations)

**Objective**: Use PostgreSQL function comments as GraphQL mutation descriptions

**Scope**: `MutationGenerator` only

#### TDD Cycle 2.1: Add Test for Function Comment → Mutation Description

1. **RED**: Write failing test for function comment usage
   - Test file: `tests/unit/introspection/test_mutation_generator.py`
   - Expected failure: Test expects `function_metadata.comment` to be used in `__doc__`

2. **GREEN**: Implement minimal code to pass
   - Files to modify: `src/fraiseql/introspection/mutation_generator.py` (lines 191-193)
   - Minimal implementation: Change `__doc__` assignment to use `function_metadata.comment`

3. **REFACTOR**: Ensure consistency with Phase 1 priority hierarchy
   - Code improvements: Match the same fallback pattern as TypeGenerator
   - Pattern compliance: Consistent comment handling across generators

4. **QA**: Verify phase completion
   - [ ] Unit test passes
   - [ ] Integration test with real database function
   - [ ] No regression in existing mutation tests
   - [ ] Consistent with Phase 1 implementation

**Test Code** (RED):
```python
def test_function_comment_used_as_mutation_description(self, mutation_generator):
    """Test that PostgreSQL function comments become GraphQL mutation descriptions."""
    # Arrange
    function_metadata = FunctionMetadata(
        schema_name="app",
        function_name="fn_create_user",
        parameters=[],
        return_type="jsonb",
        comment="Creates a new user account with email verification",  # PostgreSQL comment
        language="plpgsql"
    )
    annotation = MutationAnnotation(
        success_type="User",
        failure_type="UserError"
    )  # No explicit description

    # Act
    mutation_cls = mutation_generator._create_mutation_class(
        function_metadata, annotation, InputCls, SuccessCls, FailureCls
    )

    # Assert
    assert mutation_cls.__doc__ == "Creates a new user account with email verification"
```

**Implementation** (GREEN):
```python
# src/fraiseql/introspection/mutation_generator.py (lines 191-193)
"__doc__": (
    annotation.description  # Priority 1: Explicit annotation
    or function_metadata.comment  # Priority 2: PostgreSQL comment (NEW)
    or f"Auto-generated mutation from {function_metadata.function_name}"  # Priority 3: Fallback
),
```

---

### Phase 3: Input Field Descriptions (Composite Types → GraphQL Input Fields)

**Objective**: Use PostgreSQL composite type attribute comments as GraphQL input field descriptions

**Scope**: `InputGenerator` only - generate input types with field descriptions

#### TDD Cycle 3.1: Store Attribute Comments During Introspection

1. **RED**: Write test that expects composite attributes to have descriptions
   - Test file: `tests/unit/introspection/test_input_generator.py`
   - Expected failure: Generated input class should have field descriptions from `attr.comment`

2. **GREEN**: Store attribute comments in generated input fields
   - Files to modify: `src/fraiseql/introspection/input_generator.py` (around line 177)
   - Minimal implementation: Store `attr.comment` in field metadata

3. **REFACTOR**: Create proper field descriptor objects with descriptions
   - Code improvements: Use `FraiseQLField` or similar structure to carry description
   - Pattern compliance: Follow how TypeGenerator will handle field descriptions

4. **QA**: Verify phase completion
   - [ ] Unit test passes
   - [ ] Input fields carry description metadata
   - [ ] Ready for Phase 4 (field-level GraphQL conversion)

**Challenge**: Input classes are created with `type()` and only `__annotations__`. We need to attach field descriptions somehow.

**Solution Options**:
1. Store descriptions in a class-level dict: `__field_descriptions__ = {"email": "User email address"}`
2. Use `FraiseQLField` descriptors instead of plain annotations
3. Store in `__gql_fields__` dict (like output types do)

**Recommended**: Option 3 - use `__gql_fields__` for consistency with output types.

**Test Code** (RED):
```python
async def test_composite_attribute_comments_stored(self, input_generator, mock_introspector):
    """Test that composite type attribute comments are captured in input fields."""
    # Arrange
    mock_composite = CompositeTypeMetadata(
        schema_name="app",
        type_name="type_create_user_input",
        attributes=[
            CompositeAttribute(
                name="email",
                pg_type="text",
                ordinal_position=1,
                comment="Primary email address for authentication"  # PostgreSQL comment
            ),
            CompositeAttribute(
                name="name",
                pg_type="text",
                ordinal_position=2,
                comment="Full name of the user"
            )
        ],
        comment="Input parameters for user creation"
    )
    mock_introspector.discover_composite_type = AsyncMock(return_value=mock_composite)

    # Act
    input_cls = await input_generator._generate_from_composite_type(
        "type_create_user_input", "app", mock_introspector
    )

    # Assert
    assert hasattr(input_cls, "__gql_fields__")
    assert "email" in input_cls.__gql_fields__
    assert input_cls.__gql_fields__["email"].description == "Primary email address for authentication"
```

**Implementation** (GREEN):
```python
# src/fraiseql/introspection/input_generator.py (lines 150-178)

def _generate_from_composite_type(self, ...):
    # ... existing code ...

    # Step 2: Build annotations AND field descriptors
    annotations = {}
    gql_fields = {}  # NEW: Store field metadata

    for attr in composite_metadata.attributes:
        # ... existing field parsing ...

        # NEW: Create field descriptor with description
        from fraiseql.fields import FraiseQLField

        field_descriptor = FraiseQLField(
            field_type=python_type,
            description=attr.comment,  # PostgreSQL comment (NEW)
            purpose="input"
        )

        gql_fields[field_name] = field_descriptor
        annotations[field_name] = python_type

    # Step 4: Create input class with field metadata
    input_cls = type(
        class_name,
        (object,),
        {
            "__annotations__": annotations,
            "__gql_fields__": gql_fields,  # NEW: Store field metadata
        }
    )

    return input_cls
```

---

### Phase 4: Field-Level Descriptions (View Columns → GraphQL Output Fields)

**Objective**: Use PostgreSQL column comments as GraphQL output field descriptions

**Scope**: `TypeGenerator` + enhanced column introspection

**Challenge**: Current implementation introspects JSONB `data` column at runtime, not individual columns. Need to map JSONB fields back to underlying table columns.

#### TDD Cycle 4.1: Capture Column Comments During View Introspection

1. **RED**: Write test that expects column comments to be available
   - Test file: `tests/unit/introspection/test_postgres_introspector.py`
   - Expected failure: `ViewMetadata.columns` should include column comments

2. **GREEN**: Fix PostgreSQL introspection query to capture column comments
   - Files to modify: `src/fraiseql/introspection/postgres_introspector.py` (line 128)
   - Minimal implementation: Current query uses `obj_description()` incorrectly

3. **REFACTOR**: Verify column comment extraction works for all view types
   - Code improvements: Test with regular views, materialized views, etc.
   - Pattern compliance: Match existing introspection patterns

4. **QA**: Verify phase completion
   - [ ] Unit test passes
   - [ ] Column comments correctly captured
   - [ ] Integration test with real PostgreSQL views
   - [ ] Ready for next cycle (using comments in TypeGenerator)

**Bug Fix** (GREEN):
```python
# src/fraiseql/introspection/postgres_introspector.py (lines 123-136)

# CURRENT (WRONG):
columns_query = """
    SELECT
        a.attname as column_name,
        t.typname as pg_type,
        a.attnotnull as not_null,
        obj_description(a.attrelid, 'pg_attribute') as column_comment  -- WRONG: Gets table comment
    FROM pg_attribute a
    ...
"""

# FIXED:
columns_query = """
    SELECT
        a.attname as column_name,
        t.typname as pg_type,
        a.attnotnull as not_null,
        col_description(a.attrelid, a.attnum) as column_comment  -- CORRECT: Gets column comment
    FROM pg_attribute a
    JOIN pg_type t ON a.atttypid = t.oid
    JOIN pg_class c ON a.attrelid = c.oid
    WHERE c.relname = %s
      AND c.relkind = 'v'
      AND a.attnum > 0
      AND NOT a.attisdropped
    ORDER BY a.attnum
"""
```

#### TDD Cycle 4.2: Map JSONB Fields to View Columns

1. **RED**: Write test for field comment mapping
   - Test file: `tests/unit/introspection/test_type_generator.py`
   - Expected failure: Generated fields should have descriptions from view column comments

2. **GREEN**: Implement JSONB field → column comment mapping
   - Files to modify: `src/fraiseql/introspection/type_generator.py` (new method)
   - Minimal implementation: Match JSONB field names to view column names

3. **REFACTOR**: Handle naming convention mismatches (snake_case, camelCase)
   - Code improvements: Add field name normalization
   - Pattern compliance: Consider `@fraiseql:field name=` overrides

4. **QA**: Verify phase completion
   - [ ] Field descriptions correctly mapped
   - [ ] Handles naming convention mismatches
   - [ ] No regression in existing field generation

**Strategy**: FraiseQL views with `jsonb_column="data"` have a `data` column that contains all fields. We need to:
1. Detect if the view has explicit columns (non-JSONB pattern)
2. OR: Use a different strategy for JSONB-based views

**Alternative Approach**: For JSONB views, allow view comment to contain field metadata:
```sql
COMMENT ON VIEW app.v_users IS
'@fraiseql:type
fields:
  email: "Primary email address"
  created_at: "Account creation timestamp"
';
```

**Recommended**: Start with simpler non-JSONB views, defer JSONB field metadata to Phase 5.

#### TDD Cycle 4.3: Apply Column Comments to Generated Fields

1. **RED**: Write test that generated GraphQL fields have descriptions
   - Test file: `tests/unit/introspection/test_type_generator.py`
   - Expected failure: `__gql_fields__["email"].description` should have column comment

2. **GREEN**: Pass column comments to field generation
   - Files to modify: `src/fraiseql/introspection/type_generator.py` (around line 53-58)
   - Minimal implementation: Create `FraiseQLField` with description from column comment

3. **REFACTOR**: Ensure field descriptions work end-to-end
   - Code improvements: Verify GraphQL schema generation uses descriptions
   - Pattern compliance: Consistent with Phase 3 input field implementation

4. **QA**: Verify phase completion
   - [ ] Output fields have descriptions in GraphQL schema
   - [ ] Works with camelCase field naming
   - [ ] Integration test shows descriptions in introspection query
   - [ ] No regression in existing type generation

**Implementation** (GREEN):
```python
# src/fraiseql/introspection/type_generator.py (lines 52-70)

# NEW: Store field descriptors, not just annotations
gql_fields = {}

for field_name, field_info in jsonb_fields.items():
    python_type = self.type_mapper.pg_type_to_python(
        field_info["type"], field_info["nullable"]
    )
    annotations[field_name] = python_type

    # NEW: Get column comment if available
    column_comment = self._get_column_comment(field_name, view_metadata)

    # NEW: Create field descriptor with description
    from fraiseql.fields import FraiseQLField
    field_descriptor = FraiseQLField(
        field_type=python_type,
        description=column_comment,  # PostgreSQL column comment (NEW)
        purpose="output"
    )
    gql_fields[field_name] = field_descriptor

# Create class with field metadata
cls = type(
    class_name,
    (object,),
    {
        "__annotations__": annotations,
        "__gql_fields__": gql_fields,  # NEW: Add field metadata
        "__doc__": (
            annotation.description
            or view_metadata.comment
            or f"Auto-generated from {view_metadata.view_name}"
        ),
        "__module__": "fraiseql.introspection.generated",
    },
)
```

---

### Phase 5: Composite Type-Level Descriptions (Optional Enhancement)

**Objective**: Use composite type comments as input type `__doc__` strings

**Scope**: `InputGenerator` - set `__doc__` from `composite_metadata.comment`

#### TDD Cycle 5.1: Composite Type Comment → Input Type Description

1. **RED**: Write test for composite type comment usage
   - Test file: `tests/unit/introspection/test_input_generator.py`
   - Expected failure: Input class `__doc__` should use `composite_metadata.comment`

2. **GREEN**: Add `__doc__` to generated input classes
   - Files to modify: `src/fraiseql/introspection/input_generator.py` (line 177)
   - Minimal implementation: Include `__doc__` in type() call

3. **REFACTOR**: Ensure consistency with other generators
   - Code improvements: Same priority hierarchy as TypeGenerator/MutationGenerator
   - Pattern compliance: Clean docstrings properly

4. **QA**: Verify phase completion
   - [ ] Input types have descriptions in GraphQL schema
   - [ ] Consistent with Phase 1 and Phase 2
   - [ ] No regression

**Implementation** (GREEN):
```python
# src/fraiseql/introspection/input_generator.py (line 177)

input_cls = type(
    class_name,
    (object,),
    {
        "__annotations__": annotations,
        "__gql_fields__": gql_fields,
        "__doc__": composite_metadata.comment or f"Auto-generated from {composite_type_name}",  # NEW
    }
)
```

---

### Phase 6: Integration Testing & Documentation

**Objective**: End-to-end verification with real PostgreSQL database and user documentation

#### TDD Cycle 6.1: Integration Test Suite

1. **RED**: Write comprehensive integration test
   - Test file: `tests/integration/introspection/test_comment_descriptions_integration.py` (NEW)
   - Expected behavior: Full workflow from database comments to GraphQL introspection

2. **GREEN**: Ensure all phases work together
   - Verification: Run integration test against real PostgreSQL instance
   - Fix any integration issues discovered

3. **REFACTOR**: Optimize and clean up
   - Code improvements: Remove any temporary workarounds
   - Performance check: Ensure no N+1 queries or performance regressions

4. **QA**: Final verification
   - [ ] All unit tests pass
   - [ ] All integration tests pass
   - [ ] GraphQL introspection query shows descriptions
   - [ ] Performance benchmarks meet standards
   - [ ] Documentation complete

**Integration Test Structure**:
```python
async def test_end_to_end_comment_descriptions(db_pool):
    """Test complete flow: PostgreSQL comments → GraphQL schema descriptions."""

    # Setup: Create test database objects with comments
    await db_pool.execute("""
        CREATE VIEW app.v_test_users AS
        SELECT id, email, name FROM users;

        COMMENT ON VIEW app.v_test_users IS 'Test user profiles';
        COMMENT ON COLUMN app.v_test_users.email IS 'User email address';

        CREATE FUNCTION app.fn_create_test_user(p_email TEXT) RETURNS JSONB AS $$
        BEGIN RETURN jsonb_build_object('success', true); END;
        $$ LANGUAGE plpgsql;

        COMMENT ON FUNCTION app.fn_create_test_user IS 'Creates test user';
    """)

    # Act: Run AutoFraiseQL discovery
    discovery = AutoDiscovery(db_pool)
    schema = await discovery.generate_schema()

    # Assert: Verify descriptions in GraphQL schema
    type_def = schema.type_map["TestUsers"]
    assert type_def.description == "Test user profiles"

    email_field = type_def.fields["email"]
    assert email_field.description == "User email address"

    mutation_def = schema.mutation_type.fields["createTestUser"]
    assert mutation_def.description == "Creates test user"
```

#### TDD Cycle 6.2: User Documentation

1. **Create documentation** (not test-driven):
   - File: `docs/autofraiseql/postgresql-comments.md` (NEW)
   - Content: How to use PostgreSQL comments for GraphQL documentation
   - Examples: Complete workflow with SQL and GraphQL output

2. **Update existing docs**:
   - File: `docs/autofraiseql/README.md`
   - Add section on automatic description generation

3. **Add changelog entry**:
   - File: `CHANGELOG.md`
   - Document new feature under "Unreleased"

**Documentation Example**:
```markdown
# PostgreSQL Comments as GraphQL Descriptions

AutoFraiseQL automatically uses PostgreSQL comments as GraphQL schema descriptions.

## Adding Comments in PostgreSQL

```sql
-- Type-level descriptions
COMMENT ON VIEW app.v_users IS 'User profile data with contact information';
COMMENT ON FUNCTION app.fn_create_user(p_email text) IS 'Creates a new user account';
COMMENT ON TYPE app.type_create_user_input IS 'Input parameters for user creation';

-- Field-level descriptions
COMMENT ON COLUMN app.v_users.email IS 'Primary email address for authentication';
COMMENT ON COLUMN app.v_users.created_at IS 'Account creation timestamp (UTC)';
```

## Priority Hierarchy

1. **Explicit `@fraiseql` annotation**: Manual override in view/function comment
2. **PostgreSQL COMMENT**: Automatic from database schema
3. **Auto-generated**: Fallback generic description

## Viewing in GraphQL

```graphql
# GraphQL introspection query
{
  __type(name: "Users") {
    description  # Returns: "User profile data with contact information"
    fields {
      name
      description
    }
  }
}
```
```

---

## Success Criteria

- [x] All phases planned with clear TDD cycles
- [ ] Phase 1: Type-level descriptions (Views)
- [ ] Phase 2: Mutation-level descriptions (Functions)
- [ ] Phase 3: Input field descriptions (Composite types)
- [ ] Phase 4: Output field descriptions (View columns)
- [ ] Phase 5: Input type descriptions (Composite types)
- [ ] Phase 6: Integration tests and documentation
- [ ] All existing tests pass
- [ ] GraphQL introspection shows descriptions
- [ ] Documentation complete

## Implementation Sequence

1. **Phase 1** (Simplest) → Type-level descriptions for views
2. **Phase 2** (Similar) → Mutation-level descriptions for functions
3. **Phase 3** (Medium) → Input field descriptions from composite types
4. **Phase 4** (Complex) → Output field descriptions from columns (has challenges)
5. **Phase 5** (Easy) → Composite type descriptions for inputs
6. **Phase 6** (Validation) → Integration testing and documentation

## Phase 4 Note: JSONB Field Challenge

Phase 4 has a design challenge: FraiseQL views with `jsonb_column="data"` store all fields in a single JSONB column. PostgreSQL column comments don't apply to individual JSONB fields.

**Options**:
1. **Skip JSONB views**: Only support field descriptions for regular column-based views
2. **Metadata in view comment**: Store field metadata as structured data in view comment
3. **Trace to source tables**: Parse view definition to find source columns and their comments

**Recommendation**: Start with option 1 (non-JSONB views), evaluate need for option 2 based on user feedback.

## Notes

- Each phase is independent and can be completed separately
- Phases 1-3 are straightforward with minimal risk
- Phase 4 has complexity around JSONB fields - can be deferred
- All phases maintain backward compatibility (fallback to current behavior)
- PostgreSQL comments are standard SQL feature, no database modifications needed
