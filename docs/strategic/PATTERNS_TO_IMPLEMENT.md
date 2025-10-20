# Patterns to Implement from fraiseql_v1_alpha

**Source**: `/home/lionel/code/fraiseql_v1_alpha/` (clean v1 rebuild)
**Target**: `/home/lionel/code/fraiseql/` (v0 evolution with Rust-first simplification)
**Date**: 2025-10-16

---

## Overview

The `fraiseql_v1_alpha` project is a **clean rebuild** implementing advanced patterns. Since we're doing an **evolution** of the current v0 codebase with RUST_FIRST_SIMPLIFICATION, we need to identify which patterns are relevant and can be adapted.

---

## Key Patterns from v1_alpha

### 1. **Trinity Identifiers** (✅ Highly Relevant)

**What it is**: Three-tier ID system for every entity
- `pk_*` (SERIAL) - Internal fast joins (10x faster than UUID)
- `id` (UUID) - Public API (secure, no enumeration)
- `identifier` (TEXT) - Human URLs (usernames, slugs)

**Current v0**: Uses single ID approach (varies by entity)

**Implementation for evolution**:
```sql
-- Add to existing tables via migration
ALTER TABLE users
  ADD COLUMN pk_user SERIAL UNIQUE,
  ADD COLUMN id UUID DEFAULT gen_random_uuid() UNIQUE,
  ADD COLUMN identifier TEXT UNIQUE;

-- Update foreign keys gradually
-- Old: user_id UUID → New: pk_user INT
```

**Benefits**:
- 10x faster joins (SERIAL vs UUID)
- Secure public API (UUID)
- SEO-friendly URLs (identifier)

**Relevant to RUST_FIRST_SIMPLIFICATION**: ✅ Yes - Improves database performance which complements Rust transformation speed

---

### 2. **3-Layer Mutation Architecture** (⚠️ Partially Relevant)

**What it is**: Physical security via PostgreSQL schemas
```
app.fn_*    → Public API (permissions, validation)
  ↓
core.fn_*   → Business logic (private, no direct access)
  ↓
common.log_and_return_mutation → Unified audit logger
```

**Current v0**: Mutations are Python functions that call SQL

**Implementation for evolution**:
- **Simplified version**: Keep mutations in Python but add schema separation for sensitive functions
- **Skip full 3-layer**: Too complex for Rust-first simplification goals (we want LESS layers, not more)

**Relevant to RUST_FIRST_SIMPLIFICATION**: ⚠️ **Conflicts** - RUST_FIRST aims to reduce complexity (83% code reduction), adding 3 layers goes against this

**Alternative**: Use RLS (Row Level Security) for security instead of schema separation

---

### 3. **CQRS with Explicit Sync** (✅ Highly Relevant)

**What it is**:
- `tb_*` tables (normalized writes)
- `tv_*` tables (denormalized JSONB reads)
- `fn_sync_tv_*` functions (explicit, no triggers)

**Current v0**: Already has CQRS patterns, but may use triggers

**Implementation for evolution**:
```sql
-- Explicit sync functions (no triggers)
CREATE FUNCTION fn_sync_tv_users(p_pk_user INT) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_users (pk_user, id, data)
    SELECT pk_user, id, jsonb_build_object(...)
    FROM tb_users WHERE pk_user = p_pk_user
    ON CONFLICT (pk_user) DO UPDATE SET data = EXCLUDED.data;
END;
$$ LANGUAGE plpgsql;

-- Call explicitly after mutations
UPDATE tb_users SET email = 'new@example.com' WHERE pk_user = 123;
PERFORM fn_sync_tv_users(123);
```

**Benefits**:
- No trigger complexity
- Explicit control over sync timing
- Easier debugging
- Works perfectly with Rust transformer

**Relevant to RUST_FIRST_SIMPLIFICATION**: ✅ **YES** - Explicit sync = simpler, more predictable. Rust transformer reads from `tv_*` tables (pre-computed JSONB)

---

### 4. **Rich Return Types** (⚠️ Partially Relevant)

**What it is**: Mutations return main entity + all affected entities

```json
{
  "id": "...",
  "status": "new",
  "object_data": {...},  // Main entity
  "extra_metadata": {    // Affected entities
    "teams": [...],
    "posts": [...]
  }
}
```

**Current v0**: Returns only the created/updated entity

**Implementation for evolution**:
- **Simplified version**: Return affected IDs only (let frontend refetch if needed)
- **Skip full implementation**: Adds complexity to mutation returns

**Relevant to RUST_FIRST_SIMPLIFICATION**: ❌ **NO** - Adds complexity. RUST_FIRST is about simplification. Standard mutation returns are fine.

---

### 5. **Schema Separation** (❌ Not Relevant)

**What it is**: `app.*`, `core.*`, `common.*` schemas for physical security

**Current v0**: Functions in `public` schema

**Relevant to RUST_FIRST_SIMPLIFICATION**: ❌ **NO** - Conflicts with simplification goals. Use RLS instead for security.

---

### 6. **Debezium-Style Audit Logging** (⚠️ Optional)

**What it is**: Before/after state logging in `tb_entity_change_log`

**Current v0**: May have basic logging

**Relevant to RUST_FIRST_SIMPLIFICATION**: ⚠️ **OPTIONAL** - Nice to have but not core to Rust-first simplification

---

### 7. **Rust Acceleration (JSON Parsing)** (✅ Core to RUST_FIRST)

**What v1_alpha does**:
- Rust module for JSON parsing (40x speedup)
- PyO3 bindings
- Batch parsing

**Current v0**: Already has `fraiseql_rs` with transformation

**Implementation for evolution**:
- Enhance existing `fraiseql_rs`
- Make it **required** (no Python fallback)
- Remove all Python transformation code

**Relevant to RUST_FIRST_SIMPLIFICATION**: ✅ **YES** - This IS the core of RUST_FIRST_SIMPLIFICATION

---

## Recommended Implementation Priority

### Phase 1: Core Rust-First Simplification (Weeks 1-4)

**From RUST_FIRST_SIMPLIFICATION.md**:
1. ✅ Make Rust transformer required (no fallback)
2. ✅ Remove IntelligentPassthroughMixin (~1,500 LOC)
3. ✅ Remove JSONPassthrough wrapper (~300 LOC)
4. ✅ Remove Python case conversion (~200 LOC)
5. ✅ Simplify configuration (50+ → 7 options)
6. ✅ Single execution path (remove 5 alternative paths)

**Expected result**: 83% code reduction, same performance

---

### Phase 2: CQRS with Explicit Sync (Weeks 5-6)

**From v1_alpha ADVANCED_PATTERNS.md**:
1. ✅ Add `fn_sync_tv_*` functions for explicit sync
2. ✅ Remove triggers (if any exist)
3. ✅ Document sync pattern

**Benefits**: Simpler, more predictable CQRS

---

### Phase 3: Trinity Identifiers (Weeks 7-10)

**From v1_alpha ADVANCED_PATTERNS.md**:
1. ✅ Add migration to add `pk_*`, `id`, `identifier` to existing tables
2. ✅ Update foreign keys to use `pk_*` (SERIAL) instead of UUID
3. ✅ Update GraphQL types to expose only `id` and `identifier`

**Benefits**: 10x faster joins

---

### Phase 4: Optional Enhancements (Weeks 11+)

**Nice to have but not core**:
- ⚠️ Rich return types (simplified version)
- ⚠️ Audit logging (Debezium style)
- ⚠️ Better RLS patterns

---

## What NOT to Implement (Conflicts with Rust-First Simplification)

### ❌ 3-Layer Mutation Architecture
- **Why**: Adds complexity (3 schemas, 3 function layers)
- **RUST_FIRST goal**: Reduce complexity by 83%
- **Alternative**: Keep mutations simple, use RLS for security

### ❌ Schema Separation (app/core/common)
- **Why**: Physical security via schemas adds deployment complexity
- **RUST_FIRST goal**: Simplification
- **Alternative**: Use RLS + role-based permissions

### ❌ Complex Rich Return Types
- **Why**: Adds complexity to mutation response handling
- **RUST_FIRST goal**: Simple, predictable patterns
- **Alternative**: Return entity + list of affected IDs (frontend refetches if needed)

---

## Summary: Evolution Strategy

**Combine**:
1. ✅ RUST_FIRST_SIMPLIFICATION.md (83% code reduction, single execution path)
2. ✅ Trinity Identifiers (10x faster joins)
3. ✅ Explicit CQRS Sync (no triggers, predictable)

**Skip**:
1. ❌ 3-Layer mutation architecture (too complex)
2. ❌ Schema separation (too complex)
3. ❌ Rich return types (not needed)

**Result**:
- Simple, fast, maintainable codebase
- 83% less code than v0
- 10x faster joins (Trinity)
- Sub-1ms transformations (Rust)
- Predictable CQRS (explicit sync)

---

**Next Steps**:
1. Continue with RUST_FIRST_SIMPLIFICATION.md Phase 1-4
2. After Rust-first is complete, add Trinity Identifiers via migrations
3. After Trinity is complete, add explicit CQRS sync functions

---

**Last Updated**: 2025-10-16
**Status**: Planning - Patterns identified for evolution strategy
