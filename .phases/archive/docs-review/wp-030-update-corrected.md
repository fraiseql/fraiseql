# WP-030 Update: Correct Understanding of FraiseQL Audit Pattern

**Date:** 2025-12-08
**Status:** WP-030 specification needs correction

---

## What I Got Wrong

I initially misunderstood the blog_enterprise trigger example. FraiseQL **already has the right pattern** for audit logging:

### ✅ FraiseQL's Correct Pattern (Explicit + Infrastructure Trigger)

**Application Layer (Explicit):**
```python
# Mutations explicitly call log_and_return_mutation()
RETURN log_and_return_mutation(
    p_tenant_id := input_pk_organization,
    p_user_id := input_created_by,
    p_entity_type := 'post',
    p_entity_id := v_post_id,
    p_operation_type := 'INSERT',
    p_operation_subtype := 'new',
    p_changed_fields := ARRAY['title', 'content'],
    p_message := 'Post created',
    p_old_data := NULL,
    p_new_data := (SELECT data FROM v_post WHERE id = v_post_id),
    p_metadata := jsonb_build_object(...)
);
```

**Infrastructure Layer (Internal Trigger - Acceptable):**
```sql
-- ONLY for cryptographic chain integrity (security-critical)
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Why This Works:**
1. ✅ **Audit logging is explicit** - `log_and_return_mutation()` is called explicitly in mutation functions
2. ✅ **AI can see the audit** - The function call is visible in code
3. ✅ **CDC data is explicit** - `changed_fields`, `old_data`, `new_data` are explicit parameters
4. ✅ **Crypto is infrastructure** - The trigger only populates hash/signature (tamper-proof requirement)
5. ✅ **Testable** - Can test audit logging by checking `audit_events` table
6. ✅ **Traceable** - Code path from mutation → log_and_return_mutation → audit_events is clear

---

## Legitimate Exception: Cryptographic Chain Trigger

**When Triggers Are Acceptable:**

### ✅ Infrastructure-Level Security (Cryptographic Chain)

```sql
-- Purpose: Maintain tamper-proof cryptographic chain
-- Scope: ONLY on audit_events table
-- Fields: event_hash, signature, previous_hash
-- Rationale: Must be tamper-proof, can't be set by application

CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Why This Exception Is OK:**
- **Tamper-proof requirement** - Application code shouldn't set crypto fields
- **Infrastructure concern** - Not business logic
- **Limited scope** - Only on audit table, only crypto fields
- **Well-documented** - Clear purpose and rationale
- **Security-critical** - Breaking this would compromise audit integrity

---

## WP-030 Revised Scope

### What Needs Auditing

**❌ Still Avoid (Business Logic Triggers):**

1. **Audit Triggers on Business Tables**
   ```sql
   -- ❌ DON'T DO THIS
   CREATE TRIGGER audit_post_changes
       AFTER INSERT OR UPDATE OR DELETE ON tb_post
       FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
   ```
   **Why bad:** Business tables should explicitly call `log_and_return_mutation()`, not use triggers

2. **Timestamp Triggers**
   ```sql
   -- ❌ DON'T DO THIS
   CREATE TRIGGER update_timestamp
       BEFORE UPDATE ON tb_post
       FOR EACH ROW EXECUTE FUNCTION update_updated_at();
   ```
   **Use instead:** `updated_at TIMESTAMPTZ DEFAULT NOW()` + explicit updates in code

3. **Cascade/Cleanup Triggers**
   ```sql
   -- ❌ DON'T DO THIS
   CREATE TRIGGER delete_orphan_comments
       AFTER DELETE ON tb_post
       FOR EACH ROW EXECUTE FUNCTION cleanup_orphan_comments();
   ```
   **Use instead:** `ON DELETE CASCADE` or explicit app logic

4. **Validation Triggers**
   ```sql
   -- ❌ DON'T DO THIS
   CREATE TRIGGER validate_post_status
       BEFORE INSERT OR UPDATE ON tb_post
       FOR EACH ROW EXECUTE FUNCTION validate_post_status_transition();
   ```
   **Use instead:** `CHECK` constraints or Pydantic validation

### ✅ Acceptable Exceptions

1. **Cryptographic Chain Infrastructure** (audit_events table only)
2. **Security-Critical Tamper-Proofing** (documented, limited scope)
3. **Legacy Database Integration** (when migrating, document thoroughly)

---

## Updated WP-030 Tasks

### 1. Audit for BAD Trigger Usage (2 hours)

**Search for business logic triggers:**
```bash
# Find triggers on business tables (NOT audit infrastructure)
grep -rn "CREATE TRIGGER" examples/ docs/ \
  --include="*.sql" --include="*.md" \
  | grep -v "populate_crypto" \
  | grep -v "audit_events"
```

**Check for:**
- Audit triggers on business tables (`tb_post`, `tb_user`, etc.)
- Timestamp update triggers
- Cascade/cleanup triggers
- Validation triggers

### 2. Document the Correct Pattern (2 hours)

**Update `docs/database/avoid-triggers.md`:**

```markdown
# FraiseQL's Explicit Audit Pattern

## ✅ The Right Way: Explicit + Infrastructure

### Application Layer (Explicit)
Mutations explicitly call `log_and_return_mutation()`:

[Code example showing explicit logging]

### Infrastructure Layer (Internal)
Cryptographic chain is maintained by infrastructure trigger:

[Code example showing crypto trigger]

### Why This Works
1. Audit logging is explicit and visible
2. CDC data (changed_fields, old/new data) is explicit
3. Crypto integrity is infrastructure-level (tamper-proof)
4. AI can understand the code path
5. Testable and traceable

## ❌ What NOT to Do

[Examples of bad trigger usage on business tables]
```

### 3. Fix Blog Enterprise Example (1 hour)

The `blog_enterprise/README.md` trigger example (line 464-466) should be updated to show the **correct FraiseQL pattern**:

**Before (Misleading):**
```sql
-- Audit logging trigger
CREATE TRIGGER audit_changes
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
```

**After (FraiseQL Pattern):**
```sql
-- FraiseQL's explicit audit pattern
CREATE FUNCTION create_post_with_audit(...)
RETURNS TABLE(...) AS $$
BEGIN
    -- Business logic
    INSERT INTO tb_post (...) RETURNING id INTO v_post_id;

    -- Explicit audit logging (AI-visible!)
    RETURN QUERY SELECT * FROM log_and_return_mutation(
        p_tenant_id := input_tenant_id,
        p_entity_type := 'post',
        p_entity_id := v_post_id,
        p_operation_type := 'INSERT',
        p_changed_fields := ARRAY['title', 'content'],
        p_old_data := NULL,
        p_new_data := (SELECT data FROM v_post WHERE id = v_post_id),
        ...
    );
END;
$$ LANGUAGE plpgsql;
```

### 4. Create Linting with Exception (2 hours)

**Updated linting script:**
```python
# scripts/lint_no_triggers.py

ALLOWED_TRIGGER_PATTERNS = [
    r'populate_crypto_trigger',  # Cryptographic chain infrastructure
    r'ON\s+audit_events',        # Triggers on audit_events table only
]

ALLOWED_TRIGGER_FILES = [
    'src/fraiseql/enterprise/migrations/002_unified_audit.sql',  # Infrastructure
]

def is_allowed_trigger(trigger_line: str, file_path: str) -> bool:
    """Check if trigger is an allowed exception."""
    if str(file_path) in ALLOWED_TRIGGER_FILES:
        return True

    for pattern in ALLOWED_TRIGGER_PATTERNS:
        if re.search(pattern, trigger_line, re.IGNORECASE):
            return True

    return False
```

### 5. Update Documentation Examples (3 hours)

**Files to update:**
- `examples/blog_enterprise/README.md` - Fix trigger example (show correct pattern)
- `docs/database/avoid-triggers.md` - New guide with exceptions documented
- `docs/advanced/database-patterns.md` - Ensure audit pattern is correct
- `examples/*/` - Check for any bad trigger examples

---

## Key Takeaway

**FraiseQL already has the right pattern!**

The two-layer approach is actually elegant:
1. **Explicit layer** - Business logic calls `log_and_return_mutation()` (AI-visible)
2. **Infrastructure layer** - Crypto chain maintained by trigger (tamper-proof)

This combines:
- ✅ Explicitness (for AI and developers)
- ✅ Security (tamper-proof crypto chain)
- ✅ Traceability (visible code paths)
- ✅ Testability (can test audit logging)

WP-030 should focus on:
- **Auditing for BAD trigger usage** (business logic triggers)
- **Documenting the CORRECT pattern** (explicit + infrastructure)
- **Providing migration guidance** (from bad triggers to FraiseQL pattern)

---

**End of Correction**
