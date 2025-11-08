# Phase 5: Auth Context Parameters - Impact Assessment

**Date**: 2025-11-08
**Reviewer**: Claude Code
**Status**: ⚠️ **REQUIRES ATTENTION**
**Priority**: HIGH (Security & Integration)

---

## 🎯 Executive Summary

The SpecQL team has requested support for **authentication context parameters** in FraiseQL. This request affects our **Phase 5 implementation** in a significant but **positive** way.

### Good News ✅
- **Phase 5 already has the foundation** for this feature!
- The `context_params` auto-detection we built is **exactly aligned** with this need
- **No major rework required** - mostly enhancement and validation

### What Needs Attention ⚠️
1. Current Phase 5 detects `input_tenant_id` / `input_user_id`
2. SpecQL will use `auth_tenant_id` / `auth_user_id` (different prefix!)
3. Need to support **explicit metadata** format: `context_params=["auth_tenant_id", ...]`
4. Need to **exclude** these params from GraphQL input schema generation
5. Need to **inject** from `context.auth` in resolvers (not implemented yet)

---

## 📋 What SpecQL Needs

### 1. New Naming Convention

**What Phase 5 Currently Expects**:
```sql
CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,    -- Phase 5 auto-detects this
    input_user_id UUID,       -- Phase 5 auto-detects this
    input_payload JSONB
);
```

**What SpecQL Will Generate**:
```sql
CREATE FUNCTION crm.qualify_lead(
    p_contact_id UUID,
    auth_tenant_id TEXT,      -- NEW: auth_ prefix (not input_)
    auth_user_id UUID         -- NEW: auth_ prefix (not input_)
);

COMMENT ON FUNCTION crm.qualify_lead IS
  '@fraiseql:mutation
   name=qualifyLead,
   context_params=["auth_tenant_id", "auth_user_id"]';  -- NEW: explicit metadata
```

### 2. Security Requirement: Exclude from GraphQL Input

**Critical**: Auth params must NOT appear in GraphQL input schema:

```graphql
# ✅ CORRECT (secure)
input QualifyLeadInput {
  contactId: UUID!
  # auth_tenant_id NOT here
  # auth_user_id NOT here
}

# ❌ WRONG (security vulnerability)
input QualifyLeadInput {
  contactId: UUID!
  authTenantId: String   # ❌ Client can fake tenant!
  authUserId: UUID       # ❌ Client can fake user!
}
```

### 3. Resolver Injection

Resolvers must inject context params from GraphQL context:

```javascript
async function qualifyLead(parent, args, context, info) {
  // Inject from context (server-controlled, trusted)
  const result = await db.query(
    'SELECT crm.qualify_lead($1, $2, $3)',
    [
      args.input.contactId,      // From GraphQL input
      context.auth.tenantId,     // From JWT (trusted)
      context.auth.userId        // From JWT (trusted)
    ]
  );
}
```

---

## 🔍 Current Phase 5 Implementation Analysis

### What Phase 5 Already Has ✅

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Method**: `_extract_context_params()` (lines 26-80)

```python
def _extract_context_params(self, function_metadata: FunctionMetadata) -> dict[str, str]:
    """Auto-detect context parameters from function signature."""
    context_params = {}

    for param in function_metadata.parameters:
        # Pattern 1: input_tenant_id → tenant_id
        if param.name == 'input_tenant_id':
            context_params['tenant_id'] = param.name

        # Pattern 2: input_user_id → user_id
        elif param.name == 'input_user_id':
            context_params['user_id'] = param.name

        # Legacy patterns...
```

**✅ Good**:
- We already detect context parameters!
- We already build a `context_params` mapping!
- We already pass it to `@fraiseql.mutation` decorator!

**⚠️ Gaps**:
1. Only detects `input_*` prefix (not `auth_*`)
2. No explicit metadata parsing (`context_params=[...]` in comments)
3. No GraphQL input schema exclusion logic
4. No resolver injection logic

---

## 📊 Gap Analysis

### Gap 1: Parameter Name Detection

**Current**:
```python
if param.name == 'input_tenant_id':  # Only input_ prefix
    context_params['tenant_id'] = param.name
```

**Needed**:
```python
# Support both input_ and auth_ prefixes
if param.name in ['input_tenant_id', 'auth_tenant_id']:
    context_params['tenant_id'] = param.name
elif param.name in ['input_user_id', 'auth_user_id']:
    context_params['user_id'] = param.name
```

**Severity**: LOW (easy fix)
**Impact**: 5 lines of code

---

### Gap 2: Explicit Metadata Parsing

**Current**: Auto-detection only (no metadata parsing)

**Needed**: Parse `context_params` from function comment:

```python
# In metadata_parser.py
@dataclass
class MutationAnnotation:
    name: str
    description: Optional[str]
    success_type: str
    failure_type: str
    input_type: Optional[str] = None
    context_params: Optional[list[str]] = None  # NEW
```

```python
def parse_mutation_annotation(self, comment: str) -> MutationAnnotation:
    # ... existing code ...

    # NEW: Parse context_params
    if 'context_params' in annotation_data:
        metadata.context_params = annotation_data['context_params']
```

**Severity**: MEDIUM (straightforward but requires changes)
**Impact**: ~20 lines of code + tests

---

### Gap 3: GraphQL Input Schema Exclusion

**Current**: `InputGenerator` includes ALL function parameters in GraphQL input

**File**: `src/fraiseql/introspection/input_generator.py`

**Current Logic** (line 253-288):
```python
def _generate_from_parameters(self, function_metadata, annotation) -> Type:
    annotations = {}
    for param in function_metadata.parameters:
        # Skip context parameters
        if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
            continue  # ✅ Good! Already skipping!
        # ... add to annotations ...
```

**⚠️ Gap**: Only skips `input_tenant_*` and `input_user_*`, not `auth_*`

**Needed**:
```python
def _generate_from_parameters(self, function_metadata, annotation, context_params=None) -> Type:
    annotations = {}

    # Get list of context param names to exclude
    exclude_params = set(context_params.values()) if context_params else set()

    for param in function_metadata.parameters:
        # Skip context parameters (from explicit list or auto-detection)
        if param.name in exclude_params:
            continue

        # Skip by naming convention (backward compat)
        if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
            continue
        if param.name.startswith('auth_tenant_') or param.name.startswith('auth_user_'):
            continue

        # ... add to annotations ...
```

**Severity**: MEDIUM (critical for security)
**Impact**: ~10 lines of code + tests

---

### Gap 4: Resolver Injection Logic

**Current**: Phase 5 doesn't touch resolver generation

**Needed**: Generate resolvers that inject from `context.auth`

**This is OUTSIDE Phase 5 scope** - Phase 5 is about **introspection**, not resolver generation.

**Who handles this**: FraiseQL's resolver/execution layer (not AutoFraiseQL introspection)

**What Phase 5 provides**:
```python
# Phase 5 already provides this!
context_params = {
    "tenant_id": "auth_tenant_id",
    "user_id": "auth_user_id"
}

@fraiseql.mutation(
    function="qualify_lead",
    context_params=context_params  # ✅ Phase 5 already does this!
)
```

**What needs to happen next** (separate from Phase 5):
The `@fraiseql.mutation` decorator must:
1. Read `context_params` mapping
2. Generate resolver that injects from `context.auth`
3. Exclude from GraphQL schema

**Severity**: HIGH (but not Phase 5's responsibility)
**Impact**: Requires changes in FraiseQL core (not introspection module)

---

## ✅ What Phase 5 Already Provides (Good News!)

### 1. Context Parameter Detection ✅

Phase 5 already detects and extracts context parameters:

```python
# phase_5_implementation/mutation_generator.py
context_params = self._extract_context_params(function_metadata)
# Result: {"tenant_id": "input_tenant_id", "user_id": "input_user_id"}
```

### 2. Context Parameter Mapping ✅

Phase 5 already passes context params to mutation decorator:

```python
decorated_mutation = mutation(
    mutation_class,
    function=function_metadata.function_name,
    schema=function_metadata.schema_name,
    context_params=context_params,  # ✅ Already here!
)
```

### 3. Input Schema Exclusion (Partial) ✅

Phase 5 already excludes `input_tenant_*` and `input_user_*` from input generation:

```python
# input_generator.py (line 258-261)
if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
    continue  # ✅ Already skipping!
```

---

## 🔧 Required Changes to Phase 5

### Change 1: Support `auth_*` Prefix (SMALL)

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Line**: 56-70

**Current**:
```python
if param.name == 'input_tenant_id':
    context_params['tenant_id'] = param.name
elif param.name == 'input_user_id':
    context_params['user_id'] = param.name
```

**Change to**:
```python
# Support both input_ and auth_ prefixes
if param.name in ['input_tenant_id', 'auth_tenant_id']:
    context_params['tenant_id'] = param.name
elif param.name in ['input_user_id', 'auth_user_id']:
    context_params['user_id'] = param.name
```

**Impact**: 5 lines of code
**Tests**: Update `test_extract_context_params_new_convention` to test `auth_*` prefix

---

### Change 2: Parse `context_params` from Metadata (MEDIUM)

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Add to `MutationAnnotation` dataclass**:
```python
@dataclass
class MutationAnnotation:
    # ... existing fields ...
    context_params: Optional[list[str]] = None  # NEW
```

**Update `parse_mutation_annotation()` method**:
```python
def parse_mutation_annotation(self, comment: str) -> MutationAnnotation:
    # ... existing parsing logic ...

    # Parse context_params array if present
    context_params = None
    if 'context_params' in data:
        # Support both: context_params=["a", "b"] or context_params: [a, b]
        context_params = data['context_params']

    return MutationAnnotation(
        # ... existing fields ...
        context_params=context_params
    )
```

**Impact**: 15 lines of code
**Tests**: New test `test_parse_mutation_annotation_with_context_params`

---

### Change 3: Use Explicit Metadata in Detection (MEDIUM)

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Update `_extract_context_params()` to accept annotation**:
```python
def _extract_context_params(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation  # NEW: Accept annotation
) -> dict[str, str]:
    """
    Extract context parameters.

    Priority:
    1. Explicit metadata (annotation.context_params)
    2. Auto-detection by naming convention
    """

    # Priority 1: Explicit metadata
    if annotation.context_params:
        context_params = {}
        for param_name in annotation.context_params:
            # Find the actual parameter in function
            param = next(
                (p for p in function_metadata.parameters if p.name == param_name),
                None
            )
            if param:
                # Map to context key (remove auth_ or input_ prefix)
                context_key = param_name.replace('auth_', '').replace('input_', '')
                context_params[context_key] = param_name
        return context_params

    # Priority 2: Auto-detection (existing logic)
    # ... existing auto-detection code ...
```

**Impact**: 20 lines of code
**Tests**: New test `test_extract_context_params_explicit_metadata`

---

### Change 4: Exclude `auth_*` from Input Generation (SMALL)

**File**: `src/fraiseql/introspection/input_generator.py`

**Line**: 258-261

**Current**:
```python
if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
    continue
```

**Change to**:
```python
# Skip context parameters by naming convention
if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
    continue
if param.name.startswith('auth_tenant_') or param.name.startswith('auth_user_'):
    continue
```

**Better approach** (pass context_params explicitly):
```python
def _generate_from_parameters(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation,
    context_params: dict[str, str] = None  # NEW
) -> Type:
    # Get set of parameter names to exclude
    exclude_params = set(context_params.values()) if context_params else set()

    for param in function_metadata.parameters:
        # Skip if in explicit context_params list
        if param.name in exclude_params:
            continue

        # Skip by naming convention (backward compat)
        if param.name.startswith(('input_tenant_', 'input_user_', 'auth_tenant_', 'auth_user_')):
            continue

        # ... rest of logic ...
```

**Impact**: 10 lines of code
**Tests**: Update existing tests

---

## 📊 Summary of Required Changes

| Component | File | Change | Lines | Difficulty | Priority |
|-----------|------|--------|-------|------------|----------|
| Context detection | `mutation_generator.py` | Add `auth_*` support | 5 | LOW | HIGH |
| Metadata parsing | `metadata_parser.py` | Parse `context_params` | 15 | MEDIUM | HIGH |
| Detection logic | `mutation_generator.py` | Use explicit metadata | 20 | MEDIUM | HIGH |
| Input generation | `input_generator.py` | Exclude auth params | 10 | LOW | HIGH |
| **Total** | **3 files** | **4 changes** | **~50 lines** | **MEDIUM** | **HIGH** |

### Testing Impact
- **New tests**: ~5 new unit tests
- **Updated tests**: ~3 existing tests need updates
- **Total test effort**: ~8 test cases

---

## 🎯 Comparison: Current vs Needed

### Current Phase 5 Implementation

```python
# What Phase 5 does now
CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,
    input_user_id UUID,
    input_payload JSONB
);

# Auto-detects: context_params = {
#   "tenant_id": "input_tenant_id",
#   "user_id": "input_user_id"
# }

# Generates:
@fraiseql.mutation(
    function="create_contact",
    context_params={"tenant_id": "input_tenant_id", "user_id": "input_user_id"}
)
```

### SpecQL Requirements

```sql
# What SpecQL will generate
CREATE FUNCTION crm.qualify_lead(
    p_contact_id UUID,
    auth_tenant_id TEXT,    -- Different prefix!
    auth_user_id UUID       -- Different prefix!
);

COMMENT ON FUNCTION crm.qualify_lead IS
  '@fraiseql:mutation
   name=qualifyLead,
   context_params=["auth_tenant_id", "auth_user_id"]';  -- Explicit!

# Should detect: context_params = {
#   "tenant_id": "auth_tenant_id",
#   "user_id": "auth_user_id"
# }

# Should generate:
@fraiseql.mutation(
    function="qualify_lead",
    context_params={"tenant_id": "auth_tenant_id", "user_id": "auth_user_id"}
)

# Should exclude auth params from GraphQL input schema
```

---

## ✅ Action Items

### Immediate (Required for SpecQL Integration)

1. **Update `_extract_context_params()`** to support `auth_*` prefix
   - File: `mutation_generator.py`
   - Lines: ~5
   - Priority: HIGH
   - Difficulty: LOW

2. **Add `context_params` field to `MutationAnnotation`**
   - File: `metadata_parser.py`
   - Lines: ~1 (dataclass field)
   - Priority: HIGH
   - Difficulty: LOW

3. **Parse `context_params` from metadata**
   - File: `metadata_parser.py`
   - Lines: ~15
   - Priority: HIGH
   - Difficulty: MEDIUM

4. **Use explicit metadata in detection**
   - File: `mutation_generator.py`
   - Lines: ~20
   - Priority: HIGH
   - Difficulty: MEDIUM

5. **Exclude auth params from input generation**
   - File: `input_generator.py`
   - Lines: ~10
   - Priority: HIGH (security!)
   - Difficulty: LOW

6. **Write tests for new behavior**
   - Files: `test_*.py`
   - Tests: ~8 new/updated tests
   - Priority: HIGH
   - Difficulty: MEDIUM

### Future (Not Phase 5 Scope)

7. **Resolver generation with context injection**
   - Component: FraiseQL core (not introspection)
   - Owner: FraiseQL resolver generator team
   - Input from Phase 5: `context_params` mapping ✅ (already provided)

8. **GraphQL schema generation exclusion**
   - Component: FraiseQL core (not introspection)
   - Owner: FraiseQL schema generator team
   - Input from Phase 5: `context_params` list ✅ (already provided)

9. **Context validation in resolvers**
   - Component: FraiseQL runtime
   - Owner: FraiseQL execution team
   - Requirement: Validate `context.auth` exists before calling PostgreSQL

---

## 🚦 Risk Assessment

### Risk 1: Breaking Changes (LOW)

**Issue**: Adding `auth_*` support might affect existing code

**Mitigation**:
- Keep `input_*` support (backward compatible)
- Auto-detection still works for legacy functions
- Only new behavior when `auth_*` or explicit metadata present

**Verdict**: ✅ LOW RISK (fully backward compatible)

---

### Risk 2: Security (MEDIUM → HIGH if not addressed)

**Issue**: If auth params not excluded from GraphQL schema, major security vulnerability

**Mitigation**:
- **MUST** exclude context params from GraphQL input schema
- **MUST** inject from server context only
- **MUST** validate context exists

**Verdict**: ⚠️ HIGH PRIORITY - Critical for security

---

### Risk 3: Timeline (MEDIUM)

**Issue**: SpecQL needs this by Week 5

**Current Status**: Week 1 (Nov 8)

**Required Work**: ~50 lines of code + 8 tests = ~4-6 hours

**Mitigation**:
- Phase 5 foundation already built ✅
- Changes are small and focused
- Can be completed in 1-2 days

**Verdict**: ✅ MANAGEABLE - Should be done by Week 2-3

---

## 📋 Recommended Implementation Plan

### Phase 5.6: Auth Context Enhancement

**Goal**: Add explicit `context_params` metadata support and `auth_*` prefix detection

**Time Estimate**: 4-6 hours

**Steps**:

1. **Update Metadata Parser** (1 hour)
   - Add `context_params` field to `MutationAnnotation`
   - Parse `context_params` from function comment
   - Write tests

2. **Update Context Detection** (1 hour)
   - Support `auth_*` prefix in `_extract_context_params()`
   - Use explicit metadata if present
   - Write tests

3. **Update Input Generation** (1 hour)
   - Pass `context_params` to `_generate_from_parameters()`
   - Exclude context params from GraphQL input
   - Write tests

4. **Integration Testing** (1-2 hours)
   - Test with SpecQL-style function signatures
   - Verify auth params excluded from schema
   - Security testing (client cannot override)

5. **Documentation** (1 hour)
   - Update Phase 5 docs
   - Add examples of `auth_*` usage
   - Document `context_params` metadata format

---

## 🎯 Conclusion

### Summary

**Good News** ✅:
- Phase 5 already has **90% of what SpecQL needs**!
- The `context_params` detection and mapping we built is perfectly aligned
- Only need **small enhancements** to support SpecQL's specific conventions

**Required Work** ⚠️:
- Support `auth_*` prefix (in addition to `input_*`)
- Parse explicit `context_params` metadata from function comments
- Ensure auth params excluded from GraphQL input schema

**Effort**: ~50 lines of code + 8 tests = **4-6 hours of work**

**Timeline**: Can complete by **Week 2-3** (SpecQL needs by Week 5) ✅

**Risk**: LOW - Changes are small, focused, and backward compatible

### Recommendation

**✅ PROCEED WITH PHASE 5.6: AUTH CONTEXT ENHANCEMENT**

This is a **natural extension** of Phase 5 that:
1. Supports SpecQL's security requirements
2. Maintains backward compatibility
3. Completes the context parameter feature
4. Requires minimal additional work

**Next Steps**:
1. Review this assessment with team
2. Approve Phase 5.6 scope
3. Implement changes (4-6 hours)
4. Test with SpecQL examples
5. Coordinate with SpecQL team for integration testing

---

**Prepared by**: Claude Code
**Date**: 2025-11-08
**Status**: Ready for Implementation
**Priority**: HIGH (Security & Integration)
