# FraiseQL Examples Versioning Audit

## Executive Summary

**Critical Issue**: Examples directory has **inconsistent mutation type usage** and **undocumented versioning**.

- **OLD type**: `mutation_result` (6 fields) - deprecated
- **NEW type**: `mutation_response` (8 fields) - current standard

## Detailed Findings

### 1. mutations_demo - MAJOR CODE SMELL ❌

**Location**: `/examples/mutations_demo/`

**Problem**: Parallel versions without documentation

| File | Type | Status |
|------|------|--------|
| `init.sql` | `mutation_result` (old) | Should be removed |
| `v2_init.sql` | `mutation_response` (new) | Should be main |
| `v2_mutation_functions.sql` | `mutation_response` (new) | Should be main |
| `README.md` | No mention of versioning | Needs update |
| `demo.py` | Doesn't reference either | Unclear which to use |

**Impact**: New users don't know which version to use.

**Recommendation**:
- Remove `init.sql` (or move to `/examples/_legacy/`)
- Rename `v2_init.sql` → `setup.sql`
- Rename `v2_mutation_functions.sql` → `mutation_functions.sql`
- Update README to document the migration

---

### 2. Examples Using OLD Type (mutation_result) ❌

Need migration to `mutation_response`:

1. **context_parameters** (`schema.sql`)
   - Uses old 6-field type
   - Should migrate to 8-field type

2. **blog_api** (`db/functions/*.sql`)
   - Uses old type in app_functions.sql and core_functions.sql
   - Should migrate to new standard

---

### 3. Examples Using NEW Type (mutation_response) ✅

Already updated (correct):

1. **mutation-patterns** (all subdirectories)
   - ✅ 01-basic-crud
   - ✅ 02-validation
   - ✅ 03-business-logic
   - ✅ 04-relationships
   - ✅ 05-error-handling
   - ✅ 06-advanced

2. **ecommerce_api**
   - ✅ All mutation functions use new type

---

### 4. Examples Without Mutations (No Issue) ✓

These examples don't use mutations, so no action needed:
- blog_simple (query-only example)
- blog_enterprise (uses different pattern)
- complete_cqrs_blog (has own migration system)
- etc.

---

## Migration Path

### Immediate Actions (High Priority)

1. **mutations_demo cleanup**:
   ```bash
   cd examples/mutations_demo
   mv init.sql _old_init.sql.deprecated
   mv v2_init.sql setup.sql
   mv v2_mutation_functions.sql mutation_functions.sql
   # Update README.md to remove v2 references
   ```

2. **blog_api migration**:
   - Update `db/functions/app_functions.sql` to use `mutation_response`
   - Update `db/functions/core_functions.sql` to use `mutation_response`
   - Test that example still works

3. **context_parameters migration**:
   - Update `schema.sql` to use `mutation_response`
   - Test that example still works

### Documentation Updates

1. Add migration guide: `/docs/migrations/mutation_result_to_mutation_response.md`
2. Update all example READMEs to specify mutation type used
3. Add deprecation notice in CHANGELOG

---

## Type Comparison

### OLD: mutation_result (6 fields)
```sql
CREATE TYPE mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);
```

### NEW: mutation_response (8 fields)
```sql
CREATE TYPE mutation_response AS (
    status TEXT,           -- NEW: First field
    message TEXT,
    entity_id TEXT,        -- NEW: Renamed from 'id', TEXT not UUID
    entity_type TEXT,      -- NEW: Type name for entity
    entity JSONB,          -- NEW: Renamed from 'object_data'
    updated_fields TEXT[],
    cascade JSONB,         -- NEW: Cascade data
    metadata JSONB         -- Renamed from 'extra_metadata'
);
```

**Key differences**:
- Field order changed (status first)
- entity_id is TEXT (was UUID)
- Added entity_type field
- Added cascade field for side effects
- Renamed fields for clarity

---

## Estimated Effort

| Task | Effort | Priority |
|------|--------|----------|
| mutations_demo cleanup | 30 min | HIGH |
| blog_api migration | 1 hour | HIGH |
| context_parameters migration | 30 min | MEDIUM |
| Documentation updates | 1 hour | HIGH |
| Testing all changes | 1 hour | HIGH |

**Total**: ~4 hours

---

## Risk Assessment

**If not fixed**:
- New users will be confused about which version to use
- Some examples teach deprecated patterns
- Inconsistent codebase makes maintenance harder
- Migration guides are unclear

**Mitigation**:
- Fix high-priority items immediately
- Add clear deprecation warnings
- Update documentation comprehensively

---

**Report Date**: 2025-12-12
**Audit By**: Claude Code (FraiseQL Architecture Analysis)
