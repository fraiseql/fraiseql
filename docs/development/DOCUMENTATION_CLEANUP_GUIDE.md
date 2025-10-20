# Documentation & Examples Cleanup Guide

**Objective**: Update all documentation and examples to reflect the simplified unified audit system and remove outdated patterns.

**Context**: We've completed Phase 3 and Phase 4 of the Tier 1 Enterprise Features with significant simplifications:
- ✅ Phase 3: Event logging using PostgreSQL crypto triggers (not Python)
- ✅ Phase 4: Unified audit table (not separate tb_audit_log + audit_events)
- ✅ Philosophy: "In PostgreSQL Everything" - all crypto/audit logic in PostgreSQL

---

## 🎯 Goals

1. **Remove Complexity**: Delete references to old dual-table system and Python crypto
2. **Update Examples**: Ensure all examples use unified `audit_events` table
3. **Clarify Philosophy**: Emphasize PostgreSQL-first approach throughout
4. **Fix Inconsistencies**: Ensure all docs align with current implementation
5. **Clean Architecture Docs**: Remove outdated ADRs and planning docs

---

## 📋 Cleanup Checklist

### 1. Enterprise Documentation

#### Files to Update:
- [ ] `docs/enterprise/audit-logging.md` (if exists)
- [ ] Any enterprise feature documentation

#### Changes Needed:
- [ ] Remove references to `tenant.tb_audit_log` as separate table
- [ ] Remove references to `bridge_audit_to_chain()` trigger
- [ ] Update to unified `audit_events` table
- [ ] Show single-table schema with CDC + crypto features
- [ ] Update all SQL examples to use `log_and_return_mutation()` with unified table
- [ ] Add section on "Why One Table Instead of Two"
- [ ] Update performance claims (no duplicate writes, no bridge overhead)

#### Example Section to Add:
```markdown
## Unified Audit Table Architecture

FraiseQL uses a **single unified audit table** that combines:
- ✅ CDC (Change Data Capture) - old_data, new_data, changed_fields
- ✅ Cryptographic chain integrity - event_hash, signature, previous_hash
- ✅ Business metadata - operation types, business_actions
- ✅ Multi-tenant isolation - per-tenant cryptographic chains

### Why One Table?

**Simplicity**: One schema to understand, one table to query
**Performance**: No duplicate writes, no bridge synchronization
**Integrity**: Single source of truth, atomic operations
**Philosophy**: "In PostgreSQL Everything" - all logic in one place
```

---

### 2. Examples Cleanup

#### Files to Review:
- [ ] `examples/blog_api/db/functions/core_functions.sql`
- [ ] `examples/blog_api/db/functions/app_functions.sql`
- [ ] `examples/enterprise_patterns/cqrs/schema.sql`
- [ ] `examples/enterprise_patterns/db/migrations/001_schema.sql`
- [ ] All example READMEs

#### Changes Needed:
- [ ] Replace `tenant.tb_audit_log` with `audit_events`
- [ ] Update `log_and_return_mutation()` function signature to match unified version
- [ ] Remove any bridge trigger references
- [ ] Add examples showing crypto chain queries
- [ ] Update READMEs to explain unified audit approach

#### Template for Updated `log_and_return_mutation()`:
```sql
-- Unified audit logging function
-- Logs to audit_events table with automatic crypto chain integrity
CREATE OR REPLACE FUNCTION log_and_return_mutation(
    p_tenant_id UUID,
    p_user_id UUID,
    p_entity_type TEXT,
    p_entity_id UUID,
    p_operation_type TEXT,        -- INSERT, UPDATE, DELETE, NOOP
    p_operation_subtype TEXT,     -- new, updated, noop:duplicate, etc.
    p_changed_fields TEXT[],
    p_message TEXT,
    p_old_data JSONB,             -- CDC: before state
    p_new_data JSONB,             -- CDC: after state
    p_metadata JSONB              -- Business actions, rules, etc.
) RETURNS TABLE (
    success BOOLEAN,
    operation_type TEXT,
    entity_type TEXT,
    entity_id UUID,
    message TEXT,
    error_code TEXT,
    changed_fields TEXT[],
    old_data JSONB,
    new_data JSONB,
    metadata JSONB
) AS $$
BEGIN
    -- Insert into unified audit_events table
    -- Crypto fields auto-populated by populate_crypto_trigger
    INSERT INTO audit_events (
        tenant_id, user_id, entity_type, entity_id,
        operation_type, operation_subtype, changed_fields,
        old_data, new_data, metadata
    ) VALUES (
        p_tenant_id, p_user_id, p_entity_type, p_entity_id,
        p_operation_type, p_operation_subtype, p_changed_fields,
        p_old_data, p_new_data, p_metadata
    );

    -- Return standardized mutation result
    RETURN QUERY SELECT
        (p_operation_type IN ('INSERT', 'UPDATE', 'DELETE'))::BOOLEAN,
        p_operation_type, p_entity_type, p_entity_id, p_message,
        CASE WHEN p_operation_type = 'NOOP' THEN p_operation_subtype ELSE NULL END,
        p_changed_fields, p_old_data, p_new_data, p_metadata;
END;
$$ LANGUAGE plpgsql;
```

---

### 3. Architecture Decision Records (ADRs)

#### Files to Review:
- [ ] `docs/architecture/decisions/*.md`
- [ ] Any ADRs mentioning audit logging or mutation tracking

#### Changes Needed:
- [ ] Add new ADR: "003_unified_audit_table.md" explaining the simplification
- [ ] Update existing ADRs if they reference old dual-table approach
- [ ] Mark outdated ADRs as superseded

#### Template for New ADR:
```markdown
# ADR 003: Unified Audit Table with CDC + Cryptographic Chain

## Status
Accepted

## Context
We needed enterprise-grade audit logging with:
- Change Data Capture (CDC) for compliance
- Cryptographic chain integrity for tamper-evidence
- Multi-tenant isolation
- PostgreSQL-native implementation (no external dependencies)

Initially considered separate tables:
- `tenant.tb_audit_log` for CDC data
- `audit_events` for cryptographic chain

## Decision
Use **one unified `audit_events` table** that combines both CDC and cryptographic features.

## Rationale
1. **Simplicity**: One table to understand, query, and maintain
2. **Performance**: No duplicate writes, no bridge synchronization
3. **Integrity**: Single source of truth, atomic operations
4. **Philosophy**: Aligns with "In PostgreSQL Everything"
5. **Developer Experience**: Easier to work with, fewer moving parts

## Consequences
### Positive
- Reduced complexity (1 table instead of 2)
- Better performance (no duplicate writes)
- Easier to query (single table)
- Simpler schema migrations

### Negative
- None identified

## Implementation
See: `src/fraiseql/enterprise/migrations/002_unified_audit.sql`
```

---

### 4. Planning Documents to Archive

#### Files to Archive (Move to `archive/` directory):
- [ ] `CQRS_RUST_ARCHITECTURE.md` - Outdated architecture planning
- [ ] `DATAFLOW_SUMMARY.md` - Outdated data flow documentation
- [ ] `JSONB_TO_HTTP_SIMPLIFICATION_PLAN.md` - Implementation complete
- [ ] `PASSTHROUGH_FIX_ANALYSIS.md` - Implementation complete
- [ ] `PERFORMANCE_OPTIMIZATION_PLAN.md` - Superseded by actual implementation
- [ ] `QUERY_EXECUTION_PATH_ANALYSIS.md` - Outdated analysis
- [ ] `RUST_FIRST_IMPLEMENTATION_PROGRESS.md` - Historical record
- [ ] `UNIFIED_RUST_ARCHITECTURE_PLAN.md` - Implementation complete
- [ ] `V1_DOCUMENTATION_PLAN.md` - Superseded
- [ ] Any other `*_PLAN.md` or `*_SUMMARY.md` files that are outdated

#### Action:
```bash
# Create archive directory if it doesn't exist
mkdir -p archive/planning

# Move outdated planning docs
mv CQRS_RUST_ARCHITECTURE.md archive/planning/
mv DATAFLOW_SUMMARY.md archive/planning/
# ... etc
```

---

### 5. Core Documentation Updates

#### Files to Update:
- [ ] `README.md` - Main project README
- [ ] `docs/README.md` - Documentation index
- [ ] `docs/core/fraiseql-philosophy.md` - Already updated, verify consistency
- [ ] `docs/quickstart.md` - May need audit examples
- [ ] `ENTERPRISE.md` - Enterprise features overview

#### Changes Needed:

**README.md:**
- [ ] Update feature list to mention "Unified Audit Logging with Cryptographic Chain"
- [ ] Add performance claim: "PostgreSQL-native crypto (no Python overhead)"
- [ ] Update architecture diagram if it shows audit system

**docs/README.md:**
- [ ] Add link to enterprise audit documentation
- [ ] Update table of contents

**ENTERPRISE.md:**
- [ ] Add section on unified audit table
- [ ] Show example of querying audit trail
- [ ] Explain cryptographic chain verification
- [ ] Add compliance features (SOX, HIPAA, etc.)

---

### 6. Test Documentation

#### Files to Update:
- [ ] Any test documentation or test README files
- [ ] Integration test documentation

#### Changes Needed:
- [ ] Document that `test_unified_audit.py` is the canonical test suite
- [ ] Mark old test files as deprecated or remove them
- [ ] Update test patterns to show unified table approach

---

### 7. Migration Guides

#### Create New Guide:
- [ ] `docs/migration-guides/unified-audit-migration.md`

#### Content:
```markdown
# Migrating to Unified Audit Table

## Overview
If you're using the old dual-table audit system, migrate to the unified approach.

## Old System (Before)
```sql
-- Separate tables
tenant.tb_audit_log      -- CDC data
audit_events             -- Crypto chain
bridge_audit_to_chain()  -- Bridge trigger
```

## New System (After)
```sql
-- Single unified table
audit_events  -- CDC + Crypto in one table
```

## Migration Steps

### 1. Backup Existing Data
```sql
-- Export old audit logs
COPY tenant.tb_audit_log TO '/tmp/old_audit_log.csv' CSV HEADER;
COPY audit_events TO '/tmp/old_audit_events.csv' CSV HEADER;
```

### 2. Apply New Migration
```sql
\i src/fraiseql/enterprise/migrations/002_unified_audit.sql
```

### 3. Migrate Data (if needed)
```sql
-- Insert old tb_audit_log data into unified audit_events
INSERT INTO audit_events (
    tenant_id, user_id, entity_type, entity_id,
    operation_type, operation_subtype, changed_fields,
    old_data, new_data, metadata, timestamp
)
SELECT
    pk_organization, user_id, entity_type, entity_id,
    operation_type, operation_subtype, changed_fields,
    old_data, new_data, metadata, created_at
FROM tenant.tb_audit_log
ORDER BY created_at ASC;
-- Note: Crypto fields will be auto-populated by trigger
```

### 4. Update Function Calls
```sql
-- Change all log_and_return_mutation() calls to use new signature
-- See examples in examples/blog_api/db/functions/core_functions.sql
```

### 5. Drop Old Tables (after verification)
```sql
DROP TABLE IF EXISTS tenant.tb_audit_log CASCADE;
-- Keep only unified audit_events
```

## Breaking Changes
- Function signature slightly different (returns TABLE instead of composite type)
- Crypto fields now auto-populated (don't pass them manually)
- Single table queries instead of JOINs

## Benefits
- ✅ Simpler schema
- ✅ Better performance
- ✅ Single source of truth
- ✅ Easier to query
```

---

### 8. TIER_1_IMPLEMENTATION_PLANS.md

#### Update Status:
- [ ] Mark Phase 3 as ✅ Complete (with note: "PostgreSQL-native crypto, not Python")
- [ ] Mark Phase 4 as ✅ Complete (with note: "Unified table approach, no separate interceptors")
- [ ] Update Phase 5 to reflect unified table
- [ ] Add "Simplified Architecture" note at the top

#### Add Section:
```markdown
## ⚡ Simplification Notes

### Original Plan vs. Implementation

**Original Plan (Complex):**
- Separate `audit_events` table for crypto
- Separate `tenant.tb_audit_log` for CDC
- Python crypto modules for hashing/signing
- GraphQL interceptors in Python
- Bridge triggers to sync tables

**Actual Implementation (Simplified):**
- ✅ **Single unified `audit_events` table** (CDC + crypto together)
- ✅ **PostgreSQL handles all crypto** (triggers, not Python)
- ✅ **No GraphQL interceptors needed** (use existing `log_and_return_mutation()`)
- ✅ **No bridge triggers needed** (one table = no sync)
- ✅ **Philosophy aligned**: "In PostgreSQL Everything"

### Why Simplified?

1. **Performance**: No duplicate writes, no Python overhead
2. **Simplicity**: One table, one schema, one source of truth
3. **Maintainability**: Less code, fewer moving parts
4. **Philosophy**: PostgreSQL-native is faster and simpler
```

---

## 🔍 Search & Replace Patterns

### Global Search Terms to Update:

1. **"tenant.tb_audit_log"** → Review each instance
   - If referring to old system: Update to `audit_events`
   - If in migration guide: Keep but mark as "old system"

2. **"bridge_audit_to_chain"** → Remove or mark as deprecated
   - This trigger is no longer needed with unified table

3. **"Python crypto" / "event_logger.py"** → Clarify usage
   - Mark as "verification only, not for event creation"
   - Emphasize PostgreSQL handles creation

4. **"two tables" / "dual table"** → Update to "unified table"

5. **"GraphQL interceptors"** → Update to "log_and_return_mutation()"
   - No need for Python interceptors with PostgreSQL approach

---

## 📝 Documentation Standards

### Ensure All Docs Follow:

1. **Code Blocks**: Always specify language
   ```sql
   -- Good
   ```

   ```
   -- Bad (no language specified)
   ```

2. **Philosophy Callouts**: Use consistent formatting
   ```markdown
   **FraiseQL Philosophy: "In PostgreSQL Everything"**
   - All crypto logic in PostgreSQL triggers
   - No Python overhead for event creation
   - Single source of truth in database
   ```

3. **Examples**: Always include:
   - ✅ What it does
   - ✅ Why this approach
   - ✅ Expected output
   - ✅ Link to full example

4. **Diagrams**: Update any architecture diagrams to show:
   - Single `audit_events` table (not two)
   - PostgreSQL trigger flow
   - No Python crypto in creation path

---

## ✅ Verification Steps

After cleanup, verify:

1. **All Examples Run**
   ```bash
   # Test all example migrations
   psql -f examples/blog_api/db/schema.sql
   psql -f examples/enterprise_patterns/db/migrations/*.sql
   ```

2. **All Tests Pass**
   ```bash
   uv run pytest tests/integration/enterprise/audit/ -v
   ```

3. **Documentation Consistency**
   ```bash
   # Search for outdated patterns
   grep -r "tenant.tb_audit_log" docs/
   grep -r "bridge_audit_to_chain" docs/
   grep -r "two tables" docs/
   ```

4. **No Broken Links**
   ```bash
   # Check all markdown links work
   # (use a markdown link checker tool)
   ```

5. **Philosophy Alignment**
   - [ ] All examples show PostgreSQL-first approach
   - [ ] No examples show Python doing crypto for event creation
   - [ ] Clear distinction between "creation" (PostgreSQL) and "verification" (Python)

---

## 🎯 Priority Order

**High Priority (Do First):**
1. ✅ Archive outdated planning docs
2. ✅ Update TIER_1_IMPLEMENTATION_PLANS.md
3. ✅ Update examples/blog_api functions
4. ✅ Create migration guide
5. ✅ Update ENTERPRISE.md

**Medium Priority:**
6. Update core documentation (README, docs/README)
7. Update philosophy docs (already mostly done)
8. Create new ADR for unified table
9. Update example READMEs

**Low Priority:**
10. Update test documentation
11. Clean up any remaining references
12. Polish diagrams and visuals

---

## 📦 Deliverables

After completion, you should have:

- ✅ Clean, consistent documentation across all files
- ✅ All examples using unified `audit_events` table
- ✅ Outdated docs archived to `archive/` directory
- ✅ New migration guide for users upgrading
- ✅ Updated ADR documenting the simplification
- ✅ All tests passing with unified approach
- ✅ No references to deprecated dual-table system
- ✅ Clear philosophy messaging throughout

---

## 🚀 Getting Started

To begin cleanup:

1. **Create archive directory**:
   ```bash
   mkdir -p archive/planning
   ```

2. **Start with TIER_1_IMPLEMENTATION_PLANS.md**:
   - Mark phases as complete
   - Add simplification notes
   - Update remaining phases

3. **Update examples next**:
   - Start with blog_api (most visible)
   - Then enterprise_patterns
   - Update all READMEs

4. **Archive old planning docs**:
   - Move to archive/planning/
   - Add archive/README.md explaining what's there

5. **Update core docs last**:
   - README.md
   - ENTERPRISE.md
   - docs/

---

## ❓ Questions to Consider

While cleaning up, ask:

1. **Is this pattern still valid?** If not, update or remove
2. **Does this align with "In PostgreSQL Everything"?** If not, rewrite
3. **Is this the simplest way to explain it?** If not, simplify
4. **Would a new user understand this?** If not, add context
5. **Does this match the actual implementation?** If not, fix it

---

**Last Updated**: 2025-10-18 (after completing Phase 3 & 4 simplifications)
