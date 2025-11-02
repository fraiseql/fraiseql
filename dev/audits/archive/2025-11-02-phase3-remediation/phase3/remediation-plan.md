# Phase 3 Documentation Fixes - Detailed Remediation Plan

## 🎯 Purpose
This document provides step-by-step instructions for fixing the 13 critical and high-priority documentation violations found in Phase 3 Code Example Validation.

**Target Audience**: Junior developer or AI agent executing fixes
**Estimated Time**: 4-6 hours
**Files to Modify**: 3 documentation files
**Validation**: Automated tests after each phase

---

## 📋 Prerequisites

### Tools Required
- Text editor with markdown support
- Python 3.13+ installed
- Git for version control
- Bash shell

### Files You'll Modify
1. `docs/core/database-api.md` (963 lines)
2. `docs/performance/caching.md` (990 lines)
3. `docs/advanced/authentication.md` (993 lines)

### Reference Materials
- `dev/audits/phase3/fraiseql-pattern-violations.md` - Detailed violation report
- `dev/architecture/graphql-mutation-payload-patterns.md` - GraphQL patterns guide
- `dev/audits/documentation-quality-audit-plan.md` - Quality standards

---

## 🔄 Phased Execution Plan

### Overview
```
Phase 1: CRITICAL Fixes (2 issues)     → 1 hour
Phase 2: HIGH Priority Fixes (11 issues) → 3 hours
Phase 3: Validation (All fixes)        → 30 minutes
Phase 4: Git Commit & Documentation    → 30 minutes
```

---

## 📍 PHASE 1: CRITICAL SECURITY & PATTERN FIXES

**Duration**: 1 hour
**Priority**: MUST complete before release
**Files**: 3 files to modify

---

### Fix 1.1: Add Context Extraction Pattern to database-api.md

**File**: `docs/core/database-api.md`
**Line**: 78-89
**Issue**: Missing fundamental context extraction pattern

#### Step-by-Step Instructions

1. **Open the file**:
   ```bash
   # Open in your editor
   vim docs/core/database-api.md
   # OR
   code docs/core/database-api.md
   ```

2. **Navigate to line 78** (search for "get_my_profile")

3. **Find this code block**:
   ```python
   @query
   async def get_my_profile(info: GraphQLResolveInfo) -> User:
       """Get current user's profile."""
       user_context = info.context["user"]
       if not user_context:
           raise AuthenticationError("Not authenticated")

       # user_context is UserContext instance
       return await fetch_user_by_id(user_context.user_id)
   ```

4. **Replace with this EXACT code**:
   ```python
   @query
   async def get_my_profile(info: GraphQLResolveInfo) -> User:
       """Get current user's profile."""
       # Extract context early (standard pattern)
       user = info.context["user"]
       db = info.context["db"]
       tenant_id = info.context["tenant_id"]

       if not user:
           raise AuthenticationError("Not authenticated")

       # Use repository to fetch user data
       return await db.find_one("v_user", id=user.user_id)
   ```

5. **Save the file**

6. **Verify the change**:
   ```bash
   # Check the diff
   git diff docs/core/database-api.md | head -30
   ```

**Expected Output**:
```diff
-    user_context = info.context["user"]
-    if not user_context:
+    # Extract context early (standard pattern)
+    user = info.context["user"]
+    db = info.context["db"]
+    tenant_id = info.context["tenant_id"]
+
+    if not user:
         raise AuthenticationError("Not authenticated")

-    # user_context is UserContext instance
-    return await fetch_user_by_id(user_context.user_id)
+    # Use repository to fetch user data
+    return await db.find_one("v_user", id=user.user_id)
```

---

### Fix 1.2: Add Context Extraction Pattern to authentication.md

**File**: `docs/advanced/authentication.md`
**Line**: 126-139
**Issue**: Same missing pattern (duplicate of Fix 1.1)

#### Step-by-Step Instructions

1. **Open the file**:
   ```bash
   code docs/advanced/authentication.md
   ```

2. **Navigate to line 126** (search for "get_my_profile")

3. **Apply the SAME fix as Fix 1.1**

4. **Save and verify**:
   ```bash
   git diff docs/advanced/authentication.md | head -30
   ```

---

### Fix 1.3: Add Prominent Security Warning to caching.md

**File**: `docs/performance/caching.md`
**Line**: 255-262
**Issue**: Security anti-pattern shown without clear warning

#### Step-by-Step Instructions

1. **Open the file**:
   ```bash
   code docs/performance/caching.md
   ```

2. **Navigate to line 255** (search for "Missing tenant_id")

3. **Find this code block**:
   ```python
   # ⚠️ SECURITY ISSUE: Missing tenant_id
   base_repo = FraiseQLRepository(pool, context={})

   cached_repo = CachedRepository(base_repo, result_cache)
   users = await cached_repo.find("users", status="active")
   # Cache key: "fraiseql:users:status:active"  ← SHARED ACROSS TENANTS!
   ```

4. **Replace with this code** (adds prominent warning):
   ```python
   # 🚨 CRITICAL SECURITY VIOLATION - DO NOT USE IN PRODUCTION
   # This example shows what happens when tenant_id is missing.
   # Missing tenant_id causes CROSS-TENANT DATA LEAKAGE!

   # ❌ WRONG: No tenant_id in context
   base_repo = FraiseQLRepository(pool, context={})

   cached_repo = CachedRepository(base_repo, result_cache)
   users = await cached_repo.find("users", status="active")
   # Cache key: "fraiseql:users:status:active"
   # ⚠️ This cache key is SHARED ACROSS ALL TENANTS - SECURITY VIOLATION!

   # ✅ CORRECT: Always include tenant_id
   base_repo = FraiseQLRepository(
       pool,
       context={"tenant_id": tenant_id}  # REQUIRED for multi-tenant apps
   )
   cached_repo = CachedRepository(base_repo, result_cache)
   users = await cached_repo.find("users", status="active")
   # Cache key: "fraiseql:tenant_123:users:status:active"  ✅ Isolated per tenant
   ```

5. **Save and verify**:
   ```bash
   git diff docs/performance/caching.md | grep -A 10 -B 5 "CRITICAL SECURITY"
   ```

---

### Phase 1 Validation

**Run validation after completing all Fix 1.x**:

```bash
# Extract and validate the fixed code blocks
./scripts/validate-docs-code-examples.sh

# Check that critical patterns are now present
grep -n "db = info.context\[\"db\"\]" docs/core/database-api.md
grep -n "db = info.context\[\"db\"\]" docs/advanced/authentication.md
grep -n "CRITICAL SECURITY VIOLATION" docs/performance/caching.md
```

**Expected Output**:
```
docs/core/database-api.md:82:    db = info.context["db"]
docs/advanced/authentication.md:129:    db = info.context["db"]
docs/performance/caching.md:257:# 🚨 CRITICAL SECURITY VIOLATION - DO NOT USE IN PRODUCTION
```

**✅ Phase 1 Complete** if all three patterns are found.

---

## 📍 PHASE 2: HIGH PRIORITY PATTERN FIXES

**Duration**: 3 hours
**Priority**: Should complete before release
**Files**: 2 files to modify

---

### Fix 2.1: Add Standard Query Parameters Example

**File**: `docs/core/database-api.md`
**Location**: After line 49 (after basic repository examples)
**Issue**: Missing standard GraphQL query signature demonstration

#### Step-by-Step Instructions

1. **Open the file**:
   ```bash
   code docs/core/database-api.md
   ```

2. **Navigate to line 49** (search for "find_rust")

3. **After the existing example, ADD this new section**:

```markdown

### Standard GraphQL Query Pattern

When writing GraphQL queries (not direct repository calls), always include standard parameters for filtering, pagination, and ordering:

```python
from fraiseql import query
from fraiseql.db.pagination import (
    QueryOptions,
    PaginationInput,
    OrderByInstructions,
    OrderByInstruction,
    OrderDirection
)
from fraiseql.filters import UserWhereInput

@query
async def users(
    info,
    where: UserWhereInput | None = None,
    limit: int | None = None,
    offset: int | None = None,
    order_by: list[OrderByInstruction] | None = None
) -> list[User]:
    """List users with filtering, pagination, and ordering."""
    # Extract context (standard pattern)
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    # Build query options
    options = QueryOptions(
        filters=where,
        pagination=PaginationInput(limit=limit, offset=offset),
        order_by=OrderByInstructions(instructions=order_by) if order_by else None
    )

    # Execute query
    results, total = await db.select_from_json_view(
        tenant_id=tenant_id,
        view_name="v_user",
        options=options
    )

    return results
```

**Key Points**:
- **`where`**: Typed filter input (not plain dict)
- **`limit`/`offset`**: Standard pagination parameters
- **`order_by`**: Ordering instructions for consistent results
- **Always extract `db` and `tenant_id`** from context first

**GraphQL Usage**:
```graphql
query {
  users(
    where: { status: { eq: "active" } }
    limit: 10
    offset: 0
    orderBy: [{ field: "created_at", direction: DESC }]
  ) {
    id
    name
    email
  }
}
```

```

4. **Save the file**

5. **Verify the addition**:
   ```bash
   git diff docs/core/database-api.md | grep -A 5 "Standard GraphQL Query Pattern"
   ```

---

### Fix 2.2: Add Default Ordering Documentation

**File**: `docs/core/database-api.md`
**Location**: After the new "Standard GraphQL Query Pattern" section
**Issue**: Default ordering requirement not documented

#### Step-by-Step Instructions

1. **After the GraphQL usage example from Fix 2.1, ADD**:

```markdown

### ⚠️ Default Ordering for List Queries

**IMPORTANT**: All list queries MUST have default ordering for consistent pagination.

```python
@query
async def users(
    info,
    where: UserWhereInput | None = None,
    limit: int | None = None,
    offset: int | None = None,
    order_by: list[OrderByInstruction] | None = None
) -> list[User]:
    """List users with default ordering."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    # ✅ CORRECT: Default ordering if not specified
    if order_by is None:
        order_by = [
            OrderByInstruction(field="created_at", direction=OrderDirection.DESC)
        ]

    options = QueryOptions(
        filters=where,
        pagination=PaginationInput(limit=limit, offset=offset),
        order_by=OrderByInstructions(instructions=order_by)
    )

    results, total = await db.select_from_json_view(
        tenant_id=tenant_id,
        view_name="v_user",
        options=options
    )

    return results
```

**Why Default Ordering Matters**:
- Without ordering, pagination results are **non-deterministic**
- Database may return rows in different order between requests
- Users may see duplicates or miss items when paginating

**Best Practices**:
- Use `created_at DESC` for "most recent first" lists
- Use `name ASC` for alphabetical lists
- Use `id ASC` for stable ordering

```

2. **Save and verify**:
   ```bash
   git diff docs/core/database-api.md | grep "Default Ordering"
   ```

---

### Fix 2.3: Add Complete Context Structure Example

**File**: `docs/performance/caching.md`
**Line**: 92-106
**Issue**: Context example missing `user` object

#### Step-by-Step Instructions

1. **Open the file**:
   ```bash
   code docs/performance/caching.md
   ```

2. **Navigate to line 92** (search for "get_graphql_context")

3. **Find this code block**:
   ```python
   def get_graphql_context(request: Request) -> dict:
       base_repo = FraiseQLRepository(
           pool=app.state.pool,
           context={
               "tenant_id": request.state.tenant_id,
               "user_id": request.state.user_id
           }
       )

       return {
           "request": request,
           "db": CachedRepository(base_repo, app.state.result_cache),
           "tenant_id": request.state.tenant_id
       }
   ```

4. **Replace with this COMPLETE context structure**:
   ```python
   async def get_graphql_context(request: Request) -> dict:
       """Build complete GraphQL context with all required keys."""
       # Extract tenant and user from request state
       tenant_id = request.state.tenant_id
       user = request.state.user  # UserContext instance (or None)

       # Create repository with tenant context
       base_repo = FraiseQLRepository(
           pool=app.state.pool,
           context={
               "tenant_id": tenant_id,
               "user_id": user.user_id if user else None
           }
       )

       # Wrap with caching layer
       cached_db = CachedRepository(
           base_repository=base_repo,
           cache=app.state.result_cache
       )

       # Return complete context structure
       return {
           "request": request,          # FastAPI/Starlette request
           "db": cached_db,              # Repository with caching
           "tenant_id": tenant_id,       # Required for multi-tenancy
           "user": user                  # UserContext for auth decorators
       }
   ```

5. **Save and verify**:
   ```bash
   git diff docs/performance/caching.md | grep -A 5 '"user":'
   ```

---

### Fix 2.4-2.6: Add Type Naming Convention Documentation

**File**: `docs/core/database-api.md`
**Location**: Near line 452 (before WhereInput examples)
**Issue**: Type naming suffixes not explained

#### Step-by-Step Instructions

1. **Navigate to line 452** (search for "create_graphql_where_input")

2. **BEFORE the existing WhereInput example, ADD this section**:

```markdown

### Type Naming Conventions

FraiseQL uses consistent naming patterns for generated types:

| Type Category | Suffix | Example | Usage |
|--------------|--------|---------|-------|
| **Input Types** | `Input` | `CreateUserInput` | Mutation inputs |
| **Filter Types** | `WhereInput` | `UserWhereInput` | Query filtering |
| **Field Filters** | `Filter` | `StringFilter`, `IntFilter` | Individual field filters |
| **Success Types** | `Success` | `CreateUserSuccess` | Successful mutation result |
| **Error Types** | `Error` | `CreateUserError` | Failed mutation result |
| **Ordering** | `OrderByInstruction` | - | Sorting configuration |

**Example - Complete Type Usage**:

```python
from fraiseql.sql import create_graphql_where_input
from fraiseql.filters import StringFilter, IntFilter, BoolFilter

# Generated WhereInput types (always end with 'WhereInput')
UserWhereInput = create_graphql_where_input(User)
MachineWhereInput = create_graphql_where_input(Machine)

# Field filters always end with 'Filter'
where = UserWhereInput(
    name=StringFilter(contains="John"),      # StringFilter for text
    age=IntFilter(gte=18),                   # IntFilter for numbers
    is_active=BoolFilter(eq=True)            # BoolFilter for booleans
)

results = await db.find("v_user", where=where)
```

**Type Safety Benefits**:
- ✅ IDE autocomplete for filter fields
- ✅ Type checking catches field name typos
- ✅ Clear documentation of available filters
- ✅ Prevents invalid filter combinations

```

3. **Save and verify**:
   ```bash
   git diff docs/core/database-api.md | grep "Type Naming Conventions"
   ```

---

### Fix 2.7-2.10: Standardize Variable Naming (repo → db)

**Files**: `docs/core/database-api.md`, `docs/performance/caching.md`
**Issue**: Inconsistent use of `repo` instead of `db`
**Count**: 7 instances across 2 files

#### Step-by-Step Instructions

**This is a SEARCH AND REPLACE operation**:

1. **Create a backup** (safety first):
   ```bash
   cp docs/core/database-api.md docs/core/database-api.md.backup
   cp docs/performance/caching.md docs/performance/caching.md.backup
   ```

2. **Open database-api.md and search for all instances**:
   ```bash
   grep -n "repo = " docs/core/database-api.md
   ```

3. **For EACH instance found, manually verify and replace**:

   **Pattern to find**:
   ```python
   repo = PsycopgRepository(connection_pool)
   # ... later ...
   results = await repo.find("users")
   ```

   **Replace with**:
   ```python
   db = PsycopgRepository(connection_pool)
   # ... later ...
   results = await db.find("users")
   ```

4. **IMPORTANT**: Only replace in **code blocks**, NOT in prose text

5. **Automated replacement** (use with caution):
   ```bash
   # In code blocks only - review each change!
   # This replaces 'repo =' with 'db ='
   sed -i 's/repo = PsycopgRepository/db = PsycopgRepository/g' docs/core/database-api.md
   sed -i 's/repo = FraiseQLRepository/db = FraiseQLRepository/g' docs/core/database-api.md
   sed -i 's/await repo\./await db./g' docs/core/database-api.md
   ```

6. **Repeat for caching.md**:
   ```bash
   sed -i 's/cached_repo =/db =/g' docs/performance/caching.md
   sed -i 's/await cached_repo\./await db./g' docs/performance/caching.md
   sed -i 's/base_repo =/db =/g' docs/performance/caching.md
   ```

7. **CRITICAL: Review ALL changes**:
   ```bash
   git diff docs/core/database-api.md | less
   git diff docs/performance/caching.md | less
   ```

8. **If any change looks wrong, revert**:
   ```bash
   # Revert a single file
   git checkout -- docs/core/database-api.md
   # Restore from backup
   cp docs/core/database-api.md.backup docs/core/database-api.md
   ```

9. **Expected changes**: ~7 instances across both files

**Validation**:
```bash
# Should find ZERO instances of 'repo =' in code blocks
grep -n "repo = " docs/core/database-api.md docs/performance/caching.md

# Should find MANY instances of 'db =' in code blocks
grep -n "db = " docs/core/database-api.md docs/performance/caching.md | wc -l
```

---

### Fix 2.11: Update Typed WhereInput Examples

**File**: `docs/core/database-api.md`
**Location**: Line 407-415 and other dict-based filter examples
**Issue**: Using dicts instead of typed WhereInput (12 instances)

#### Step-by-Step Instructions

1. **Navigate to line 407** (search for "machine.*name.*eq.*Server")

2. **Find dict-based filter examples like this**:
   ```python
   where = {
       "machine": {
           "name": {"eq": "Server-01"}
       }
   }
   results = await repo.find("allocations", where=where)
   ```

3. **Add a COMPARISON showing both approaches**:

```markdown

### Dict-Based vs Typed Filters

FraiseQL supports both dict-based and typed filter inputs. **Typed inputs are recommended** for type safety.

#### Dict-Based Filters (Simple, but no type checking)

```python
# ⚠️ Works, but no IDE autocomplete or type checking
where = {
    "machine": {
        "name": {"eq": "Server-01"}
    }
}
results = await db.find("v_allocation", where=where)
# SQL: WHERE data->'machine'->>'name' = 'Server-01'
```

#### Typed Filters (Recommended - Type Safe)

```python
# ✅ RECOMMENDED: Full type safety and IDE support
from fraiseql.sql import create_graphql_where_input
from fraiseql.filters import StringFilter

AllocationWhereInput = create_graphql_where_input(Allocation)
MachineWhereInput = create_graphql_where_input(Machine)

where = AllocationWhereInput(
    machine=MachineWhereInput(
        name=StringFilter(eq="Server-01")
    )
)
results = await db.find("v_allocation", where=where)
# Same SQL, but with type checking!
```

**Benefits of Typed Filters**:
- ✅ IDE autocomplete shows available fields
- ✅ Type checker catches typos: `nmae` → error
- ✅ Invalid operators rejected: `StringFilter(gte=...)` → error
- ✅ Better documentation through types

**When to Use Each**:
- **Typed**: Production code, complex filters, team projects
- **Dict**: Quick scripts, simple filters, prototyping

```

4. **Save and verify**:
   ```bash
   git diff docs/core/database-api.md | grep -A 5 "Dict-Based vs Typed"
   ```

---

### Phase 2 Validation

**Run validation after completing all Fix 2.x**:

```bash
# Re-run code extraction and validation
./scripts/validate-docs-code-examples.sh

# Verify standard query parameters example added
grep -n "Standard GraphQL Query Pattern" docs/core/database-api.md

# Verify default ordering documentation added
grep -n "Default Ordering for List Queries" docs/core/database-api.md

# Verify complete context structure
grep -n '"user": user' docs/performance/caching.md

# Verify type naming conventions documented
grep -n "Type Naming Conventions" docs/core/database-api.md

# Verify variable naming standardized
grep -c "db = " docs/core/database-api.md  # Should be > 10
grep -c "repo = " docs/core/database-api.md  # Should be 0

# Verify typed filter examples added
grep -n "Dict-Based vs Typed" docs/core/database-api.md
```

**Expected Output**:
```
docs/core/database-api.md:55:### Standard GraphQL Query Pattern
docs/core/database-api.md:115:### ⚠️ Default Ordering for List Queries
docs/performance/caching.md:114:           "user": user                  # UserContext for auth decorators
docs/core/database-api.md:455:### Type Naming Conventions
docs/core/database-api.md:15  (count of 'db = ')
docs/core/database-api.md:0   (count of 'repo = ')
docs/core/database-api.md:510:### Dict-Based vs Typed Filters
```

**✅ Phase 2 Complete** if all checks pass.

---

## 📍 PHASE 3: VALIDATION & TESTING

**Duration**: 30 minutes
**Priority**: MUST complete to verify all fixes

---

### Validation 3.1: Re-run Code Example Extraction

```bash
# Clean previous extraction
rm -rf dev/audits/phase3/extracted_code/
rm -f dev/audits/phase3/code_validation_report.md

# Re-run validation script
./scripts/validate-docs-code-examples.sh
```

**Expected Improvements**:
- Previous syntax errors: 129
- Expected after fixes: < 110 (reduced by ~15%)
- New valid blocks from added examples

---

### Validation 3.2: Verify Critical Patterns Present

```bash
# Create validation script
cat > /tmp/validate-fixes.sh << 'EOF'
#!/bin/bash

echo "🔍 Validating Phase 3 Fixes"
echo "==========================="
echo ""

ERRORS=0

# Fix 1.1 & 1.2: Context extraction pattern
echo "✓ Checking context extraction pattern..."
if grep -q 'db = info.context\["db"\]' docs/core/database-api.md && \
   grep -q 'db = info.context\["db"\]' docs/advanced/authentication.md; then
    echo "  ✅ Context extraction pattern found"
else
    echo "  ❌ Context extraction pattern MISSING"
    ERRORS=$((ERRORS + 1))
fi

# Fix 1.3: Security warning
echo "✓ Checking security warning..."
if grep -q 'CRITICAL SECURITY VIOLATION' docs/performance/caching.md; then
    echo "  ✅ Security warning added"
else
    echo "  ❌ Security warning MISSING"
    ERRORS=$((ERRORS + 1))
fi

# Fix 2.1: Standard query parameters
echo "✓ Checking standard query parameters..."
if grep -q 'Standard GraphQL Query Pattern' docs/core/database-api.md; then
    echo "  ✅ Standard query parameters documented"
else
    echo "  ❌ Standard query parameters MISSING"
    ERRORS=$((ERRORS + 1))
fi

# Fix 2.2: Default ordering
echo "✓ Checking default ordering..."
if grep -q 'Default Ordering for List Queries' docs/core/database-api.md; then
    echo "  ✅ Default ordering documented"
else
    echo "  ❌ Default ordering MISSING"
    ERRORS=$((ERRORS + 1))
fi

# Fix 2.3: Complete context structure
echo "✓ Checking complete context structure..."
if grep -q '"user": user' docs/performance/caching.md; then
    echo "  ✅ Complete context structure shown"
else
    echo "  ❌ Complete context MISSING"
    ERRORS=$((ERRORS + 1))
fi

# Fix 2.4-2.6: Type naming conventions
echo "✓ Checking type naming conventions..."
if grep -q 'Type Naming Conventions' docs/core/database-api.md; then
    echo "  ✅ Type naming conventions documented"
else
    echo "  ❌ Type naming conventions MISSING"
    ERRORS=$((ERRORS + 1))
fi

# Fix 2.7-2.10: Variable naming
echo "✓ Checking variable naming standardization..."
REPO_COUNT=$(grep -c 'repo = Psycopg\|repo = FraiseQL' docs/core/database-api.md || echo "0")
if [ "$REPO_COUNT" -eq 0 ]; then
    echo "  ✅ Variable naming standardized (no 'repo =' found)"
else
    echo "  ❌ Found $REPO_COUNT instances of 'repo =' (should be 'db =')"
    ERRORS=$((ERRORS + 1))
fi

# Fix 2.11: Typed WhereInput examples
echo "✓ Checking typed filter examples..."
if grep -q 'Dict-Based vs Typed' docs/core/database-api.md; then
    echo "  ✅ Typed filter examples added"
else
    echo "  ❌ Typed filter examples MISSING"
    ERRORS=$((ERRORS + 1))
fi

echo ""
echo "==========================="
if [ $ERRORS -eq 0 ]; then
    echo "✅ ALL VALIDATIONS PASSED"
    exit 0
else
    echo "❌ FOUND $ERRORS VALIDATION ERRORS"
    exit 1
fi
EOF

chmod +x /tmp/validate-fixes.sh
/tmp/validate-fixes.sh
```

**Expected Output**:
```
🔍 Validating Phase 3 Fixes
===========================

✓ Checking context extraction pattern...
  ✅ Context extraction pattern found
✓ Checking security warning...
  ✅ Security warning added
✓ Checking standard query parameters...
  ✅ Standard query parameters documented
✓ Checking default ordering...
  ✅ Default ordering documented
✓ Checking complete context structure...
  ✅ Complete context structure shown
✓ Checking type naming conventions...
  ✅ Type naming conventions documented
✓ Checking variable naming standardization...
  ✅ Variable naming standardized (no 'repo =' found)
✓ Checking typed filter examples...
  ✅ Typed filter examples added

===========================
✅ ALL VALIDATIONS PASSED
```

---

### Validation 3.3: Manual Review Checklist

**Human review required** - check each item:

- [ ] **Fix 1.1**: `docs/core/database-api.md:~80` shows `db = info.context["db"]`
- [ ] **Fix 1.2**: `docs/advanced/authentication.md:~128` shows `db = info.context["db"]`
- [ ] **Fix 1.3**: `docs/performance/caching.md:~257` has 🚨 CRITICAL SECURITY warning
- [ ] **Fix 2.1**: New "Standard GraphQL Query Pattern" section exists
- [ ] **Fix 2.2**: New "Default Ordering for List Queries" section exists
- [ ] **Fix 2.3**: `get_graphql_context` returns `"user": user` in context dict
- [ ] **Fix 2.4-6**: "Type Naming Conventions" table with Input/Filter/Success/Error suffixes
- [ ] **Fix 2.7-10**: No instances of `repo =` in code blocks (only `db =`)
- [ ] **Fix 2.11**: "Dict-Based vs Typed Filters" comparison section exists

**If ANY item fails, return to the corresponding fix and retry.**

---

### Validation 3.4: Git Diff Review

```bash
# Review all changes
git diff docs/core/database-api.md | less
git diff docs/performance/caching.md | less
git diff docs/advanced/authentication.md | less

# Count of changes per file
echo "Lines changed per file:"
git diff --stat
```

**Expected Stats**:
```
docs/core/database-api.md        | ~150 insertions, ~20 deletions
docs/performance/caching.md      | ~50 insertions, ~15 deletions
docs/advanced/authentication.md  | ~10 insertions, ~5 deletions
```

---

## 📍 PHASE 4: GIT COMMIT & DOCUMENTATION

**Duration**: 30 minutes
**Priority**: Document the fixes for team review

---

### Commit 4.1: Stage the Changes

```bash
# Stage only the documentation files
git add docs/core/database-api.md
git add docs/performance/caching.md
git add docs/advanced/authentication.md

# Verify staged files
git status
```

**Expected Output**:
```
On branch dev
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
	modified:   docs/advanced/authentication.md
	modified:   docs/core/database-api.md
	modified:   docs/performance/caching.md
```

---

### Commit 4.2: Create Detailed Commit

```bash
git commit -m "$(cat <<'EOF'
docs: fix Phase 3 critical and high-priority pattern violations

Fixes 13 critical and high-priority documentation issues found in
Phase 3 Code Example Validation audit.

## CRITICAL Fixes (2 issues)

1. **Context Extraction Pattern** (database-api.md, authentication.md)
   - Added db/tenant_id extraction from info.context
   - Shows fundamental FraiseQL resolver pattern
   - Fixes: #violation-1, #violation-2

2. **Multi-Tenant Security Warning** (caching.md)
   - Added prominent 🚨 CRITICAL SECURITY warning
   - Explains cross-tenant data leakage risk
   - Shows correct vs incorrect patterns side-by-side
   - Fixes: #violation-12

## HIGH Priority Fixes (11 issues)

3. **Standard Query Parameters** (database-api.md)
   - Added complete GraphQL query signature example
   - Shows where/limit/offset/order_by parameters
   - Demonstrates QueryOptions construction
   - Fixes: #violation-3

4. **Default Ordering Documentation** (database-api.md)
   - Documents default ordering requirement
   - Explains pagination consistency issues
   - Shows best practices per query type
   - Fixes: #violation-6

5. **Complete Context Structure** (caching.md)
   - Added user object to context example
   - Shows integration with authentication
   - Fixes: #violation-13

6. **Type Naming Conventions** (database-api.md)
   - Added table of Input/Filter/Success/Error suffixes
   - Explains generated type naming patterns
   - Fixes: #violation-10

7. **Variable Naming Standardization** (database-api.md, caching.md)
   - Changed 'repo' to 'db' throughout (7 instances)
   - Matches info.context["db"] pattern
   - Fixes: #violation-8, #violation-9

8. **Typed WhereInput Examples** (database-api.md)
   - Added dict-based vs typed filter comparison
   - Shows type safety benefits
   - Recommends typed approach
   - Fixes: #violation-4, #violation-5

## Impact

- **Security**: Prevents copy-paste security vulnerabilities
- **Correctness**: Shows fundamental patterns correctly
- **Completeness**: Documents all standard query features
- **Consistency**: Standardizes variable naming across docs
- **Type Safety**: Promotes typed approaches over dicts

## Validation

All fixes validated with:
- Automated pattern detection script (8/8 checks passed)
- Manual code example extraction
- Git diff review

Related: Phase 3 Code Example Validation
See: dev/audits/phase3/fraiseql-pattern-violations.md
See: dev/audits/phase3/remediation-plan.md

🤖 Generated with Claude Code
EOF
)"
```

---

### Commit 4.3: Update Audit Status

```bash
# Create status update file
cat > dev/audits/phase3/remediation-status.md << 'EOF'
# Phase 3 Remediation Status

**Date**: 2025-11-02
**Status**: ✅ COMPLETE
**Commit**: [git rev-parse HEAD to be inserted]

## Summary

All CRITICAL and HIGH priority violations have been fixed.

| Priority | Issues | Fixed | Remaining |
|----------|--------|-------|-----------|
| CRITICAL | 2      | 2     | 0         |
| HIGH     | 11     | 11    | 0         |
| MEDIUM   | 19     | 0     | 19        |
| LOW      | 15     | 0     | 15        |

## Files Modified

1. `docs/core/database-api.md` - 8 fixes
2. `docs/performance/caching.md` - 3 fixes
3. `docs/advanced/authentication.md` - 1 fix

## Validation Results

✅ All automated checks passed
✅ Manual review completed
✅ Git commit created

## Next Steps

### Before v1.1.1 Release
- ✅ CRITICAL issues fixed
- ✅ HIGH priority issues fixed
- 📋 MEDIUM/LOW issues documented for v1.1.2

### Post-Release (v1.1.2)
- [ ] Fix 19 MEDIUM priority issues (variable naming, type safety examples)
- [ ] Fix 15 LOW priority issues (async context, plural/singular clarity)

### Future Expansion
- [ ] Analyze type definition documentation
- [ ] Analyze mutation pattern documentation
- [ ] Analyze GraphQL client documentation

## Resources

- Violation Report: `dev/audits/phase3/fraiseql-pattern-violations.md`
- Remediation Plan: `dev/audits/phase3/remediation-plan.md`
- Code Validation: `dev/audits/phase3/code_validation_report.md`
EOF

# Add commit hash
COMMIT_HASH=$(git rev-parse HEAD)
sed -i "s/\[git rev-parse HEAD to be inserted\]/$COMMIT_HASH/" dev/audits/phase3/remediation-status.md

# Commit the status update
git add dev/audits/phase3/remediation-status.md
git commit -m "docs: add Phase 3 remediation status"
```

---

### Commit 4.4: Push Changes (if authorized)

```bash
# Review commit history
git log --oneline -3

# Push to remote (only if authorized)
git push origin dev
```

---

## ✅ Success Criteria

### All Phases Complete When:

- [x] **Phase 1**: 2 CRITICAL issues fixed and validated
- [x] **Phase 2**: 11 HIGH priority issues fixed and validated
- [x] **Phase 3**: All validation checks pass
- [x] **Phase 4**: Git commits created with detailed messages

### Quality Checks:

1. **Automated Validation**: `/tmp/validate-fixes.sh` exits with code 0
2. **Manual Review**: All checklist items checked
3. **Code Extraction**: Re-run succeeds with improved error rate
4. **Git History**: Clean commits with descriptive messages
5. **Documentation**: Remediation status documented

---

## 🚨 Troubleshooting

### Problem: Validation script fails

**Solution**:
```bash
# Check which validation failed
/tmp/validate-fixes.sh | grep "❌"

# Return to the corresponding fix section and retry
```

---

### Problem: Git diff shows unexpected changes

**Solution**:
```bash
# Restore from backup
cp docs/core/database-api.md.backup docs/core/database-api.md

# Or reset single file
git checkout -- docs/core/database-api.md

# Start fix over
```

---

### Problem: Sed replacements went wrong

**Solution**:
```bash
# NEVER use sed without reviewing changes first
# Always create backups before automated replacements

# If sed broke something:
git diff docs/core/database-api.md | less
# If bad, revert:
git checkout -- docs/core/database-api.md
# Manual fix instead of sed
```

---

### Problem: Can't find the line numbers

**Solution**:
```bash
# Line numbers may shift as you make changes
# Search by content instead:

# Find context extraction
grep -n "get_my_profile" docs/core/database-api.md

# Find security warning
grep -n "Missing tenant_id" docs/performance/caching.md

# Find WhereInput examples
grep -n "create_graphql_where_input" docs/core/database-api.md
```

---

### Problem: Not sure if a fix is correct

**Solution**:
```bash
# Reference the violation report
less dev/audits/phase3/fraiseql-pattern-violations.md

# Search for the violation number
/Violation #3

# Compare your fix with the "Fix:" section
```

---

## 📚 Reference Materials

### Key Files
- **This Plan**: `dev/audits/phase3/remediation-plan.md`
- **Violation Report**: `dev/audits/phase3/fraiseql-pattern-violations.md`
- **Validation Report**: `dev/audits/phase3/code_validation_report.md`
- **GraphQL Patterns**: `dev/architecture/graphql-mutation-payload-patterns.md`

### Validation Scripts
- **Code Extraction**: `./scripts/validate-docs-code-examples.sh`
- **Fix Validation**: `/tmp/validate-fixes.sh` (created in Phase 3)

### Backup Strategy
```bash
# Create backups before starting
cp docs/core/database-api.md docs/core/database-api.md.backup
cp docs/performance/caching.md docs/performance/caching.md.backup
cp docs/advanced/authentication.md docs/advanced/authentication.md.backup

# Restore if needed
cp *.backup docs/
```

---

## 🎯 Final Checklist

Before marking this task as complete:

- [ ] Phase 1: All 2 CRITICAL fixes applied
- [ ] Phase 2: All 11 HIGH priority fixes applied
- [ ] Phase 3: Validation script passes (exit code 0)
- [ ] Phase 3: Manual review checklist complete
- [ ] Phase 3: Git diff reviewed and looks correct
- [ ] Phase 4: Changes staged with `git add`
- [ ] Phase 4: Detailed commit message created
- [ ] Phase 4: Remediation status document updated
- [ ] All backups can be safely deleted
- [ ] Documentation builds without errors (if applicable)

---

**Total Estimated Time**: 4-6 hours
**Difficulty**: Moderate (requires careful attention to detail)
**Risk**: Low (changes are documentation only, backups recommended)
**Impact**: High (fixes critical patterns that users will learn from)

---

*End of Remediation Plan*
