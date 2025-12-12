# FraiseQL Trigger Audit Findings (WP-030)

**Date:** 2025-12-08
**Auditor:** Claude Code (Documentation Review)
**Scope:** All examples, documentation, and source code

---

## Executive Summary

**Total Triggers Found:** 47+ trigger definitions
**Infrastructure Triggers (GOOD):** 2 (audit_events only)
**Business Logic Triggers (BAD):** 45+ (timestamp, notification, slug generation)

**Recommendation:** Replace business logic triggers with explicit patterns, keep infrastructure triggers.

---

## Classification

### ✅ GOOD: Infrastructure Triggers (Acceptable)

**Location:** `src/fraiseql/enterprise/migrations/`

1. **`populate_crypto_trigger`** (audit_events table)
   - **File:** `002_unified_audit.sql:227`, `001_audit_tables.sql:168`
   - **Purpose:** Cryptographic chain integrity (tamper-proof audit trail)
   - **Scope:** ONLY audit_events table, ONLY crypto fields (hash, signature)
   - **Status:** ✅ KEEP - Security-critical infrastructure
   - **Rationale:** Cannot be done in application (would compromise tamper-proof requirement)

2. **`create_audit_partition_trigger`** (audit_events table)
   - **File:** `001_audit_tables.sql:199`
   - **Purpose:** Automatic partition creation for audit log retention
   - **Scope:** ONLY audit_events table
   - **Status:** ✅ KEEP - Infrastructure automation
   - **Rationale:** Partitioning is infrastructure concern

---

## ❌ BAD: Business Logic Triggers (Replace with Explicit Patterns)

### Category 1: Timestamp Update Triggers (Most Common)

**Pattern:** Automatically update `updated_at` on UPDATE

**Files Affected:** 35+ instances across all examples

**Examples:**
- `examples/blog_simple/db/setup.sql:149-159` (tb_user, tb_post, tb_comment)
- `examples/blog_api/db/0_schema/04_triggers/041_triggers.sql:12-22`
- `examples/ecommerce_api/db/migrations/001_initial_schema.sql:260-303` (11 tables!)
- `examples/real_time_chat/db/migrations/001_chat_schema.sql:205-212`
- `examples/complete_cqrs_blog/migrations/001_initial_schema.sql:263-270`
- `examples/analytics_dashboard/db/migrations/001_analytics_schema.sql:281-291`
- `examples/ecommerce/schema.sql:445-461`

**Current Pattern (BAD):**
```sql
CREATE TRIGGER update_tb_user_updated_at BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

**Problems:**
- AI doesn't "see" updated_at being set
- Hidden side effect
- Testing requires full database setup

**Recommended Replacement:**
```sql
-- Use DEFAULT value + explicit application updates
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()  -- Explicit default
);
```

```python
# Explicit update in mutation
@mutation
async def update_user(id: str, data: UpdateUserInput, context: Context) -> User:
    return await context.db.update("tb_user", id, {
        **data.dict(),
        "updated_at": datetime.utcnow()  # Explicit!
    })
```

**Impact:** LOW - Easy to replace, does not affect audit logging architecture

---

### Category 2: Audit Logging Triggers (Documentation Example)

**Files Affected:** 1 instance (documentation example only)

**Example:**
- `examples/blog_enterprise/README.md:464-466`

**Current Pattern (BAD):**
```sql
CREATE TRIGGER audit_changes
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
```

**Problems:**
- Contradicts FraiseQL's explicit pattern
- AI doesn't see audit log creation
- Confusing for developers

**Recommended Replacement (FraiseQL's Correct Pattern):**

**Layer 1: Explicit Application Code**
```sql
CREATE FUNCTION create_post_with_audit(
    p_tenant_id UUID,
    p_user_id UUID,
    p_title TEXT,
    p_content TEXT
) RETURNS TABLE(...) AS $$
DECLARE
    v_post_id UUID;
BEGIN
    -- Business logic
    INSERT INTO tb_post (title, content, author_id)
    VALUES (p_title, p_content, p_user_id)
    RETURNING id INTO v_post_id;

    -- Explicit audit logging (AI-visible!)
    RETURN QUERY SELECT * FROM log_and_return_mutation(
        p_tenant_id := p_tenant_id,
        p_user_id := p_user_id,
        p_entity_type := 'post',
        p_entity_id := v_post_id,
        p_operation_type := 'INSERT',
        p_operation_subtype := 'new',
        p_changed_fields := ARRAY['title', 'content'],
        p_old_data := NULL,
        p_new_data := (SELECT row_to_json(p) FROM tb_post p WHERE id = v_post_id),
        p_metadata := jsonb_build_object('client', 'web')
    );
END;
$$ LANGUAGE plpgsql;
```

**Layer 2: Infrastructure Trigger (Already Exists)**
```sql
-- ONLY on audit_events table, ONLY for crypto fields
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Impact:** HIGH - This is the primary documentation example that needs correction

---

### Category 3: Business Logic Triggers (Notifications, Calculations)

**Files Affected:** 7 instances

**Examples:**
- `examples/real_time_chat/db/migrations/001_chat_schema.sql:232-277`
  - `message_event_trigger` (notify_message_event)
  - `typing_event_trigger` (notify_typing_event)
  - `presence_event_trigger` (notify_presence_event)
- `examples/analytics_dashboard/db/migrations/001_analytics_schema.sql:309`
  - `session_duration_trigger` (calculate_session_duration)
- `examples/ecommerce_api/db/0_schema/00_common/001_types/0013_cdc_logging.sql:115`
  - `trigger_notify_cdc_event` (notify_cdc_event)

**Current Pattern (BAD - Real-time Chat Example):**
```sql
CREATE TRIGGER message_event_trigger
    AFTER INSERT ON tb_message
    FOR EACH ROW EXECUTE FUNCTION notify_message_event();
```

**Problems:**
- Hidden notification logic
- Hard to test in isolation
- AI can't see notification code path

**Recommended Replacement:**
```python
# Explicit notification in application
@mutation
async def send_message(room_id: str, content: str, context: Context) -> Message:
    # Insert message
    message = await context.db.insert("tb_message", {
        "room_id": room_id,
        "content": content,
        "user_id": context.user_id
    })

    # Explicit notification (AI-visible!)
    await context.notify_message_event(message)

    return message
```

**Impact:** MEDIUM - Requires application-level notification code

---

### Category 4: Auto-Generation Triggers (Slugs, Timestamps)

**Files Affected:** 3 instances

**Examples:**
- `examples/blog_simple/db/setup.sql:179` - `tb_post_set_published_at`
- `examples/blog_simple/db/setup.sql:216` - `tb_post_auto_generate_slug`
- `examples/blog_simple/db/setup.sql:238` - `tb_tag_auto_generate_slug`

**Current Pattern (BAD):**
```sql
CREATE TRIGGER tb_post_auto_generate_slug
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION auto_generate_post_slug();
```

**Problems:**
- Hidden slug generation
- Hard to customize or override
- AI doesn't understand slug logic

**Recommended Replacement:**
```python
# Explicit slug generation in application
@mutation
async def create_post(title: str, content: str, context: Context) -> Post:
    slug = generate_slug(title)  # Explicit!

    return await context.db.insert("tb_post", {
        "title": title,
        "content": content,
        "slug": slug,  # Explicit!
        "published_at": datetime.utcnow() if publish else None
    })
```

**Impact:** LOW - Easy to move to application code

---

### Category 5: Documentation Example (Getting Started Guide)

**Files Affected:** 1 instance

**Example:**
- `docs/getting-started/first-hour.md:329-332`

**Current Pattern:**
```sql
CREATE TRIGGER tr_note_updated_at
    BEFORE UPDATE ON tb_note
    EXECUTE FUNCTION fn_update_updated_at();
```

**Recommended Replacement:**
```sql
-- Use DEFAULT value (explicit in schema)
CREATE TABLE tb_note (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Impact:** LOW - Documentation update only

---

## Summary by Example/Project

| Example/Project | Trigger Count | Type | Status |
|----------------|---------------|------|--------|
| **blog_simple** | 5 | Timestamp (3) + Auto-gen (2) | 🔄 Replace |
| **blog_api** | 3 | Timestamp | 🔄 Replace |
| **blog_enterprise** | 1 (doc only) | Audit (BAD pattern) | 🔄 **FIX** |
| **ecommerce** | 6 | Timestamp | 🔄 Replace |
| **ecommerce_api** | 12 | Timestamp (11) + CDC (1) | 🔄 Replace |
| **real_time_chat** | 6 | Timestamp (3) + Notification (3) | 🔄 Replace |
| **complete_cqrs_blog** | 3 | Timestamp | 🔄 Replace |
| **analytics_dashboard** | 5 | Timestamp (4) + Calculation (1) | 🔄 Replace |
| **enterprise_patterns/cqrs** | 3 | Timestamp | 🔄 Replace |
| **docs/getting-started** | 1 | Timestamp | 🔄 Replace |
| **src/fraiseql/enterprise** | 2 | Infrastructure (crypto) | ✅ **KEEP** |

**Total:** 47 triggers (45 to replace, 2 to keep)

---

## Recommended Actions (Priority Order)

### Priority 1: Fix Documentation Example (Critical)
- [ ] **blog_enterprise/README.md** - Replace bad audit trigger with FraiseQL's two-layer pattern
- [ ] **docs/getting-started/first-hour.md** - Remove trigger, use DEFAULT value

**Impact:** HIGH - These are what developers read first

---

### Priority 2: Create Documentation Guide (Critical)
- [ ] Create `docs/database/avoid-triggers.md`
  - Explain FraiseQL's two-layer pattern
  - Show BAD vs GOOD patterns
  - Document infrastructure trigger exceptions
  - Provide migration guide

**Impact:** HIGH - Sets architectural direction

---

### Priority 3: Create Linting Script (Important)
- [ ] Create `scripts/lint_no_triggers.py`
  - Allow infrastructure triggers (populate_crypto_trigger, audit_events)
  - Catch business logic triggers
  - Integrate into CI

**Impact:** MEDIUM - Prevents future bad patterns

---

### Priority 4: Update Examples (Optional)
- [ ] Timestamp triggers → Use DEFAULT + explicit updates
- [ ] Notification triggers → Move to application code
- [ ] Auto-generation triggers → Move to application code

**Impact:** LOW - Examples still work, but could be improved

**Note:** Example updates are OPTIONAL for this WP. The key is to:
1. Fix the documentation that shows bad patterns
2. Create the guide explaining the correct pattern
3. Add linting to prevent future bad patterns

---

## FraiseQL's Two-Layer Pattern (Summary)

### ✅ Layer 1: Explicit Application Code (AI-Visible)
- Business logic calls `log_and_return_mutation()` explicitly
- CDC data (changed_fields, old_data, new_data) passed as parameters
- AI can see the full audit code path

### ✅ Layer 2: Infrastructure Trigger (Tamper-Proof)
- `populate_crypto_trigger` on audit_events table ONLY
- Maintains cryptographic chain integrity (hash + signature)
- Application cannot tamper with crypto fields

### Why This Works:
- ✅ Audit logging is explicit and traceable
- ✅ AI can generate correct audit code
- ✅ Cryptographic integrity is tamper-proof
- ✅ Testing is straightforward
- ✅ Code is self-documenting

---

## Next Steps

1. **Update blog_enterprise/README.md** - Show correct pattern (Priority 1)
2. **Create docs/database/avoid-triggers.md** - Full guide (Priority 2)
3. **Create scripts/lint_no_triggers.py** - Prevent future issues (Priority 3)
4. **Optional:** Update examples to remove timestamp triggers (Priority 4)

---

**End of Audit Report**
