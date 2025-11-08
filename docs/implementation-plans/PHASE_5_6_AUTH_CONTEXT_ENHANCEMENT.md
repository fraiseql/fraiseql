# FraiseQL Phase 5.6: Auth Context Enhancement - Implementation Plan

**Status**: Ready for Implementation
**Priority**: HIGH (Security & SpecQL Integration)
**Complexity**: LOW-MEDIUM
**Estimated Time**: 4-6 hours
**Breaking Changes**: ✅ YES (No users, full steam ahead on `auth_*`)

---

## ⚠️ IMPORTANT: Breaking Changes Allowed

**We have no users yet** - we can make breaking changes to establish the right patterns from the start.

### New Standard Convention

**OLD** (Phase 5.0-5.5):
```sql
CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,    -- ❌ DEPRECATED
    input_user_id UUID,       -- ❌ DEPRECATED
    input_payload JSONB
);
```

**NEW** (Phase 5.6+):
```sql
CREATE FUNCTION app.create_contact(
    auth_tenant_id UUID,      -- ✅ NEW STANDARD
    auth_user_id UUID,         -- ✅ NEW STANDARD
    input_payload JSONB
);
```

**Rationale**:
- `auth_` prefix is clearer: "these come from authentication"
- `input_` is ambiguous: everything is input
- Aligns with SpecQL team's convention
- Standard across the ecosystem

---

## 📋 Table of Contents

1. [Goals](#goals)
2. [Changes Overview](#changes-overview)
3. [Implementation Steps](#implementation-steps)
4. [Testing Strategy](#testing-strategy)
5. [Migration Guide](#migration-guide)

---

## Goals

### Primary Goals

1. ✅ **Standardize on `auth_*` prefix** for authentication context parameters
2. ✅ **Support explicit `context_params` metadata** from function comments
3. ✅ **Exclude auth params from GraphQL input schema** (security critical)
4. ✅ **Validate context params** before PostgreSQL function calls
5. ✅ **Remove legacy `input_*` support** (breaking change, that's OK!)

### What This Enables

**SpecQL Integration**: Seamless integration with SpecQL-generated schemas
**Security**: Auth context cannot be client-controlled
**Clarity**: Clear distinction between auth context and business input

---

## Changes Overview

### 3 Files to Modify

| File | Component | Changes | Lines |
|------|-----------|---------|-------|
| `metadata_parser.py` | Metadata Parsing | Add `context_params` field & parsing | ~20 |
| `mutation_generator.py` | Context Detection | Use `auth_*` standard, parse metadata | ~30 |
| `input_generator.py` | Input Generation | Exclude auth params properly | ~15 |

**Total**: ~65 lines of new/modified code + tests

### Breaking Changes Summary

| What Changes | Old Behavior | New Behavior |
|--------------|--------------|--------------|
| **Parameter prefix** | `input_tenant_id`, `input_user_id` | `auth_tenant_id`, `auth_user_id` |
| **Detection** | Auto-detect `input_*` | Auto-detect `auth_*` + explicit metadata |
| **Legacy support** | Supported `input_pk_*` | ❌ REMOVED |

---

## Implementation Steps

---

## 🔧 STEP 1: Update Metadata Parser

### Step 1.1: Add `context_params` Field to `MutationAnnotation`

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Location**: Add to `MutationAnnotation` dataclass (around line 60)

**Change**:
```python
@dataclass
class MutationAnnotation:
    """Parsed @fraiseql:mutation annotation."""

    name: str
    description: Optional[str]
    success_type: str
    failure_type: str
    input_type: Optional[str] = None
    context_params: Optional[list[str]] = None  # NEW: Explicit context params
```

**Why**: SpecQL will specify context params explicitly in metadata.

---

### Step 1.2: Parse `context_params` from Function Comment

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Location**: Inside `parse_mutation_annotation()` method (around line 95)

**Add this parsing logic**:

```python
def parse_mutation_annotation(self, comment: str | None) -> MutationAnnotation | None:
    """
    Parse @fraiseql:mutation annotation from function comment.

    Now supports context_params:
        @fraiseql:mutation
        name: createContact
        success_type: Contact
        failure_type: ContactError
        context_params: [auth_tenant_id, auth_user_id]  # NEW
    """
    if not comment or "@fraiseql:mutation" not in comment:
        return None

    # Extract YAML content
    lines = comment.split('\n')
    yaml_lines = []
    in_annotation = False

    for line in lines:
        if '@fraiseql:mutation' in line:
            in_annotation = True
            continue
        if in_annotation:
            if line.strip() and not line.strip().startswith('@'):
                yaml_lines.append(line)
            elif line.strip().startswith('@'):
                break

    if not yaml_lines:
        return None

    # Parse YAML
    try:
        import yaml
        data = yaml.safe_load('\n'.join(yaml_lines))

        # Required fields
        name = data.get('name')
        success_type = data.get('success_type')
        failure_type = data.get('failure_type')

        if not all([name, success_type, failure_type]):
            return None

        # Optional fields
        description = data.get('description')
        input_type = data.get('input_type')
        context_params = data.get('context_params')  # NEW: Parse context_params array

        return MutationAnnotation(
            name=name,
            description=description,
            success_type=success_type,
            failure_type=failure_type,
            input_type=input_type,
            context_params=context_params  # NEW
        )
    except Exception as e:
        logger.warning(f"Failed to parse mutation annotation: {e}")
        return None
```

**What this does**:
- Parses `context_params: [auth_tenant_id, auth_user_id]` from YAML metadata
- Returns it as a list of parameter names
- Handles both YAML list format and JSON array format

---

### Step 1.3: Write Tests for Metadata Parsing

**File**: `tests/unit/introspection/test_metadata_parser.py`

**Add these tests**:

```python
def test_parse_mutation_annotation_with_context_params():
    """Test parsing mutation annotation with context_params."""
    # Given: Mutation comment with context_params
    comment = """
    @fraiseql:mutation
    name: qualifyLead
    success_type: Contact
    failure_type: ContactError
    context_params: [auth_tenant_id, auth_user_id]
    """

    # When: Parse annotation
    parser = MetadataParser()
    annotation = parser.parse_mutation_annotation(comment)

    # Then: context_params is parsed
    assert annotation is not None
    assert annotation.name == "qualifyLead"
    assert annotation.context_params == ["auth_tenant_id", "auth_user_id"]


def test_parse_mutation_annotation_without_context_params():
    """Test parsing mutation annotation without context_params (backward compat)."""
    # Given: Mutation comment without context_params
    comment = """
    @fraiseql:mutation
    name: getStatus
    success_type: Status
    failure_type: StatusError
    """

    # When: Parse annotation
    parser = MetadataParser()
    annotation = parser.parse_mutation_annotation(comment)

    # Then: context_params is None (will use auto-detection)
    assert annotation is not None
    assert annotation.name == "getStatus"
    assert annotation.context_params is None
```

**Run tests**:
```bash
uv run pytest tests/unit/introspection/test_metadata_parser.py::test_parse_mutation_annotation_with_context_params -v
```

**Expected**: ✅ Tests pass

---

## 🔧 STEP 2: Update Context Parameter Detection

### Step 2.1: Standardize on `auth_*` Prefix

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Location**: Replace `_extract_context_params()` method (lines 26-80)

**Replace entire method with**:

```python
def _extract_context_params(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation
) -> dict[str, str]:
    """
    Extract context parameters from function signature.

    NEW STANDARD (Phase 5.6):
        auth_tenant_id UUID   → context["tenant_id"]
        auth_user_id UUID     → context["user_id"]

    Priority:
    1. Explicit metadata (annotation.context_params)
    2. Auto-detection by auth_ prefix

    BREAKING CHANGE: No longer supports input_* or input_pk_* conventions.
    Use auth_* prefix for authentication context parameters.

    Args:
        function_metadata: Function metadata from introspection
        annotation: Parsed mutation annotation (may contain explicit context_params)

    Returns:
        Mapping of context_key → function_parameter_name

    Example:
        Function signature:
            app.qualify_lead(p_contact_id UUID, auth_tenant_id UUID, auth_user_id UUID)

        Returns:
            {
                "tenant_id": "auth_tenant_id",
                "user_id": "auth_user_id"
            }
    """
    context_params = {}

    # PRIORITY 1: Explicit metadata (SpecQL provides this)
    if annotation and annotation.context_params:
        for param_name in annotation.context_params:
            # Find the parameter in function metadata
            param = next(
                (p for p in function_metadata.parameters if p.name == param_name),
                None
            )
            if param:
                # Extract context key from parameter name
                # auth_tenant_id → tenant_id
                # auth_user_id → user_id
                if param_name.startswith('auth_'):
                    context_key = param_name.replace('auth_', '')
                else:
                    # Non-standard naming, use as-is
                    context_key = param_name

                context_params[context_key] = param_name

        return context_params

    # PRIORITY 2: Auto-detection by auth_ prefix
    for param in function_metadata.parameters:
        # Standard: auth_tenant_id → tenant_id
        if param.name == 'auth_tenant_id':
            context_params['tenant_id'] = param.name

        # Standard: auth_user_id → user_id
        elif param.name == 'auth_user_id':
            context_params['user_id'] = param.name

        # Generic: auth_<name> → <name>
        elif param.name.startswith('auth_'):
            context_key = param.name.replace('auth_', '')
            context_params[context_key] = param.name

    return context_params
```

**What changed**:
- ❌ **REMOVED**: `input_tenant_id`, `input_user_id` support
- ❌ **REMOVED**: `input_pk_*` legacy support
- ❌ **REMOVED**: `input_created_by` legacy support
- ✅ **NEW**: `auth_*` prefix as standard
- ✅ **NEW**: Explicit metadata support (priority 1)
- ✅ **NEW**: Generic `auth_<name>` pattern

---

### Step 2.2: Update Mutation Generation to Pass Annotation

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Location**: `generate_mutation_for_function()` method (around line 100)

**Change**:

```python
# Line ~115
# OLD:
context_params = self._extract_context_params(function_metadata)

# NEW:
context_params = self._extract_context_params(function_metadata, annotation)
```

**Why**: Pass annotation so `_extract_context_params()` can use explicit metadata.

---

### Step 2.3: Write Tests for New Context Detection

**File**: `tests/unit/introspection/test_mutation_generator.py`

**Replace existing context param tests with**:

```python
def test_extract_context_params_auth_prefix():
    """Test context parameter extraction with auth_ prefix (new standard)."""
    # Given: MutationGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)

    # Given: Function with auth_ prefix context params
    function = FunctionMetadata(
        schema_name="app",
        function_name="qualify_lead",
        parameters=[
            ParameterInfo("p_contact_id", "uuid", "IN", None),
            ParameterInfo("auth_tenant_id", "uuid", "IN", None),
            ParameterInfo("auth_user_id", "uuid", "IN", None),
        ],
        return_type="app.mutation_result",
        comment=None,
        language="plpgsql"
    )

    # Given: Annotation without explicit context_params (will auto-detect)
    annotation = MutationAnnotation(
        name="qualifyLead",
        description=None,
        success_type="Contact",
        failure_type="ContactError",
        context_params=None
    )

    # When: Extract context params
    context_params = mutation_generator._extract_context_params(function, annotation)

    # Then: Correct mapping
    assert context_params == {
        "tenant_id": "auth_tenant_id",
        "user_id": "auth_user_id"
    }


def test_extract_context_params_explicit_metadata():
    """Test context parameter extraction with explicit metadata."""
    # Given: MutationGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)

    # Given: Function with context params
    function = FunctionMetadata(
        schema_name="crm",
        function_name="qualify_lead",
        parameters=[
            ParameterInfo("p_contact_id", "uuid", "IN", None),
            ParameterInfo("auth_tenant_id", "text", "IN", None),
            ParameterInfo("auth_user_id", "uuid", "IN", None),
        ],
        return_type="jsonb",
        comment=None,
        language="plpgsql"
    )

    # Given: Annotation WITH explicit context_params (SpecQL provides this)
    annotation = MutationAnnotation(
        name="qualifyLead",
        description=None,
        success_type="Contact",
        failure_type="ContactError",
        context_params=["auth_tenant_id", "auth_user_id"]  # Explicit!
    )

    # When: Extract context params
    context_params = mutation_generator._extract_context_params(function, annotation)

    # Then: Uses explicit metadata (priority 1)
    assert context_params == {
        "tenant_id": "auth_tenant_id",
        "user_id": "auth_user_id"
    }


def test_extract_context_params_no_context():
    """Test context parameter extraction with no context params."""
    # Given: Function without context parameters
    function = FunctionMetadata(
        schema_name="public",
        function_name="get_status",
        parameters=[
            ParameterInfo("p_status_id", "uuid", "IN", None),
        ],
        return_type="jsonb",
        comment=None,
        language="plpgsql"
    )

    annotation = MutationAnnotation(
        name="getStatus",
        description=None,
        success_type="Status",
        failure_type="StatusError",
        context_params=None
    )

    # When: Extract context params
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)
    context_params = mutation_generator._extract_context_params(function, annotation)

    # Then: Empty dict (no context params)
    assert context_params == {}


def test_extract_context_params_generic_auth_prefix():
    """Test generic auth_ prefix support (e.g., auth_organization_id)."""
    # Given: Function with non-standard auth param
    function = FunctionMetadata(
        schema_name="app",
        function_name="create_item",
        parameters=[
            ParameterInfo("p_name", "text", "IN", None),
            ParameterInfo("auth_organization_id", "uuid", "IN", None),  # Non-standard
        ],
        return_type="jsonb",
        comment=None,
        language="plpgsql"
    )

    annotation = MutationAnnotation(
        name="createItem",
        description=None,
        success_type="Item",
        failure_type="ItemError",
        context_params=None
    )

    # When: Extract context params
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)
    context_params = mutation_generator._extract_context_params(function, annotation)

    # Then: Generic auth_ handling (auth_organization_id → organization_id)
    assert context_params == {
        "organization_id": "auth_organization_id"
    }
```

**Delete these old tests**:
- `test_extract_context_params_new_convention` (replaced by `test_extract_context_params_auth_prefix`)
- `test_extract_context_params_legacy_convention` (no longer supporting legacy)

**Run tests**:
```bash
uv run pytest tests/unit/introspection/test_mutation_generator.py -v
```

**Expected**: ✅ All tests pass

---

## 🔧 STEP 3: Update Input Generation

### Step 3.1: Exclude Auth Parameters from Input Schema

**File**: `src/fraiseql/introspection/input_generator.py`

**Location**: `_generate_from_parameters()` method (around line 253)

**Current code** (lines 258-261):
```python
# Skip context parameters
if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
    continue
```

**Replace with**:
```python
# Skip authentication context parameters (NEVER expose these to GraphQL)
# These are server-controlled and injected from context.auth
if param.name.startswith('auth_'):
    continue
```

**Why**:
- Simpler: Just check `auth_` prefix
- Secure: ALL auth params excluded from GraphQL input
- Clear: `auth_` means "server-controlled, not client input"

---

### Step 3.2: Pass Context Params to Input Generator

**File**: `src/fraiseql/introspection/input_generator.py`

**Location**: `generate_input_type()` method (around line 197)

**Better approach** - Pass context_params explicitly:

**Update method signature** (line 197):
```python
async def generate_input_type(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation,
    introspector: "PostgresIntrospector",
    context_params: dict[str, str] = None  # NEW: Explicit exclusion list
) -> Type:
```

**Update `_generate_from_parameters()` call** (line 245):
```python
# OLD:
return self._generate_from_parameters(function_metadata, annotation)

# NEW:
return self._generate_from_parameters(function_metadata, annotation, context_params)
```

**Update `_generate_from_parameters()` signature** (line 253):
```python
def _generate_from_parameters(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation,
    context_params: dict[str, str] = None  # NEW
) -> Type:
    """Generate input class from function parameters (legacy pattern)."""
    class_name = self._function_to_input_name(function_metadata.function_name)

    annotations = {}

    # Get set of parameter names to exclude
    exclude_params = set(context_params.values()) if context_params else set()

    for param in function_metadata.parameters:
        # Skip if in explicit context_params list
        if param.name in exclude_params:
            continue

        # Skip authentication context parameters by prefix
        if param.name.startswith('auth_'):
            continue

        # Skip input_payload (composite type pattern)
        if param.name == 'input_payload':
            continue

        # Skip output parameters
        if param.mode != "IN":
            continue

        # Map parameter to input field
        field_name = param.name.replace("p_", "")  # Remove p_ prefix
        python_type = self.type_mapper.pg_type_to_python(
            param.pg_type,
            nullable=(param.default_value is not None)
        )
        annotations[field_name] = python_type

    # Create input class
    input_cls = type(class_name, (object,), {"__annotations__": annotations})

    return input_cls
```

**What this does**:
- Accepts explicit `context_params` list to exclude
- Checks both explicit list AND `auth_` prefix
- Ensures auth params NEVER appear in GraphQL input schema

---

### Step 3.3: Update Caller to Pass Context Params

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Location**: `generate_mutation_for_function()` method (around line 105)

**Change**:

```python
# Around line 105-110

# OLD:
input_cls = await self.input_generator.generate_input_type(
    function_metadata,
    annotation,
    introspector
)

# NEW:
# First extract context params (for exclusion from input schema)
context_params = self._extract_context_params(function_metadata, annotation)

# Then generate input type (excluding context params)
input_cls = await self.input_generator.generate_input_type(
    function_metadata,
    annotation,
    introspector,
    context_params  # Pass for exclusion
)
```

**Why**: Ensures input generator knows which params to exclude from GraphQL schema.

---

### Step 3.4: Write Tests for Input Exclusion

**File**: `tests/unit/introspection/test_input_generator.py`

**Add this test**:

```python
@pytest.mark.asyncio
async def test_generate_input_excludes_auth_params(test_db_pool):
    """Test that auth_ parameters are excluded from GraphQL input schema."""
    # Given: InputGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    introspector = PostgresIntrospector(test_db_pool)

    # Given: Function with auth params (should be excluded)
    function = FunctionMetadata(
        schema_name="crm",
        function_name="qualify_lead",
        parameters=[
            ParameterInfo("p_contact_id", "uuid", "IN", None),
            ParameterInfo("auth_tenant_id", "text", "IN", None),  # Should be excluded
            ParameterInfo("auth_user_id", "uuid", "IN", None),    # Should be excluded
        ],
        return_type="jsonb",
        comment=None,
        language="plpgsql"
    )

    # Given: Context params (for exclusion)
    context_params = {
        "tenant_id": "auth_tenant_id",
        "user_id": "auth_user_id"
    }

    # Given: Annotation
    annotation = MutationAnnotation(
        name="qualifyLead",
        description=None,
        success_type="Contact",
        failure_type="ContactError"
    )

    # When: Generate input type
    input_cls = await input_generator.generate_input_type(
        function,
        annotation,
        introspector,
        context_params  # Pass context params for exclusion
    )

    # Then: Only business parameter included (NO auth params)
    assert input_cls.__name__ == "QualifyLeadInput"
    assert "contact_id" in input_cls.__annotations__
    assert "auth_tenant_id" not in input_cls.__annotations__  # ✅ Excluded!
    assert "auth_user_id" not in input_cls.__annotations__    # ✅ Excluded!
    assert "tenant_id" not in input_cls.__annotations__       # ✅ Excluded!
    assert "user_id" not in input_cls.__annotations__         # ✅ Excluded!
```

**Run test**:
```bash
uv run pytest tests/unit/introspection/test_input_generator.py::test_generate_input_excludes_auth_params -v
```

**Expected**: ✅ Test passes

---

## 🧪 Testing Strategy

### Unit Tests Summary

**New Tests** (5 tests):
1. `test_parse_mutation_annotation_with_context_params` - Metadata parsing
2. `test_extract_context_params_auth_prefix` - Auth prefix auto-detection
3. `test_extract_context_params_explicit_metadata` - Explicit metadata priority
4. `test_extract_context_params_generic_auth_prefix` - Generic auth_ support
5. `test_generate_input_excludes_auth_params` - GraphQL schema exclusion

**Updated Tests** (2 tests):
1. `test_extract_context_params_no_context` - Update for new signature
2. `test_generate_input_from_parameters_legacy` - Update for context_params param

**Deleted Tests** (2 tests):
1. `test_extract_context_params_new_convention` - Replaced by auth_prefix test
2. `test_extract_context_params_legacy_convention` - Legacy no longer supported

**Total Test Changes**: 5 new + 2 updated - 2 deleted = **5 net new tests**

### Run All Tests

```bash
# Run all introspection unit tests
uv run pytest tests/unit/introspection/ -v

# Run specific modules
uv run pytest tests/unit/introspection/test_metadata_parser.py -v
uv run pytest tests/unit/introspection/test_mutation_generator.py -v
uv run pytest tests/unit/introspection/test_input_generator.py -v
```

**Expected**: ✅ All tests pass

---

### Integration Tests

**File**: `tests/integration/introspection/test_composite_type_generation_integration.py`

**Update test schema** to use `auth_*` prefix:

```sql
-- Update fixture SQL
CREATE FUNCTION app.create_contact(
    auth_tenant_id UUID,    -- Changed from input_tenant_id
    auth_user_id UUID,       -- Changed from input_user_id
    input_payload JSONB
) RETURNS app.mutation_result;

COMMENT ON FUNCTION app.create_contact IS
  '@fraiseql:mutation
   name: createContact
   input_type: app.type_create_contact_input
   success_type: Contact
   failure_type: ContactError
   context_params: [auth_tenant_id, auth_user_id]';  -- NEW
```

**Run integration tests**:
```bash
uv run pytest tests/integration/introspection/test_composite_type_generation_integration.py -v
```

**Expected**: ✅ Tests skip (no SpecQL schema) or pass (if schema exists)

---

## 📚 Migration Guide

### For Internal Development

**If you have test functions using old convention:**

```sql
-- OLD (Phase 5.0-5.5)
CREATE FUNCTION app.test_function(
    input_tenant_id UUID,    -- ❌ Change to auth_tenant_id
    input_user_id UUID,       -- ❌ Change to auth_user_id
    p_data TEXT
);

-- NEW (Phase 5.6+)
CREATE FUNCTION app.test_function(
    auth_tenant_id UUID,      -- ✅ New standard
    auth_user_id UUID,         -- ✅ New standard
    p_data TEXT
);
```

**Find and replace in SQL files**:
```bash
# Find old patterns
grep -r "input_tenant_id\|input_user_id" db/

# Update to new standard
sed -i 's/input_tenant_id/auth_tenant_id/g' db/**/*.sql
sed -i 's/input_user_id/auth_user_id/g' db/**/*.sql
```

### For SpecQL Team

**No migration needed!** ✅

You're already using the `auth_*` convention, which is now the standard.

---

## ✅ Validation Checklist

### Phase 5.6 Complete When:

- [ ] `MutationAnnotation` has `context_params` field
- [ ] Metadata parser extracts `context_params` from comments
- [ ] Context detection uses `auth_*` prefix as standard
- [ ] Context detection supports explicit metadata (priority 1)
- [ ] Context detection supports generic `auth_<name>` pattern
- [ ] Input generation excludes `auth_*` parameters
- [ ] Input generation accepts explicit `context_params` exclusion list
- [ ] All unit tests pass (59 + 5 new = 64 tests)
- [ ] Integration tests updated for `auth_*` prefix
- [ ] Old `input_*` convention removed from code
- [ ] Documentation updated

---

## 🎯 Success Criteria

### Functional Requirements

1. ✅ **Auto-detect `auth_*` context params**
   ```sql
   CREATE FUNCTION f(auth_tenant_id UUID, auth_user_id UUID, p_data TEXT);
   -- Detects: {tenant_id: "auth_tenant_id", user_id: "auth_user_id"}
   ```

2. ✅ **Parse explicit `context_params` metadata**
   ```sql
   COMMENT ON FUNCTION f IS '@fraiseql:mutation context_params: [auth_tenant_id, auth_user_id]';
   -- Uses explicit metadata (priority 1)
   ```

3. ✅ **Exclude auth params from GraphQL input**
   ```graphql
   input FInput {
     data: String!
     # NO auth_tenant_id
     # NO auth_user_id
   }
   ```

4. ✅ **Pass context params to mutation decorator**
   ```python
   @fraiseql.mutation(
       function="f",
       context_params={"tenant_id": "auth_tenant_id", "user_id": "auth_user_id"}
   )
   ```

### Security Requirements

1. ✅ **Auth params NEVER in GraphQL input schema**
   - Client cannot control auth context
   - Server injects from `context.auth`

2. ✅ **Clear naming convention**
   - `auth_*` means "from authentication"
   - No ambiguity about parameter source

---

## 📊 Summary

### Changes Made

| Component | File | Changes | Status |
|-----------|------|---------|--------|
| Metadata Parsing | `metadata_parser.py` | Add `context_params` field & parsing | Ready |
| Context Detection | `mutation_generator.py` | Use `auth_*` standard, explicit metadata | Ready |
| Input Generation | `input_generator.py` | Exclude auth params | Ready |
| Tests | `test_*.py` | 5 new, 2 updated, 2 deleted | Ready |

### Breaking Changes

- ❌ **REMOVED**: `input_tenant_id`, `input_user_id` auto-detection
- ❌ **REMOVED**: `input_pk_*` legacy support
- ❌ **REMOVED**: `input_created_by` legacy support
- ✅ **NEW**: `auth_*` prefix as standard
- ✅ **NEW**: Explicit `context_params` metadata

### Benefits

1. **Clear Convention**: `auth_*` is unambiguous
2. **SpecQL Alignment**: Matches SpecQL team's standard
3. **Security**: Auth params excluded from client input
4. **Explicit Control**: Metadata overrides auto-detection
5. **Simplicity**: Removed legacy compatibility code

---

## 🚀 Ready to Implement

**Total Work**: ~65 lines of code + 5 new tests + 2 updated tests

**Time Estimate**: 4-6 hours

**Risk**: LOW (no users, breaking changes OK)

**Priority**: HIGH (security + SpecQL integration)

---

**Let's go full steam ahead on `auth_*`! 🚀**
