# FraiseQL's Explicit Audit Pattern: Why We Avoid Business Logic Triggers

**TL;DR:** FraiseQL uses explicit audit logging (`log_and_return_mutation()`) for business logic, with infrastructure triggers only for cryptographic integrity. This makes code AI-friendly, testable, and traceable.

---

## Table of Contents

- [The Two-Layer Pattern](#the-two-layer-pattern)
- [Why Avoid Business Logic Triggers?](#why-avoid-business-logic-triggers)
- [What NOT to Do](#what-not-to-do)
- [Acceptable Patterns](#acceptable-patterns)
- [Migration Guide](#migration-guide)
- [Examples](#examples)

---

## The Two-Layer Pattern

FraiseQL separates audit concerns into two layers:

### ✅ Layer 1: Explicit Application Code (AI-Visible)

**Purpose:** Business logic and audit logging
**Implementation:** Mutation functions explicitly call `log_and_return_mutation()`

```sql
CREATE FUNCTION create_post_with_audit(
    p_tenant_id UUID,
    p_user_id UUID,
    p_title TEXT,
    p_content TEXT
) RETURNS TABLE(
    entity_id UUID,
    entity_type TEXT,
    operation_type TEXT,
    success BOOLEAN
) AS $$
DECLARE
    v_post_id UUID;
BEGIN
    -- Business logic (explicit)
    INSERT INTO tb_post (title, content, author_id, tenant_id)
    VALUES (p_title, p_content, p_user_id, p_tenant_id)
    RETURNING id INTO v_post_id;

    -- Explicit audit logging (AI can see this!)
    RETURN QUERY SELECT * FROM log_and_return_mutation(
        p_tenant_id := p_tenant_id,
        p_user_id := p_user_id,
        p_entity_type := 'post',
        p_entity_id := v_post_id,
        p_operation_type := 'INSERT',
        p_operation_subtype := 'new',
        p_changed_fields := ARRAY['title', 'content'],
        p_message := 'Post created',
        p_old_data := NULL,
        p_new_data := (SELECT row_to_json(p) FROM tb_post p WHERE id = v_post_id),
        p_metadata := jsonb_build_object('client', 'web')
    );
END;
$$ LANGUAGE plpgsql;
```

**Why this works:**
- ✅ Audit logging is **explicit** - visible in the function code
- ✅ CDC data (`changed_fields`, `old_data`, `new_data`) is **explicit**
- ✅ AI models can **see and generate** the audit code
- ✅ **Testable** - check `audit_events` table after mutation
- ✅ **Traceable** - full code path is visible

---

### ✅ Layer 2: Infrastructure Trigger (Tamper-Proof)

**Purpose:** Cryptographic chain integrity
**Implementation:** Infrastructure trigger on `audit_events` table ONLY

```sql
-- ONLY on audit_events table, ONLY for crypto fields
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**What it does:**
- Populates `previous_hash` (links to last audit event)
- Computes `event_hash` (SHA-256 of event data)
- Generates `signature` (HMAC for tamper detection)

**Why a trigger is acceptable here:**
- 🔒 **Security-critical** - crypto chain must be tamper-proof
- 🏗️ **Infrastructure concern** - not business logic
- 🎯 **Limited scope** - ONLY audit_events table, ONLY crypto fields
- 📝 **Well-documented** - clear purpose and rationale

**Application cannot tamper with crypto fields** - this ensures audit integrity.

---

## Why Avoid Business Logic Triggers?

Business logic triggers are **problematic for AI-assisted development**:

### 1. **Implicit Behavior** (AI-hostile)
Triggers execute automatically without visible code paths, making it hard for AI to understand data flow.

```sql
-- ❌ BAD: AI doesn't "see" audit log creation
CREATE TRIGGER audit_changes
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
```

**Problem:** AI models can't trace:
- "Where is the audit log created?"
- "What fields are logged?"
- "Can I modify audit behavior?"

### 2. **Hidden Side Effects**
Changes in one table can affect others invisibly, confusing debugging.

```sql
-- ❌ BAD: Hidden notification
CREATE TRIGGER notify_on_message
    AFTER INSERT ON tb_message
    FOR EACH ROW EXECUTE FUNCTION send_notification();
```

**Problem:** Developer (and AI) must remember "inserting a message sends a notification."

### 3. **Testing Complexity**
Hard to isolate and test trigger logic independently.

```sql
-- ❌ BAD: Can't test validation without database
CREATE TRIGGER validate_status
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION validate_status_transition();
```

**Problem:** Unit testing requires full database setup.

### 4. **Code Generation Issues**
AI models struggle to generate correct trigger syntax vs explicit code.

**AI is better at:**
```python
# ✅ GOOD: AI can generate this
@mutation
async def update_post(id: str, status: str) -> Post:
    if status == "archived":
        raise ValueError("Cannot archive")
    return await db.update("tb_post", id, {"status": status})
```

**AI struggles with:**
```sql
-- ❌ BAD: AI often gets trigger syntax wrong
CREATE TRIGGER validate_status BEFORE UPDATE ON tb_post
    FOR EACH ROW WHEN (NEW.status != OLD.status)
    EXECUTE FUNCTION validate_status_transition();
```

### 5. **Maintenance Burden**
Developers (and AI) must remember "invisible" trigger logic when modifying schema.

### 6. **Performance Unpredictability**
Triggers can cause cascading effects that are hard to profile.

### 7. **Documentation Drift**
Trigger logic often becomes undocumented or forgotten.

---

## What NOT to Do

### ❌ BAD: Audit Triggers on Business Tables

```sql
-- ❌ AVOID: Implicit audit logging
CREATE TRIGGER audit_changes
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
```

**Problems:**
- AI doesn't see audit log creation
- Hidden side effect
- Hard to customize per operation

**Use instead:** [FraiseQL's Two-Layer Pattern](#the-two-layer-pattern)

---

### ❌ BAD: Timestamp Update Triggers

```sql
-- ❌ AVOID: Hidden timestamp updates
CREATE TRIGGER update_timestamp
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
```

**Problems:**
- AI doesn't see timestamp being set
- Implicit behavior
- Can't control when timestamp updates

**Use instead:**

```sql
-- ✅ GOOD: Explicit default
CREATE TABLE tb_post (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

```python
# ✅ GOOD: Explicit update in application
@mutation
async def update_post(id: str, data: UpdatePostInput, context: Context) -> Post:
    return await context.db.update("tb_post", id, {
        **data.dict(),
        "updated_at": datetime.utcnow()  # Explicit!
    })
```

---

### ❌ BAD: Cascade/Cleanup Triggers

```sql
-- ❌ AVOID: Hidden cleanup logic
CREATE TRIGGER delete_orphan_comments
    AFTER DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION cleanup_orphan_comments();
```

**Problems:**
- Hidden cascade behavior
- AI can't see deletion logic
- Debugging is confusing

**Use instead:**

```sql
-- ✅ GOOD: Explicit foreign key cascade
CREATE TABLE tb_comment (
    id UUID PRIMARY KEY,
    post_id UUID REFERENCES tb_post(id) ON DELETE CASCADE
);
```

```python
# ✅ GOOD: Explicit application logic
@mutation
async def delete_post(id: str, context: Context) -> DeletePostResult:
    async with context.db.transaction():
        # Explicit cascade (AI-visible)
        await context.db.delete("tb_comment", post_id=id)
        await context.db.delete("tb_post", id=id)
        return DeletePostSuccess(message="Post and comments deleted")
```

---

### ❌ BAD: Validation Triggers

```sql
-- ❌ AVOID: Hidden validation logic
CREATE TRIGGER validate_post_status
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION validate_status_transition();
```

**Problems:**
- Validation rules hidden in trigger
- AI can't generate validation code
- Hard to test independently

**Use instead:**

```python
# ✅ GOOD: Explicit validation (Pydantic)
class UpdatePostInput(BaseModel):
    status: Literal["draft", "published", "archived"]

    @validator("status")
    def validate_status_transition(cls, v, values):
        current_status = values.get("current_status")
        if current_status == "archived" and v != "archived":
            raise ValueError("Cannot un-archive a post")
        return v

@mutation
async def update_post(id: str, data: UpdatePostInput, context: Context) -> Post:
    # Validation happened above (explicit!)
    return await context.db.update("tb_post", id, data.dict())
```

```sql
-- ✅ GOOD: CHECK constraint (explicit in schema)
CREATE TABLE tb_post (
    id UUID PRIMARY KEY,
    status TEXT CHECK (status IN ('draft', 'published', 'archived'))
);
```

---

### ❌ BAD: Notification Triggers

```sql
-- ❌ AVOID: Hidden notification logic
CREATE TRIGGER notify_on_message
    AFTER INSERT ON tb_message
    FOR EACH ROW EXECUTE FUNCTION send_notification();
```

**Problems:**
- Notification logic is invisible
- Can't customize per use case
- Hard to test

**Use instead:**

```python
# ✅ GOOD: Explicit notification in application
@mutation
async def send_message(room_id: str, content: str, context: Context) -> Message:
    # Insert message
    message = await context.db.insert("tb_message", {
        "room_id": room_id,
        "content": content,
        "user_id": context.user_id
    })

    # Explicit notification (AI-visible!)
    await context.notify_room(room_id, message)

    return message
```

---

### ❌ BAD: Auto-Generation Triggers

```sql
-- ❌ AVOID: Hidden slug generation
CREATE TRIGGER auto_generate_slug
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION generate_slug_from_title();
```

**Problems:**
- Slug generation logic is hidden
- Can't customize or override
- AI doesn't understand logic

**Use instead:**

```python
# ✅ GOOD: Explicit slug generation
from slugify import slugify

@mutation
async def create_post(title: str, content: str, context: Context) -> Post:
    slug = slugify(title)  # Explicit!

    return await context.db.insert("tb_post", {
        "title": title,
        "content": content,
        "slug": slug,  # Explicit!
        "published_at": datetime.utcnow() if publish else None
    })
```

---

## Acceptable Patterns

### ✅ GOOD: Explicit Schema Features (AI-Friendly)

**1. DEFAULT values** - Clear and explicit
```sql
created_at TIMESTAMPTZ DEFAULT NOW()
```

**2. CHECK constraints** - Documented in schema
```sql
CHECK (status IN ('draft', 'published', 'archived'))
```

**3. FOREIGN KEY CASCADE** - Explicit in schema
```sql
REFERENCES tb_post(id) ON DELETE CASCADE
```

**4. GENERATED ALWAYS AS** - Explicit computed column
```sql
full_name TEXT GENERATED ALWAYS AS (first_name || ' ' || last_name) STORED
```

**5. Explicit Functions** - Called from application
```python
result = await db.call_function("create_post_with_audit", ...)
```

---

### ✅ ACCEPTABLE EXCEPTION: Infrastructure Triggers (Security-Critical)

**1. Cryptographic Chain Integrity** - Tamper-proof audit trail

```sql
-- ONLY on audit_events table, ONLY for crypto fields
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Why acceptable:**
- 🔒 Tamper-proof requirement (application code shouldn't set crypto fields)
- 🏗️ Infrastructure concern (not business logic)
- 🎯 Limited scope (only audit table, only crypto fields)
- 📝 Well-documented (clear purpose and rationale)
- 🛡️ Security-critical (breaking this would compromise audit integrity)

**Application cannot set:**
- `previous_hash` - Must link to actual last event
- `event_hash` - Must be computed from event data
- `signature` - Must use server-side secret key

---

## Migration Guide

If you have existing triggers, here's how to migrate to FraiseQL's explicit pattern:

### Step 1: Identify Your Triggers

```sql
SELECT
    trigger_name,
    event_object_table,
    action_statement
FROM information_schema.triggers
WHERE trigger_schema = 'public'
ORDER BY event_object_table, trigger_name;
```

### Step 2: Classify Trigger Purpose

| Trigger Type | Migration Strategy |
|--------------|-------------------|
| Audit logging | Use `log_and_return_mutation()` |
| Timestamps | Use `DEFAULT NOW()` + explicit updates |
| Cascades | Use `ON DELETE CASCADE` or explicit app logic |
| Validation | Use `CHECK` constraints or Pydantic validation |
| Notifications | Explicit app-level notification code |
| Auto-generation | Explicit generation in app code |

### Step 3: Replace with Explicit Pattern

**Example: Audit Logging Trigger → FraiseQL Pattern**

**Before (Trigger):**
```sql
CREATE TRIGGER audit_changes
    AFTER INSERT ON tb_post
    FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
```

**After (FraiseQL):**
```sql
CREATE FUNCTION create_post_with_audit(...) RETURNS TABLE(...) AS $$
BEGIN
    INSERT INTO tb_post (...) RETURNING id INTO v_post_id;

    -- Explicit audit call
    RETURN QUERY SELECT * FROM log_and_return_mutation(
        p_entity_type := 'post',
        p_entity_id := v_post_id,
        p_operation_type := 'INSERT',
        ...
    );
END;
$$ LANGUAGE plpgsql;
```

---

**Example: Timestamp Trigger → Explicit Updates**

**Before (Trigger):**
```sql
CREATE TRIGGER update_timestamp
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
```

**After (Explicit):**
```sql
-- Use DEFAULT (set once on INSERT)
CREATE TABLE tb_post (
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

```python
# Explicit update in mutations
@mutation
async def update_post(id: str, title: str, context: Context) -> Post:
    return await context.db.update("tb_post", id, {
        "title": title,
        "updated_at": datetime.utcnow()  # Explicit!
    })
```

---

### Step 4: Test Thoroughly

**Before dropping triggers:**

1. **Unit test the new explicit code**
   ```python
   async def test_audit_logging():
       post = await create_post_with_audit(...)
       audit_event = await db.fetch_one("SELECT * FROM audit_events WHERE entity_id = $1", post.id)
       assert audit_event is not None
   ```

2. **Integration test the full flow**
   ```python
   async def test_mutation_with_audit():
       result = await schema.execute("""
           mutation { createPost(input: {...}) { id } }
       """)
       assert result.errors is None
   ```

3. **Verify behavior matches trigger**
   - Compare audit logs before/after
   - Check timestamp updates
   - Verify cascade behavior

---

### Step 5: Drop Old Trigger

```sql
DROP TRIGGER IF EXISTS audit_changes ON tb_post;
DROP FUNCTION IF EXISTS audit_table_changes();
```

---

## Examples

### Complete Example: Post Creation with Audit

**FraiseQL's Two-Layer Pattern:**

```sql
-- Layer 1: Explicit Application Code
CREATE FUNCTION create_post_with_audit(
    p_tenant_id UUID,
    p_user_id UUID,
    p_title TEXT,
    p_content TEXT,
    p_status TEXT DEFAULT 'draft'
) RETURNS TABLE(
    entity_id UUID,
    entity_type TEXT,
    operation_type TEXT,
    success BOOLEAN
) AS $$
DECLARE
    v_post_id UUID;
BEGIN
    -- Validation (explicit in code)
    IF p_status NOT IN ('draft', 'published', 'archived') THEN
        RAISE EXCEPTION 'Invalid status: %', p_status;
    END IF;

    -- Business logic (explicit)
    INSERT INTO tb_post (
        title,
        content,
        status,
        author_id,
        tenant_id,
        created_at,
        updated_at
    )
    VALUES (
        p_title,
        p_content,
        p_status,
        p_user_id,
        p_tenant_id,
        NOW(),  -- Explicit
        NOW()   -- Explicit
    )
    RETURNING id INTO v_post_id;

    -- Explicit audit logging (AI-visible!)
    RETURN QUERY SELECT * FROM log_and_return_mutation(
        p_tenant_id := p_tenant_id,
        p_user_id := p_user_id,
        p_entity_type := 'post',
        p_entity_id := v_post_id,
        p_operation_type := 'INSERT',
        p_operation_subtype := 'new',
        p_changed_fields := ARRAY['title', 'content', 'status'],
        p_message := format('Post "%s" created with status "%s"', p_title, p_status),
        p_old_data := NULL,
        p_new_data := (
            SELECT row_to_json(p)
            FROM tb_post p
            WHERE id = v_post_id
        ),
        p_metadata := jsonb_build_object(
            'client', 'web',
            'ip_address', current_setting('app.client_ip', true),
            'user_agent', current_setting('app.user_agent', true)
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Layer 2: Infrastructure Trigger (Crypto Chain)
-- Already exists in src/fraiseql/enterprise/migrations/002_unified_audit.sql
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Python GraphQL Mutation:**

```python
from fraiseql import mutation, Context
from datetime import datetime

@mutation
async def create_post(
    title: str,
    content: str,
    status: str = "draft",
    context: Context = None
) -> Post:
    """Create a new post with explicit audit logging."""

    # Call the explicit audit function
    result = await context.db.fetch_one(
        """
        SELECT * FROM create_post_with_audit(
            p_tenant_id := $1,
            p_user_id := $2,
            p_title := $3,
            p_content := $4,
            p_status := $5
        )
        """,
        context.tenant_id,
        context.user_id,
        title,
        content,
        status
    )

    # Return the created post
    return await context.db.fetch_one(
        "SELECT * FROM v_post WHERE id = $1",
        result["entity_id"]
    )
```

**Why This Works:**

✅ **Audit logging is explicit** - visible in `create_post_with_audit()` function
✅ **CDC data is explicit** - `changed_fields`, `old_data`, `new_data` are parameters
✅ **Validation is explicit** - status check is visible in code
✅ **Timestamps are explicit** - `NOW()` calls are visible
✅ **Crypto is infrastructure** - trigger only populates hash/signature (tamper-proof)
✅ **AI-friendly** - full code path is traceable
✅ **Testable** - can check `audit_events` table after mutation

---

## Summary

**FraiseQL's Philosophy:** Explicit over implicit.

### ❌ Avoid Business Logic Triggers:
- Audit logging triggers
- Timestamp update triggers
- Cascade/cleanup triggers
- Validation triggers
- Notification triggers
- Auto-generation triggers

### ✅ Use Explicit Patterns:
- **Audit:** Call `log_and_return_mutation()` explicitly
- **Timestamps:** Use `DEFAULT NOW()` + explicit updates
- **Cascades:** Use `ON DELETE CASCADE` or explicit app logic
- **Validation:** Use `CHECK` constraints or Pydantic
- **Notifications:** Explicit app-level code
- **Generation:** Explicit function calls

### ✅ Acceptable Exception:
- **Infrastructure triggers** for cryptographic chain integrity
- ONLY on `audit_events` table
- ONLY for security-critical tamper-proofing
- Well-documented and limited in scope

---

**Benefits of FraiseQL's Explicit Pattern:**

1. 🤖 **AI-Friendly** - Code paths are visible and traceable
2. 🧪 **Testable** - Easy to unit test explicit code
3. 📝 **Self-Documenting** - Code IS the documentation
4. 🐛 **Debuggable** - No hidden side effects
5. 🔒 **Secure** - Crypto chain is tamper-proof
6. 🚀 **Performant** - Easier to profile and optimize
7. 📚 **Maintainable** - Future developers understand the code

---

**Related Documentation:**
- [Trinity Pattern](../core/trinity-pattern.md) - FraiseQL's tb_/v_/tv_ naming
- [Database Patterns](../advanced/database-patterns.md) - Advanced patterns
- [Audit Trails](../security-compliance/README.md) - Enterprise audit system

---

**Questions or Feedback?**
- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Documentation: https://fraiseql.org/docs
