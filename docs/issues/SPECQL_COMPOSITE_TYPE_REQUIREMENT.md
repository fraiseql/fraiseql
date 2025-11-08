# Issue: Support PostgreSQL Composite Type-Based Input Generation

**Priority**: High
**Component**: AutoFraiseQL / Input Type Generation
**Affects**: Phase 4+ (Mutation Generation)
**Created**: 2025-11-08
**Reporter**: FraiseQL Team

---

## Executive Summary

The current AutoFraiseQL implementation (Phase 1-4) assumes a direct parameter-to-field mapping for mutation input generation. However, production PostgreSQL functions in PrintOptim (and likely other enterprise codebases) use a more sophisticated pattern: **composite types with JSONB wrappers**.

This pattern separates:
1. **Context parameters** (`input_pk_*`, `input_created_by`) - injected from GraphQL execution context
2. **Business logic input** (`input_payload JSONB`) - corresponds to a PostgreSQL composite type

The current implementation cannot handle this pattern, making it incompatible with real-world enterprise PostgreSQL schemas.

---

## Problem Statement

### Current Implementation Assumes

```sql
-- Simple parameter-based function
CREATE FUNCTION fn_create_user(
    p_name TEXT,
    p_email TEXT
) RETURNS User;
```

**Current behavior**: InputGenerator extracts `p_name` and `p_email` directly from function signature, generates:
```python
class CreateUserInput:
    name: str
    email: str
```

### Actual Enterprise Pattern (PrintOptim)

```sql
-- 1. Define composite type for structured input
CREATE TYPE app.type_organizational_unit_input AS (
    organizational_unit_level_id UUID,
    parent_id UUID,
    name TEXT,
    short_name TEXT,
    abbreviation TEXT
);

-- 2. Function accepts context + JSONB payload
CREATE FUNCTION app.create_organizational_unit(
    input_pk_organization UUID,      -- Context: tenant ID
    input_created_by UUID,            -- Context: user ID
    input_payload JSONB               -- Business input (maps to composite type)
) RETURNS app.mutation_result
LANGUAGE plpgsql
AS $$
DECLARE
    v_input app.type_organizational_unit_input;
BEGIN
    -- Parse JSONB into typed composite
    v_input := (
        (input_payload->>'organizational_unit_level_id')::UUID,
        (input_payload->>'parent_id')::UUID,
        input_payload->>'name',
        input_payload->>'short_name',
        input_payload->>'abbreviation'
    );

    -- Use structured input for business logic
    RETURN core.create_organizational_unit(
        input_pk_organization,
        input_created_by,
        v_input,
        input_payload
    );
END;
$$;
```

**Expected behavior**: InputGenerator should:
1. Detect `input_payload JSONB` parameter
2. Introspect `app.type_organizational_unit_input` composite type
3. Generate GraphQL input from composite type fields (not function parameters)
4. Automatically extract context parameters for `context_params` mapping

```python
# Generated GraphQL input (from composite type)
@fraiseql.input
class CreateOrganizationalUnitInput:
    organizational_unit_level_id: UUID
    parent_id: UUID | None
    name: str
    short_name: str | None
    abbreviation: str | None

# Generated mutation (with auto-detected context params)
@fraiseql.mutation(
    function="create_organizational_unit",
    schema="app",
    context_params={
        "tenant_id": "input_pk_organization",  # Auto-detected
        "user_id": "input_created_by",         # Auto-detected
    }
)
class CreateOrganizationalUnit:
    input: CreateOrganizationalUnitInput
    success: CreateOrganizationalUnitSuccess
    failure: CreateOrganizationalUnitError
```

---

## Why This Pattern Exists

### 1. Type Safety in PostgreSQL
Composite types provide compile-time type checking within PL/pgSQL, catching field name typos and type mismatches before runtime.

```sql
-- Type-safe: PostgreSQL validates field names/types at function creation
v_input.organizational_unit_level_id  -- ✓ Validated by compiler

-- Error-prone: No validation until runtime
input_payload->>'organizational_unit_levle_id'  -- ✗ Typo only caught at runtime
```

### 2. Separation of Concerns
- **Context parameters** are infrastructure (tenant ID, user ID, transaction context)
- **Business input** is domain logic (entity fields, relationships)

Mixing them in function signatures creates confusion and makes context injection harder to implement consistently.

### 3. API Gateway Pattern
The `app.*` functions act as API gateways:
- Accept raw JSONB (from GraphQL/REST)
- Parse into typed composite
- Delegate to core business logic functions
- Handle context injection uniformly

```sql
-- API Gateway (accepts JSONB)
app.create_organizational_unit(UUID, UUID, JSONB) → mutation_result

-- Core Business Logic (strongly typed)
core.create_organizational_unit(UUID, UUID, type_organizational_unit_input, JSONB) → mutation_result
```

### 4. Versioning and Evolution
Composite types can be versioned independently of function signatures:
```sql
CREATE TYPE app.type_organizational_unit_input_v2 AS (...);

-- Function signature unchanged, only internal parsing differs
CREATE FUNCTION app.create_organizational_unit_v2(UUID, UUID, JSONB) ...
```

---

## Technical Requirements

### 1. Composite Type Introspection

Add to `PostgresIntrospector`:

```python
@dataclass
class CompositeTypeMetadata:
    """Metadata for a PostgreSQL composite type."""
    schema_name: str
    type_name: str
    attributes: list["CompositeAttribute"]
    comment: Optional[str]

@dataclass
class CompositeAttribute:
    """Attribute within a composite type."""
    name: str
    pg_type: str
    ordinal_position: int
    is_nullable: bool  # Not directly available, may need convention
    default_value: Optional[str]

async def discover_composite_type(
    self,
    type_name: str,
    schema: str = "public"
) -> CompositeTypeMetadata | None:
    """
    Introspect a PostgreSQL composite type.

    Query: pg_type + pg_attribute + pg_namespace
    """
```

**SQL Query Template**:
```sql
SELECT
    a.attname AS attribute_name,
    t.typname AS pg_type,
    a.attnum AS ordinal_position,
    a.attnotnull AS not_null,
    pg_get_expr(d.adbin, d.adrelid) AS default_value,
    col_description(c.oid, a.attnum) AS comment
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_attribute a ON a.attrelid = c.oid
JOIN pg_type t ON t.oid = a.atttypid
LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
WHERE c.relkind = 'c'  -- Composite type
  AND n.nspname = $1   -- Schema name
  AND c.relname = $2   -- Type name
  AND a.attnum > 0     -- Exclude system columns
  AND NOT a.attisdropped
ORDER BY a.attnum;
```

### 2. Input Generator Enhancement

Update `InputGenerator.generate_input_type()`:

```python
def generate_input_type(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation,
    introspector: PostgresIntrospector  # NEW: pass introspector
) -> Type:
    """
    Generate input class for mutation.

    Strategy:
    1. Look for JSONB parameter (typically 'input_payload')
    2. Check function comment for @type annotation referencing composite type
    3. If found, introspect composite type and use its attributes
    4. Otherwise, fall back to parameter-based generation (legacy)
    """

    # Look for JSONB input parameter
    jsonb_param = next(
        (p for p in function_metadata.parameters
         if p.pg_type.lower() == 'jsonb'
         and p.name.startswith('input_')
         and not p.name.startswith('input_pk_')
         and p.name != 'input_created_by'),
        None
    )

    if jsonb_param:
        # Extract composite type name from annotation or convention
        composite_type_name = self._extract_composite_type_name(
            function_metadata,
            annotation
        )

        if composite_type_name:
            # Generate from composite type
            return await self._generate_from_composite_type(
                composite_type_name,
                function_metadata.schema_name,
                introspector
            )

    # Fall back to parameter-based generation
    return self._generate_from_parameters(function_metadata, annotation)
```

### 3. Composite Type Name Extraction

Add convention-based and annotation-based extraction:

```python
def _extract_composite_type_name(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation
) -> str | None:
    """
    Extract composite type name from function metadata.

    Priority:
    1. Explicit annotation: @fraiseql:input_type app.type_user_input
    2. Convention: fn_create_user → type_user_input
    3. Function comment parsing
    """

    # 1. Check for explicit annotation in function comment
    if annotation.input_type:
        return annotation.input_type

    # 2. Convention-based extraction
    # fn_create_organizational_unit → type_organizational_unit_input
    action_entity = function_metadata.function_name.replace('fn_', '')
    return f"type_{action_entity}_input"
```

### 4. Context Parameter Auto-Detection

Update `MutationGenerator`:

```python
def _extract_context_params(
    self,
    function_metadata: FunctionMetadata
) -> dict[str, str]:
    """
    Auto-detect context parameters from function signature.

    Convention:
    - input_pk_* → Maps to context field (remove 'input_pk_' prefix)
    - input_created_by → Maps to 'created_by' or 'user_id' context
    - input_updated_by → Maps to 'updated_by' or 'user_id' context

    Returns:
        Mapping of context_key → function_parameter_name
    """
    context_params = {}

    for param in function_metadata.parameters:
        if param.name.startswith('input_pk_'):
            # input_pk_organization → organization_id
            context_key = param.name.replace('input_pk_', '') + '_id'
            context_params[context_key] = param.name

        elif param.name == 'input_created_by':
            context_params['user_id'] = param.name

        elif param.name == 'input_updated_by':
            context_params['user_id'] = param.name

    return context_params
```

### 5. Metadata Parser Extension

Add to `MutationAnnotation`:

```python
@dataclass
class MutationAnnotation:
    """Parsed @fraiseql:mutation annotation."""
    name: str
    description: Optional[str]
    success_type: str
    failure_type: str
    input_type: Optional[str] = None  # NEW: Explicit input type reference
    context_mapping: Optional[dict[str, str]] = None  # NEW: Override auto-detection
```

Example usage in function comment:
```sql
COMMENT ON FUNCTION app.create_organizational_unit IS
'@fraiseql:mutation
name: createOrganizationalUnit
description: Create a new organizational unit
input_type: app.type_organizational_unit_input
success_type: CreateOrganizationalUnitSuccess
failure_type: CreateOrganizationalUnitError
context_mapping: {"tenant_id": "input_pk_organization", "user_id": "input_created_by"}';
```

---

## Implementation Phases

### Phase 5.1: Composite Type Introspection (Foundation)
- [ ] Add `CompositeTypeMetadata` and `CompositeAttribute` dataclasses
- [ ] Implement `PostgresIntrospector.discover_composite_type()`
- [ ] Write unit tests for composite type discovery
- [ ] Integration test with real PrintOptim schema

**Acceptance**: Can introspect `app.type_organizational_unit_input` and retrieve all 5 fields with correct types.

### Phase 5.2: Input Generation from Composite Types
- [ ] Update `InputGenerator.generate_input_type()` to detect JSONB parameters
- [ ] Implement `_extract_composite_type_name()` with convention-based extraction
- [ ] Implement `_generate_from_composite_type()` using introspector
- [ ] Fall back to parameter-based generation for non-composite functions
- [ ] Write unit tests for both code paths

**Acceptance**: Generates correct GraphQL input type from `app.type_organizational_unit_input` composite type.

### Phase 5.3: Context Parameter Auto-Detection
- [ ] Implement `_extract_context_params()` in MutationGenerator
- [ ] Handle `input_pk_*` naming convention
- [ ] Handle `input_created_by` / `input_updated_by` convention
- [ ] Pass auto-detected params to `@fraiseql.mutation` decorator
- [ ] Write tests for various parameter combinations

**Acceptance**: Automatically generates `context_params={"tenant_id": "input_pk_organization", "user_id": "input_created_by"}`.

### Phase 5.4: Annotation-Based Overrides
- [ ] Extend `MutationAnnotation` with `input_type` and `context_mapping`
- [ ] Update `MetadataParser.parse_mutation_annotation()` to extract new fields
- [ ] Use annotation values to override convention-based detection
- [ ] Document annotation format in schema comments
- [ ] Write tests for annotation parsing and override behavior

**Acceptance**: Can explicitly specify input type and context mapping via function comments.

### Phase 5.5: Integration and E2E Testing
- [ ] Test with full PrintOptim schema (organizational_unit, location, machine, etc.)
- [ ] Verify generated mutations match hand-written equivalents
- [ ] Test GraphQL schema generation and query execution
- [ ] Performance testing with large schemas (100+ composite types)
- [ ] Update AutoFraiseQL documentation with composite type pattern

**Acceptance**: Can auto-generate all PrintOptim mutations with zero manual code, matching existing hand-written implementations.

---

## Example Test Cases

### Test 1: Basic Composite Type Input Generation
```python
async def test_composite_type_input_generation():
    """Verify input generation from composite type."""

    # Given: Function with JSONB parameter
    function = FunctionMetadata(
        schema_name="app",
        function_name="create_organizational_unit",
        parameters=[
            ParameterInfo("input_pk_organization", "uuid", "IN", None),
            ParameterInfo("input_created_by", "uuid", "IN", None),
            ParameterInfo("input_payload", "jsonb", "IN", None),
        ],
        return_type="app.mutation_result",
        comment="@fraiseql:mutation ...",
        language="plpgsql"
    )

    # Given: Composite type exists
    composite_type = CompositeTypeMetadata(
        schema_name="app",
        type_name="type_organizational_unit_input",
        attributes=[
            CompositeAttribute("organizational_unit_level_id", "uuid", 1, False, None),
            CompositeAttribute("parent_id", "uuid", 2, True, None),
            CompositeAttribute("name", "text", 3, False, None),
            CompositeAttribute("short_name", "text", 4, True, None),
            CompositeAttribute("abbreviation", "text", 5, True, None),
        ],
        comment=None
    )

    # When: Generate input type
    input_cls = input_generator.generate_input_type(function, annotation)

    # Then: Input class has composite type fields
    assert input_cls.__name__ == "CreateOrganizationalUnitInput"
    assert input_cls.__annotations__ == {
        "organizational_unit_level_id": UUID,
        "parent_id": UUID | None,
        "name": str,
        "short_name": str | None,
        "abbreviation": str | None,
    }
```

### Test 2: Context Parameter Auto-Detection
```python
def test_context_param_auto_detection():
    """Verify automatic context parameter extraction."""

    # Given: Function with context parameters
    function = FunctionMetadata(
        schema_name="app",
        function_name="create_organizational_unit",
        parameters=[
            ParameterInfo("input_pk_organization", "uuid", "IN", None),
            ParameterInfo("input_created_by", "uuid", "IN", None),
            ParameterInfo("input_payload", "jsonb", "IN", None),
        ],
        ...
    )

    # When: Extract context params
    context_params = mutation_generator._extract_context_params(function)

    # Then: Context params correctly mapped
    assert context_params == {
        "organization_id": "input_pk_organization",
        "user_id": "input_created_by",
    }
```

### Test 3: Legacy Parameter-Based Fallback
```python
async def test_legacy_parameter_based_generation():
    """Verify fallback to parameter-based generation when no composite type."""

    # Given: Function with simple parameters (no JSONB)
    function = FunctionMetadata(
        schema_name="public",
        function_name="fn_create_user",
        parameters=[
            ParameterInfo("p_name", "text", "IN", None),
            ParameterInfo("p_email", "text", "IN", None),
        ],
        ...
    )

    # When: Generate input type
    input_cls = input_generator.generate_input_type(function, annotation)

    # Then: Falls back to parameter-based generation
    assert input_cls.__annotations__ == {
        "name": str,
        "email": str,
    }
```

---

## Migration Path

### For Existing Simple Functions (No Breaking Changes)
Functions without JSONB parameters continue to work with parameter-based generation:
```sql
CREATE FUNCTION fn_simple_mutation(p_name TEXT, p_value INT) ...
```
→ No changes required, existing behavior preserved.

### For New Composite Type Functions
Add composite type and update function signature:
```sql
-- 1. Define composite type
CREATE TYPE app.type_entity_input AS (...);

-- 2. Update function to accept JSONB
CREATE FUNCTION app.create_entity(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB  -- References composite type
) ...;

-- 3. Add @fraiseql annotation (optional, convention-based extraction also works)
COMMENT ON FUNCTION app.create_entity IS '@fraiseql:mutation
input_type: app.type_entity_input
...';
```
→ AutoFraiseQL automatically detects and generates from composite type.

---

## Open Questions for SpecQL Team

1. **Nullable Semantics**: PostgreSQL composite types don't have explicit NULL constraints on attributes. Should we:
   - Assume all fields nullable by default?
   - Parse field comments for `@nullable: false` annotations?
   - Use a naming convention (e.g., `field_name!` for required)?

2. **Schema Resolution**: If function is in `app` schema but references `type_user_input` without schema prefix:
   - Search same schema first, then `public`?
   - Require fully qualified type names in annotations?

3. **Type Name Conventions**: Current proposal uses:
   - `fn_create_user` → `type_create_user_input`

   Alternative conventions:
   - `fn_create_user` → `type_user_input` (remove action verb)
   - Require explicit annotation always?

4. **Array/Nested Types**: How to handle composite types with array fields or nested composite types?
   ```sql
   CREATE TYPE app.type_complex_input AS (
       tags TEXT[],
       metadata app.type_metadata_input
   );
   ```

5. **Versioning**: Should we support versioned composite types?
   ```sql
   CREATE TYPE app.type_user_input_v2 AS (...);
   ```
   How to specify which version in annotations?

6. **Performance**: For large schemas with 100+ composite types:
   - Cache composite type metadata?
   - Lazy load on first use?
   - Pre-load all during startup?

---

## Success Metrics

- [ ] **Zero Manual Code**: All PrintOptim mutations auto-generated from PostgreSQL metadata
- [ ] **Type Safety**: Generated inputs match composite type definitions exactly
- [ ] **Context Injection**: Automatic context parameter detection works for 100% of functions
- [ ] **Performance**: Schema introspection completes in <2s for 100+ functions
- [ ] **Backward Compatibility**: Existing parameter-based functions continue to work unchanged

---

## References

### Related Code
- **Current Implementation**: `/home/lionel/code/fraiseql/src/fraiseql/introspection/input_generator.py:20-55`
- **PrintOptim Example**: `/home/lionel/code/printoptim_backend/db/0_schema/03_functions/034_dim/0342_org/03421_organizational_unit/034211_create_organizational_unit.sql`
- **Composite Type Example**: `app.type_organizational_unit_input` (5 fields: UUID, UUID?, TEXT, TEXT?, TEXT?)

### Documentation
- PostgreSQL Composite Types: https://www.postgresql.org/docs/current/rowtypes.html
- FraiseQL Mutation Decorator: Current repo docs
- AutoFraiseQL Architecture: Phase 1-4 implementation notes

### Similar Patterns in Industry
- **Hasura**: Introspects PostgreSQL types for GraphQL schema generation
- **PostGraphile**: Generates GraphQL from database schema, supports composite types
- **Prisma**: Type-safe ORM with schema introspection

---

## Contacts

**Reporter**: FraiseQL Team
**Assignee**: SpecQL Meta-Framework Team
**Reviewers**: Claude Code Development Team

**Questions?** Reference this document in your implementation PRs.
