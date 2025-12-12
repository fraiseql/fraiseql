# Phase 4: Manual Review - Edge Cases and Complex Patterns

## Objective

Manually review verification results to identify:
1. False positives (automated checks flagged correct code)
2. Edge cases not covered by automated rules
3. Complex patterns requiring human judgment
4. Documentation/code mismatches

## Context

Phase 3 automated verification provides 80-90% coverage. This phase handles:
- Examples with valid deviations from patterns
- Legacy examples with different conventions
- Complex multi-file patterns
- Documentation accuracy verification

## Files to Modify/Create

### Read-Only (Review)
- `.phases/verify-examples-compliance/compliance-report.md` - Automated findings
- Examples flagged with violations
- Documentation SQL examples

### Create
- `.phases/verify-examples-compliance/manual-review-findings.md` - Human judgment
- `.phases/verify-examples-compliance/false-positives.yaml` - Exceptions to rules
- `.phases/verify-examples-compliance/edge-cases.md` - Patterns needing special handling

## Implementation Steps

### Step 1: Review High-Severity Violations

Prioritize ERROR-level violations from automated report:

**Review Process:**
```bash
# Get all ERROR violations
python .phases/verify-examples-compliance/verify.py --severity ERROR --format json > errors.json

# Group by rule ID
jq 'group_by(.rule_id) | map({rule: .[0].rule_id, count: length, examples: map(.entity_name)})' errors.json
```

**For each ERROR:**
1. Read the flagged code
2. Check if violation is accurate
3. Determine if:
   - True positive → Document for remediation (Phase 5)
   - False positive → Add to exceptions list
   - Edge case → Document pattern variation

### Step 2: Verify Documentation Examples Match Code

Compare SQL examples in docs to actual implementations:

**Documentation Review Checklist:**

**`docs/core/concepts-glossary.md`:**
- [ ] Lines 75-97: Trinity Table example matches `blog_api/db/0_schema/01_write/011_tb_user.sql`
- [ ] Lines 176-214: JSONB View example matches `blog_api/db/0_schema/02_read/021_user/0211_v_user.sql`
- [ ] Lines 298-330: Projection Table (tv_*) example is executable
- [ ] Lines 586-621: Mutation function example follows actual pattern

**`README.md`:**
- [ ] Lines 485-498: v_user view example matches actual code
- [ ] Lines 519-555: fn_publish_post example is executable
- [ ] Lines 734-762: Table View (tv_*) sync pattern matches code

**`~/.claude/skills/printoptim-database-patterns.md`:**
- [ ] Lines 130-161: Color mode trinity example follows FraiseQL patterns
- [ ] Lines 383-449: Function example matches FraiseQL conventions
- [ ] Lines 604-665: Materialized view example translates to FraiseQL

**Verification Method:**
```bash
# Extract SQL from docs
awk '/```sql/,/```/' docs/core/concepts-glossary.md | grep -v '```' > /tmp/doc_examples.sql

# Try to execute (syntax check)
psql -d test_db --dry-run -f /tmp/doc_examples.sql

# Compare to actual code
diff -u /tmp/doc_examples.sql examples/blog_api/db/0_schema/01_write/011_tb_user.sql
```

### Step 3: Review Edge Cases

Identify valid pattern variations:

**Edge Case 1: Simple Tables Without Trinity**
Some examples may not need full Trinity pattern:

```sql
-- Valid for lookup/reference tables
CREATE TABLE tb_status (
    pk_status SMALLINT PRIMARY KEY,  -- Small enum, no UUID needed
    name TEXT UNIQUE NOT NULL
);
```

**Decision:** Document as acceptable exception for:
- Enum/lookup tables with <100 rows
- Internal-only tables never exposed via GraphQL
- Migration/ETL staging tables

**Edge Case 2: Views Without identifier**
Some entities don't need human-readable slugs:

```sql
-- Valid for transactional data
CREATE TABLE tb_reading (
    pk_reading INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    -- No identifier (readings don't need slugs)
    reading_at TIMESTAMPTZ,
    value NUMERIC
);
```

**Decision:** identifier is optional, downgrade rule TR-003 to INFO level

**Edge Case 3: Legacy Examples**
Older examples may use different patterns:

```bash
# Identify old examples
find examples/ -name "*.sql" -exec grep -l "SERIAL PRIMARY KEY" {} \;
# Examples with SERIAL instead of INTEGER GENERATED should be flagged for update
```

### Step 4: Test Documentation Examples

Execute all SQL examples from documentation:

```bash
# Setup test database
createdb fraiseql_doc_test

# Test each example
psql -d fraiseql_doc_test <<SQL
-- From docs/core/concepts-glossary.md:75-97
CREATE TABLE tb_user (
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    identifier TEXT UNIQUE,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL
);

-- Verify structure
\d tb_user

-- Test insert
INSERT INTO tb_user (name, email, identifier)
VALUES ('Test User', 'test@example.com', 'test-user');

-- Verify trinity works
SELECT pk_user, id, identifier FROM tb_user;
SQL
```

**Expected Result:**
```
 pk_user |                  id                  | identifier
---------+--------------------------------------+-----------
       1 | 550e8400-e29b-41d4-a716-446655440000 | test-user
```

### Step 5: Verify Python Type Definitions

Check that Python types match JSONB view structures:

**Review `examples/blog_api/app.py`:**
```python
@fraiseql.type(sql_source="v_user", jsonb_column="data")
class User:
    id: UUID          # ✅ Matches v_user.data->>'id'
    identifier: str   # ✅ Matches v_user.data->>'identifier'
    email: str        # ✅ Matches v_user.data->>'email'
    name: str         # ✅ Matches v_user.data->>'name'
    # pk_user NOT here ✅ (internal only)
```

**Verification:**
```sql
-- Extract JSONB keys from v_user
SELECT jsonb_object_keys(data) FROM v_user LIMIT 1;

-- Should return: id, identifier, email, name, bio, avatar_url, is_active, roles, created_at, updated_at
```

Compare with Python type fields → Must match exactly

### Step 6: Document Valid Pattern Variations

Create exceptions file for legitimate deviations:

```yaml
# false-positives.yaml
exceptions:
  - rule_id: TR-003
    entity: tb_status
    reason: "Enum/lookup table, no identifier needed"
    permanent: true

  - rule_id: VW-002
    entity: v_category
    reason: "Included pk_category for hierarchical queries (ltree)"
    permanent: true
    note: "Hierarchical tables need pk_* for path construction"

  - rule_id: MF-002
    entity: fn_delete_user
    reason: "Delete operations don't need tv_* sync (CASCADE handles it)"
    permanent: true

legacy_examples:
  - example: examples/simple_blog
    status: deprecated
    reason: "Old example before Trinity pattern standardized"
    action: "Update in Phase 5 or mark as legacy"
```

## Verification Commands

### Find False Positives
```bash
# Examples with errors but actually correct
python .phases/verify-examples-compliance/verify.py examples/blog_api/ --verbose | grep ERROR

# Manual check each error
```

### Test All Doc Examples
```bash
# Extract and test all SQL examples
./test-doc-examples.sh docs/core/concepts-glossary.md
./test-doc-examples.sh README.md
```

### Verify Python/SQL Alignment
```bash
# For each example
python check_python_sql_alignment.py examples/blog_api/
```

## Expected Output

### manual-review-findings.md
```markdown
# Manual Review Findings

## False Positives (12 total)

### TR-003: identifier Optional
- **Examples**: tb_reading, tb_meter, tb_audit_log
- **Decision**: Downgrade to INFO level
- **Reason**: Not all entities need human-readable slugs

### VW-002: pk_* in Hierarchical Views
- **Examples**: v_category (ltree), v_location (ltree)
- **Decision**: Add exception for hierarchical tables
- **Reason**: ltree path construction requires pk_*

## Documentation Issues (5 total)

### concepts-glossary.md:330
- **Issue**: tv_user sync example shows trigger, but code uses explicit PERFORM
- **Fix**: Update docs to show explicit sync pattern
- **Severity**: Minor

### README.md:520-555
- **Issue**: fn_publish_post example returns success/error but actual code returns JSONB
- **Fix**: Update example to match actual mutation pattern
- **Severity**: Major

## Edge Cases (8 total)

### Enum Tables
- **Pattern**: Small lookup tables (tb_status, tb_role)
- **Trinity**: Only pk_* + name, no UUID
- **Decision**: Document as acceptable exception

## Recommendations

1. Update 3 documentation examples to match current code
2. Add 5 pattern exceptions to verification rules
3. Mark 2 legacy examples for update or deprecation
4. Create "Common Patterns" guide from edge cases
```

## Acceptance Criteria

- [ ] All ERROR violations manually reviewed (100%)
- [ ] False positives documented (10-20 expected)
- [ ] Documentation examples tested for accuracy
- [ ] Python/SQL alignment verified
- [ ] Edge cases categorized and documented
- [ ] Exceptions list created
- [ ] Ready for Phase 5 (Remediation)

## DO NOT

- ❌ Do NOT skip violations (review ALL)
- ❌ Do NOT assume code is wrong (docs may be outdated)
- ❌ Do NOT create new patterns without justification
- ❌ Do NOT fix issues yet (document for Phase 5)
