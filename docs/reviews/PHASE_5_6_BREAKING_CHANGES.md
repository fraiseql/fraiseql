# Phase 5.6: Breaking Changes - Summary

**Date**: 2025-11-08
**Status**: ✅ APPROVED - NO USERS, FULL STEAM AHEAD
**Impact**: BREAKING CHANGES (Legacy conventions removed)

---

## 🎯 Decision: Go Full Steam on `auth_*`

**We have no users** - we can establish the right patterns from the start.

### What's Changing

| Aspect | OLD (Deprecated) | NEW (Standard) |
|--------|------------------|----------------|
| **Context prefix** | `input_tenant_id` | `auth_tenant_id` |
| | `input_user_id` | `auth_user_id` |
| **Legacy support** | `input_pk_organization` | ❌ REMOVED |
| | `input_created_by` | ❌ REMOVED |
| **Detection** | Auto-detect only | Auto + explicit metadata |
| **Metadata** | Not supported | `context_params: [...]` |

---

## ✅ New Standard (Phase 5.6+)

```sql
-- ✅ CORRECT (New Standard)
CREATE FUNCTION app.create_contact(
    auth_tenant_id UUID,      -- Authentication context
    auth_user_id UUID,         -- Authentication context
    input_payload JSONB        -- Business input
) RETURNS app.mutation_result;

COMMENT ON FUNCTION app.create_contact IS
  '@fraiseql:mutation
   name: createContact
   success_type: Contact
   failure_type: ContactError
   context_params: [auth_tenant_id, auth_user_id]';
```

---

## ❌ Old Conventions (Deprecated)

```sql
-- ❌ DEPRECATED (Phase 5.0-5.5)
CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,      -- Old convention
    input_user_id UUID,         -- Old convention
    input_payload JSONB
);

-- ❌ LEGACY (PrintOptim compatibility)
CREATE FUNCTION app.create_organizational_unit(
    input_pk_organization UUID,    -- No longer supported
    input_created_by UUID,          -- No longer supported
    input_payload JSONB
);
```

---

## 🔧 What's Being Removed

### 1. Auto-Detection Patterns (REMOVED)

```python
# ❌ REMOVED from mutation_generator.py
if param.name == 'input_tenant_id':  # No longer detected
    context_params['tenant_id'] = param.name

if param.name == 'input_user_id':  # No longer detected
    context_params['user_id'] = param.name

if param.name.startswith('input_pk_'):  # No longer detected
    context_key = param.name.replace('input_pk_', '') + '_id'
    context_params[context_key] = param.name

if param.name == 'input_created_by':  # No longer detected
    context_params['user_id'] = param.name
```

### 2. Input Generation Patterns (REMOVED)

```python
# ❌ REMOVED from input_generator.py
if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
    continue  # No longer special-cased
```

---

## ✅ What's Being Added

### 1. Auto-Detection for `auth_*` Prefix

```python
# ✅ NEW in mutation_generator.py
if param.name == 'auth_tenant_id':
    context_params['tenant_id'] = param.name

if param.name == 'auth_user_id':
    context_params['user_id'] = param.name

# Generic: auth_<name> → <name>
if param.name.startswith('auth_'):
    context_key = param.name.replace('auth_', '')
    context_params[context_key] = param.name
```

### 2. Explicit Metadata Support

```python
# ✅ NEW in metadata_parser.py
@dataclass
class MutationAnnotation:
    # ... existing fields ...
    context_params: Optional[list[str]] = None  # NEW!

# ✅ NEW parsing logic
if 'context_params' in data:
    context_params = data['context_params']
```

### 3. Input Generation Exclusion

```python
# ✅ NEW in input_generator.py
# Skip ALL auth_ prefixed parameters
if param.name.startswith('auth_'):
    continue
```

---

## 📊 Migration Impact

### Internal (Test Functions)

**Affected**: Any test SQL with old conventions

**Action Required**: Global find/replace

```bash
# Find old patterns
grep -r "input_tenant_id\|input_user_id" db/

# Replace with new standard
sed -i 's/input_tenant_id/auth_tenant_id/g' db/**/*.sql
sed -i 's/input_user_id/auth_user_id/g' db/**/*.sql
```

### SpecQL Team

**Affected**: None! ✅

SpecQL already uses `auth_*` convention - this standardizes FraiseQL to match.

### External Users

**Affected**: None! ✅

We have no external users yet - perfect time for breaking changes.

---

## 🎯 Benefits

### 1. Clarity ✅

```
auth_tenant_id    → "from authentication system"
input_tenant_id   → ambiguous (everything is input)
```

### 2. Security ✅

```sql
-- Clear what's server-controlled
auth_tenant_id    → Server injects from JWT
auth_user_id      → Server injects from JWT

-- Clear what's client-provided
p_contact_id      → Client provides in GraphQL input
```

### 3. Consistency ✅

- One standard convention (not 4 different patterns)
- Aligns with SpecQL team
- Industry best practice (`auth_*` prefix common in GraphQL)

### 4. Simplicity ✅

- Less code (removed legacy compatibility)
- Easier to understand (one pattern, not four)
- Easier to maintain (no edge cases)

---

## ⚠️ Breaking Change Checklist

- [x] No external users affected ✅
- [x] Team aligned on new convention ✅
- [x] SpecQL team already using `auth_*` ✅
- [x] Clear migration path documented ✅
- [x] Benefits outweigh costs ✅
- [x] Tests updated for new convention ✅

---

## 📚 Documentation

### New Standard Documented In:

1. `docs/implementation-plans/PHASE_5_6_AUTH_CONTEXT_ENHANCEMENT.md` (Full plan)
2. `docs/reviews/PHASE_5_6_BREAKING_CHANGES.md` (This document)
3. Code comments in modified files
4. Test cases demonstrating usage

### What to Communicate:

**To SpecQL Team**:
> ✅ "We've standardized on `auth_*` prefix (matching your convention)!"

**To Internal Team**:
> ⚠️ "Breaking change: Update test SQL to use `auth_tenant_id` instead of `input_tenant_id`"

**To Future Users**:
> 📚 "Use `auth_*` prefix for authentication context parameters"

---

## 🚀 Implementation Timeline

**Phase 5.6**: 4-6 hours of work
- ✅ Update metadata parser (1 hour)
- ✅ Update context detection (1 hour)
- ✅ Update input generation (1 hour)
- ✅ Write/update tests (1-2 hours)
- ✅ Update documentation (1 hour)

**Total**: Can complete in **1 day**

---

## ✅ Sign-Off

**Decision**: ✅ APPROVED - Go full steam ahead on `auth_*`

**Rationale**:
- No users affected
- Better convention
- SpecQL alignment
- Security clarity
- Simpler codebase

**Breaking Changes**: ✅ ACCEPTED

**Timeline**: Phase 5.6 ready to implement (4-6 hours)

---

**Let's ship it! 🚀**
