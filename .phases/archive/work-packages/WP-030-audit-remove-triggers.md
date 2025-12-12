# WP-030: Audit and Remove Database Triggers in Favor of Explicit Patterns

**Assignee:** ENG-CORE + TW-CORE
**Priority:** P1 (Important - Architecture Decision)
**Estimated Hours:** 10
**Week:** 3
**Dependencies:** None

---

## Objective

Audit all examples and documentation for **business logic triggers** and document FraiseQL's correct explicit pattern (application-level audit logging + infrastructure-level crypto triggers).

**Current State:** Some documentation examples may show bad trigger patterns (business logic triggers on tables) which are implicit and harder for AI-assisted development.

**Target State:** All examples demonstrate FraiseQL's correct two-layer pattern:
1. **Explicit layer** - Business logic calls `log_and_return_mutation()` (AI-visible)
2. **Infrastructure layer** - Crypto chain maintained by infrastructure trigger (tamper-proof)

Documentation clearly distinguishes between:
- ❌ **BAD triggers**: Business logic triggers (audit on tb_post, timestamp updates, cascades, validation)
- ✅ **GOOD triggers**: Infrastructure triggers (crypto chain on audit_events table only)

---

## Problem Statement

**Why Triggers Are Problematic for AI-Assisted Development:**

1. **Implicit Behavior** - Triggers execute automatically without visible code paths, making it hard for AI to understand data flow
2. **Hidden Side Effects** - Changes in one table can affect others invisibly, confusing debugging
3. **Testing Complexity** - Hard to isolate and test trigger logic independently
4. **Code Generation Issues** - AI models struggle to generate correct trigger syntax vs explicit code
5. **Maintenance Burden** - Developers (and AI) must remember "invisible" trigger logic when modifying schema
6. **Performance Unpredictability** - Triggers can cause cascading effects that are hard to profile
7. **Documentation Drift** - Trigger logic often becomes undocumented or forgotten

**FraiseQL Philosophy:** Favor **explicit over implicit**. AI-assisted development thrives on clear, traceable code paths.

---

## Scope of Audit

### Examples to Audit
1. `examples/blog_enterprise/README.md` - Known trigger usage (audit logging)
2. `examples/blog_simple/` - Check for any triggers in SQL files
3. `examples/*/` - All example directories for trigger usage
4. Standalone `.py` files in `examples/` with SQL strings

### Documentation to Audit
1. `docs/database/` - Any trigger recommendations
2. `docs/advanced/` - Advanced patterns using triggers
3. `docs/patterns/` - Pattern documentation
4. `docs/core/` - Core concepts
5. Tutorial and guide content

### Code to Audit
1. `src/fraiseql/` - Framework code (should not create triggers)
2. Migration files
3. Test fixtures

---

## Trigger Types to Replace

### 1. Audit/History Triggers

#### ❌ BAD: Triggers on Business Tables

**Problematic Pattern:**
```sql
-- ❌ DON'T DO THIS - Implicit audit on business tables
CREATE TRIGGER audit_changes
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION audit_table_changes();
```

**Problems:**
- AI doesn't "see" audit logs being created
- Debugging requires understanding trigger execution order
- Testing requires full database setup
- Performance impact hidden from application code

#### ✅ GOOD: FraiseQL's Explicit Pattern

**FraiseQL uses a two-layer approach:**

**Layer 1: Explicit Application Code (AI-Visible)**
```sql
-- PostgreSQL function with explicit audit call
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

**Layer 2: Infrastructure Trigger (Tamper-Proof Crypto Chain)**
```sql
-- ✅ ACCEPTABLE - Infrastructure trigger for security-critical operations
-- ONLY on audit_events table, ONLY for crypto fields
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Why This Works:**
- ✅ **Audit logging is explicit** - `log_and_return_mutation()` is called explicitly in mutation functions
- ✅ **AI can see the audit** - The function call is visible in code
- ✅ **CDC data is explicit** - `changed_fields`, `old_data`, `new_data` are explicit parameters
- ✅ **Crypto is infrastructure** - The trigger only populates hash/signature (tamper-proof requirement)
- ✅ **Testable** - Can test audit logging by checking `audit_events` table
- ✅ **Traceable** - Code path from mutation → log_and_return_mutation → audit_events is clear

---

### 2. Timestamp Triggers ❌ AVOID

**Current Pattern (Problematic):**
```sql
-- Implicit timestamp updates
CREATE TRIGGER update_timestamp
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
```

**Preferred Pattern (Explicit):**
```sql
-- Use DEFAULT and explicit updates
CREATE TABLE tb_post (
    id UUID PRIMARY KEY,
    title TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),  -- Explicit default
    updated_at TIMESTAMPTZ DEFAULT NOW()   -- Explicit default
);
```

```python
# Explicit update in application
@mutation
async def update_post(id: str, data: UpdatePostInput, context: Context) -> Post:
    # AI sees the timestamp update explicitly
    return await context.db.update("tb_post", id, {
        **data.dict(),
        "updated_at": datetime.utcnow()  # Explicit!
    })
```

**Why Better:**
- AI understands "updated_at is set by application code"
- No hidden magic
- Easier to test (just check the field value)

---

### 3. Cascade/Cleanup Triggers ❌ AVOID

**Current Pattern (Problematic):**
```sql
-- Implicit cleanup via trigger
CREATE TRIGGER delete_orphan_comments
    AFTER DELETE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION cleanup_orphan_comments();
```

**Preferred Pattern (Explicit):**
```python
# Explicit cascade in application
@mutation
async def delete_post(id: str, context: Context) -> DeletePostResult:
    async with context.db.transaction():
        # AI sees the cascade logic
        await context.db.delete("tb_comment", post_id=id)  # Explicit!
        await context.db.delete("tb_post", id=id)

        return DeletePostSuccess(message="Post and comments deleted")
```

```sql
-- Or use database CASCADE (explicit in schema)
CREATE TABLE tb_comment (
    id UUID PRIMARY KEY,
    post_id UUID REFERENCES tb_post(id) ON DELETE CASCADE  -- Explicit!
);
```

**Why Better:**
- Cascade behavior documented in schema (foreign key)
- OR explicit in application code
- AI can trace deletion logic
- No surprise side effects

---

### 4. Validation Triggers ❌ AVOID

**Current Pattern (Problematic):**
```sql
-- Implicit validation via trigger
CREATE TRIGGER validate_post_status
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION validate_post_status_transition();
```

**Preferred Pattern (Explicit):**
```python
# Explicit validation in application (Pydantic)
class UpdatePostInput(BaseModel):
    status: Literal["draft", "published", "archived"]

    @validator("status")
    def validate_status_transition(cls, v, values):
        # AI understands validation rules
        current_status = values.get("current_status")
        if current_status == "archived" and v != "archived":
            raise ValueError("Cannot un-archive a post")
        return v

@mutation
async def update_post(id: str, data: UpdatePostInput, context: Context) -> Post:
    # Validation happens here (explicit)
    return await context.db.update("tb_post", id, data.dict())
```

```sql
-- Or use CHECK constraints (explicit in schema)
CREATE TABLE tb_post (
    id UUID PRIMARY KEY,
    status TEXT CHECK (status IN ('draft', 'published', 'archived'))  -- Explicit!
);
```

**Why Better:**
- Validation rules visible in type definitions
- AI can generate correct validation code
- Python validation easier to test than SQL triggers

---

## Acceptable PostgreSQL Automation

**✅ GOOD: Explicit Schema Features** (AI-friendly)

1. **DEFAULT values** - Clear and explicit
   ```sql
   created_at TIMESTAMPTZ DEFAULT NOW()
   ```

2. **CHECK constraints** - Documented in schema
   ```sql
   CHECK (status IN ('draft', 'published'))
   ```

3. **FOREIGN KEY CASCADE** - Explicit in schema
   ```sql
   REFERENCES tb_post(id) ON DELETE CASCADE
   ```

4. **GENERATED ALWAYS AS** - Explicit computed column
   ```sql
   full_name TEXT GENERATED ALWAYS AS (first_name || ' ' || last_name) STORED
   ```

5. **Explicit Functions** - Called from application
   ```python
   result = await db.call_function("create_post_with_audit", ...)
   ```

**✅ ACCEPTABLE EXCEPTION: Infrastructure Triggers** (Security-Critical)

1. **Cryptographic Chain Integrity** - Tamper-proof audit trail
   ```sql
   -- ONLY on audit_events table, ONLY for crypto fields
   CREATE TRIGGER populate_crypto_trigger
       BEFORE INSERT ON audit_events
       FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
   ```
   **Why acceptable:**
   - Tamper-proof requirement (application code shouldn't set crypto fields)
   - Infrastructure concern (not business logic)
   - Limited scope (only audit table, only crypto fields)
   - Well-documented (clear purpose and rationale)
   - Security-critical (breaking this would compromise audit integrity)

**❌ AVOID: Business Logic Triggers** (AI-hostile)

1. **BEFORE/AFTER triggers on business tables** - Hidden side effects
2. **Audit triggers on tb_* tables** - Use explicit `log_and_return_mutation()` instead
3. **Timestamp update triggers** - Use explicit updates instead
4. **Cascade/cleanup triggers** - Use ON DELETE CASCADE or explicit app logic
5. **Validation triggers** - Use CHECK constraints or Pydantic validation
6. **INSTEAD OF triggers** - Confusing behavior
7. **Event triggers** - Implicit DDL hooks
8. **Rule systems** - Legacy and confusing

---

## Implementation Steps

### Step 1: Audit Current Usage (2 hours)

**Grep for triggers:**
```bash
# Find all trigger definitions (exclude infrastructure triggers)
grep -rn "CREATE TRIGGER\|CREATE OR REPLACE TRIGGER" \
  examples/ docs/ \
  --include="*.sql" --include="*.md" --include="*.py" \
  | grep -v "populate_crypto_trigger" \
  | grep -v "audit_events"

# Find trigger functions
grep -rn "EXECUTE FUNCTION\|EXECUTE PROCEDURE" \
  examples/ docs/ \
  --include="*.sql" --include="*.md" --include="*.py" \
  | grep -v "populate_crypto_trigger" \
  | grep -v "audit_events"
```

**Document findings:**
- Create inventory: `TRIGGER-AUDIT-FINDINGS.md`
- List: File path, trigger name, purpose, classification (BAD business logic vs GOOD infrastructure)
- For BAD triggers: recommended replacement pattern

---

### Step 2: Update Examples (3 hours)

**For each example with triggers:**

1. **blog_enterprise/README.md** (line 464-466)
   - **Replace bad trigger example with FraiseQL's correct pattern**
   - Show two-layer approach: explicit `log_and_return_mutation()` + infrastructure crypto trigger
   - Add note: "FraiseQL uses explicit audit logging with infrastructure-level crypto chain"
   - Example:
   ```sql
   -- ❌ OLD (Bad pattern)
   CREATE TRIGGER audit_changes
       AFTER INSERT OR UPDATE OR DELETE ON tb_post
       FOR EACH ROW EXECUTE FUNCTION audit_table_changes();

   -- ✅ NEW (FraiseQL's correct pattern)
   CREATE FUNCTION create_post_with_audit(...) RETURNS TABLE(...) AS $$
   BEGIN
       -- Business logic
       INSERT INTO tb_post (...) RETURNING id INTO v_post_id;

       -- Explicit audit logging (AI-visible!)
       RETURN QUERY SELECT * FROM log_and_return_mutation(
           p_entity_type := 'post',
           p_entity_id := v_post_id,
           p_operation_type := 'INSERT',
           ...
       );
   END;
   $$ LANGUAGE plpgsql;

   -- Infrastructure trigger (acceptable for crypto integrity)
   CREATE TRIGGER populate_crypto_trigger
       BEFORE INSERT ON audit_events
       FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
   ```

2. **Check blog_simple SQL files**
   - Verify no BAD business logic triggers exist
   - If found, replace with explicit patterns

3. **Check other examples**
   - Audit for business logic triggers
   - Replace with FraiseQL's explicit pattern
   - Update README explanations to show correct pattern

---

### Step 3: Update Documentation (3 hours)

**Create new guide: `docs/database/avoid-triggers.md`**

```markdown
# FraiseQL's Explicit Audit Pattern (Why We Avoid Business Logic Triggers)

**TL;DR:** FraiseQL uses explicit audit logging (`log_and_return_mutation()`) for business logic, with infrastructure triggers only for cryptographic integrity.

## The Two-Layer Pattern

### ✅ Layer 1: Explicit Application Code (AI-Visible)

Mutation functions explicitly call `log_and_return_mutation()`:

```sql
CREATE FUNCTION create_post_with_audit(...) RETURNS TABLE(...) AS $$
BEGIN
    -- Business logic
    INSERT INTO tb_post (...) RETURNING id INTO v_post_id;

    -- Explicit audit (AI can see this!)
    RETURN QUERY SELECT * FROM log_and_return_mutation(
        p_entity_type := 'post',
        p_entity_id := v_post_id,
        p_operation_type := 'INSERT',
        p_changed_fields := ARRAY['title', 'content'],
        p_old_data := NULL,
        p_new_data := (SELECT row_to_json(p) FROM tb_post p WHERE id = v_post_id),
        ...
    );
END;
$$ LANGUAGE plpgsql;
```

### ✅ Layer 2: Infrastructure Trigger (Tamper-Proof)

Cryptographic chain maintained by infrastructure trigger:

```sql
-- ONLY on audit_events table, ONLY for crypto fields
CREATE TRIGGER populate_crypto_trigger
    BEFORE INSERT ON audit_events
    FOR EACH ROW EXECUTE FUNCTION populate_crypto_fields();
```

**Why this works:**
- ✅ Audit logging is explicit and visible to AI
- ✅ CDC data (changed_fields, old/new data) is explicit
- ✅ Crypto integrity is infrastructure-level (can't be tampered with)
- ✅ Testable and traceable

## ❌ What NOT to Do

### Bad: Audit Triggers on Business Tables
[Examples of triggers on tb_post, tb_user, etc.]

### Bad: Timestamp Update Triggers
[Use DEFAULT NOW() + explicit updates instead]

### Bad: Cascade/Cleanup Triggers
[Use ON DELETE CASCADE or explicit app logic instead]

### Bad: Validation Triggers
[Use CHECK constraints or Pydantic validation instead]

## Acceptable Exceptions

1. **Cryptographic Chain Infrastructure** - Only on audit_events table
2. **Security-Critical Tamper-Proofing** - Must be well-documented
3. **Legacy Database Integration** - When migrating, document thoroughly

## Migration Guide

[How to migrate from bad triggers to FraiseQL's explicit pattern]
```

**Update existing docs:**
- `docs/database/table-naming-conventions.md` - Add trigger avoidance note
- `docs/patterns/` - Remove any trigger patterns
- `docs/advanced/database-patterns.md` - Replace trigger examples

---

### Step 4: Create Linting/Validation (2 hours)

**Add to CI/CD:**

```python
# scripts/lint_no_triggers.py
"""
Lint examples and docs for BAD trigger usage (business logic triggers).
Allows infrastructure triggers (crypto chain on audit_events).
Fails CI if bad triggers found.
"""

import re
import sys
from pathlib import Path

TRIGGER_PATTERN = re.compile(
    r'CREATE\s+(OR\s+REPLACE\s+)?TRIGGER',
    re.IGNORECASE
)

# Allowed infrastructure trigger patterns
ALLOWED_TRIGGER_PATTERNS = [
    r'populate_crypto_trigger',  # Cryptographic chain infrastructure
    r'ON\s+audit_events',        # Triggers on audit_events table only
]

# Allowed files with infrastructure triggers
ALLOWED_TRIGGER_FILES = [
    'src/fraiseql/enterprise/migrations/002_unified_audit.sql',  # Infrastructure
]

def is_allowed_trigger(trigger_line: str, file_path: str) -> bool:
    """Check if trigger is an allowed infrastructure exception."""
    if str(file_path) in ALLOWED_TRIGGER_FILES:
        return True

    for pattern in ALLOWED_TRIGGER_PATTERNS:
        if re.search(pattern, trigger_line, re.IGNORECASE):
            return True

    return False

def check_file(file_path: Path) -> list[str]:
    """Check file for BAD trigger usage."""
    issues = []
    content = file_path.read_text()

    for i, line in enumerate(content.splitlines(), 1):
        if TRIGGER_PATTERN.search(line):
            if not is_allowed_trigger(line, str(file_path)):
                issues.append(
                    f"{file_path}:{i} - Business logic trigger found (use explicit pattern instead)"
                )

    return issues

def main():
    """Scan all files for bad triggers."""
    paths_to_check = [
        Path("examples/"),
        Path("docs/"),
    ]

    all_issues = []
    for base_path in paths_to_check:
        for pattern in ["**/*.sql", "**/*.md", "**/*.py"]:
            for file_path in base_path.glob(pattern):
                all_issues.extend(check_file(file_path))

    if all_issues:
        print("❌ Business logic trigger usage found:")
        for issue in all_issues:
            print(f"  {issue}")
        print("\n💡 Use FraiseQL's explicit pattern instead:")
        print("   - Call log_and_return_mutation() explicitly in mutations")
        print("   - Infrastructure triggers (crypto chain) are OK on audit_events")
        print("   - See docs/database/avoid-triggers.md")
        sys.exit(1)
    else:
        print("✅ No business logic triggers found")
        print("   (Infrastructure triggers on audit_events are allowed)")

if __name__ == "__main__":
    main()
```

**Add to CI:**
```yaml
# .github/workflows/docs-quality.yml
- name: Check for trigger usage
  run: python scripts/lint_no_triggers.py
```

---

## Acceptance Criteria

### Examples
- ✅ Zero **business logic** trigger usage in all examples
- ✅ Infrastructure triggers (crypto chain) properly documented
- ✅ All examples demonstrate FraiseQL's explicit pattern (`log_and_return_mutation()`)
- ✅ README explanations updated with correct two-layer pattern

### Documentation
- ✅ New guide: `docs/database/avoid-triggers.md` created
  - Documents FraiseQL's two-layer approach
  - Explains infrastructure trigger exception
  - Shows BAD vs GOOD patterns
- ✅ All existing BAD trigger examples replaced with correct pattern
- ✅ Clear migration guidance provided
- ✅ AI-friendly rationale documented

### Code Quality
- ✅ Linting script created and integrated into CI
  - Allows infrastructure triggers (populate_crypto_trigger, audit_events)
  - Catches business logic triggers
  - Clear error messages
- ✅ Exceptions list properly documented
  - `src/fraiseql/enterprise/migrations/002_unified_audit.sql` (infrastructure)

### Testing
- ✅ All examples still function correctly
- ✅ Test suites don't rely on business logic triggers
- ✅ Infrastructure triggers (crypto chain) verified working

---

## Testing Plan

### Manual Testing

1. **Verify Examples Work:**
   ```bash
   # Run each example
   cd examples/blog_simple && python app.py
   cd examples/blog_enterprise && python app.py
   # Ensure no trigger-related errors
   ```

2. **Check Documentation:**
   - Read new `avoid-triggers.md` guide
   - Verify all code examples are explicit
   - Check for broken links

3. **Run Linting:**
   ```bash
   python scripts/lint_no_triggers.py
   # Should pass (zero triggers found)
   ```

### Automated Testing

```python
# tests/test_no_triggers.py
def test_examples_have_no_triggers():
    """Ensure examples don't use triggers."""
    trigger_pattern = re.compile(r'CREATE\s+TRIGGER', re.IGNORECASE)

    for sql_file in Path("examples/").rglob("*.sql"):
        content = sql_file.read_text()
        assert not trigger_pattern.search(content), \
            f"{sql_file} contains trigger (use explicit pattern)"

def test_documentation_recommends_no_triggers():
    """Ensure docs recommend explicit patterns."""
    avoid_triggers_doc = Path("docs/database/avoid-triggers.md")
    assert avoid_triggers_doc.exists(), "Missing avoid-triggers guide"

    content = avoid_triggers_doc.read_text()
    assert "explicit" in content.lower()
    assert "AI-assisted" in content or "AI-friendly" in content
```

---

## DO NOT

- ❌ Remove or flag infrastructure triggers (populate_crypto_trigger is GOOD!)
- ❌ Remove business logic triggers without showing FraiseQL's explicit pattern
- ❌ Leave examples broken after trigger pattern updates
- ❌ Create documentation that's preachy (focus on AI-assisted dev benefits)
- ❌ Document the pattern incorrectly (must show two-layer approach)
- ❌ Make developers feel bad for using triggers (just explain FraiseQL's better way)

---

## Success Metrics

### Quantitative
- **Business logic trigger count:** 0 in examples and docs
- **Infrastructure triggers:** Properly documented (populate_crypto_trigger on audit_events)
- **CI passing:** Linting script passes (allows infrastructure, catches business logic)
- **Examples working:** All examples run successfully with correct pattern
- **Documentation complete:** New guide + updated existing docs with two-layer pattern

### Qualitative
- **AI-friendly:** Code paths are explicit and traceable (log_and_return_mutation visible)
- **Security-aware:** Infrastructure triggers documented as legitimate exception
- **Maintainable:** Future developers understand two-layer approach
- **Testable:** Business logic can be tested independently
- **Clear rationale:** Documentation explains "why" FraiseQL's pattern is better

---

## Related Work Packages

- **WP-001/002:** Core documentation (may reference triggers)
- **WP-003:** Migration guide (should mention trigger removal)
- **WP-006:** Example READMEs (now need trigger pattern updates)
- **WP-021:** Code validation (lint script integrates here)

---

## Migration Guide for Users

**If users have existing triggers, provide migration path:**

```markdown
# Migrating from Triggers to Explicit Patterns

## Step 1: Identify Your Triggers
```sql
SELECT * FROM information_schema.triggers
WHERE trigger_schema = 'public';
```

## Step 2: Understand Trigger Purpose
- Audit logging? → Use application-level audit
- Timestamps? → Use explicit updates
- Cascades? → Use ON DELETE CASCADE or app logic
- Validation? → Use CHECK constraints or app validation

## Step 3: Replace with Explicit Pattern
[Examples for each trigger type]

## Step 4: Test Thoroughly
[Testing approach]

## Step 5: Drop Old Trigger
```sql
DROP TRIGGER trigger_name ON table_name;
```
```

---

## Notes

**Why FraiseQL's Two-Layer Pattern Matters:**

1. **AI Code Generation** - AI models can see explicit `log_and_return_mutation()` calls
2. **Code Review** - Developers can trace full data flow: mutation → audit → crypto chain
3. **Debugging** - Business logic is visible, infrastructure is documented
4. **Testing** - Can test audit logging by checking audit_events table
5. **Security** - Crypto chain is tamper-proof (application can't set hash/signature)
6. **Performance** - Explicit patterns easier to profile and optimize
7. **Documentation** - Code IS the documentation (self-documenting)

**Philosophy Alignment:**

FraiseQL's database-first approach means:
- Schema is explicit (views, tables, functions)
- Business logic is explicit (stored functions with visible audit calls)
- Infrastructure is documented (crypto triggers clearly explained)
- Side effects are traceable (no hidden business logic triggers)

**This makes FraiseQL ideal for AI-assisted development where visibility = understandability.**

**Key Distinction:**
- ❌ **Business Logic Triggers** - Hidden, implicit, AI-hostile
- ✅ **Infrastructure Triggers** - Security-critical, well-documented, limited scope
- ✅ **Explicit Audit Calls** - Visible, traceable, AI-friendly

**FraiseQL's Pattern:**
```
Mutation Function → log_and_return_mutation() [EXPLICIT]
                           ↓
                    audit_events INSERT
                           ↓
           populate_crypto_trigger [INFRASTRUCTURE]
                           ↓
              Tamper-proof crypto chain
```

---

**End of WP-030**
