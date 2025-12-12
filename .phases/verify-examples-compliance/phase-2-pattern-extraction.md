# Phase 2: Pattern Extraction - Build Verification Rules

## Objective

Extract and formalize all pattern rules from documentation, code, and tests into executable verification logic.

## Context

Phase 1 discovered all examples and documentation. Now we need to:
1. Extract precise pattern rules from documentation
2. Identify "golden examples" from code/tests
3. Build verification rules that can detect violations
4. Create rule priority/severity levels

## Files to Modify/Create

### Read-Only (Analysis)
- `~/.claude/skills/printoptim-database-patterns.md` - Trinity pattern reference
- `docs/core/concepts-glossary.md` - JSONB view patterns
- `docs/README.md` - FraiseQL architecture
- `examples/blog_api/**/*.sql` - Golden example (well-tested)
- `tests/integration/**/*.py` - Test expectations

### Create
- `.phases/verify-examples-compliance/rules.yaml` - Verification rules
- `.phases/verify-examples-compliance/golden-patterns.md` - Reference patterns
- `.phases/verify-examples-compliance/sql-parser.py` - SQL parsing utilities

## Implementation Steps

### Step 1: Define Trinity Pattern Rules

Extract from `printoptim-database-patterns.md` and `concepts-glossary.md`:

**Rule TR-001: Table Must Have Primary Key pk_***
```yaml
id: TR-001
name: "Trinity: INTEGER Primary Key"
description: "Every table must have pk_<entity> INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY"
severity: ERROR
applies_to: ["CREATE TABLE"]
verification:
  method: regex
  pattern: |
    pk_\w+\s+INTEGER\s+GENERATED\s+(ALWAYS|BY\s+DEFAULT)\s+AS\s+IDENTITY\s+PRIMARY\s+KEY
  negative_pattern: |
    # Must NOT use SERIAL (deprecated pattern)
    pk_\w+\s+SERIAL
example_pass: |
  CREATE TABLE tb_user (
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ...
  );
example_fail: |
  CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,  -- ❌ Use INTEGER GENERATED ALWAYS
    ...
  );
```

**Rule TR-002: Table Must Have UUID id Column**
```yaml
id: TR-002
name: "Trinity: UUID Public Identifier"
description: "Every table must have 'id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE'"
severity: ERROR
applies_to: ["CREATE TABLE"]
verification:
  method: regex
  pattern: |
    \bid\s+UUID\s+(DEFAULT\s+gen_random_uuid\(\))?\s*.*?UNIQUE
  requires: ["id", "UUID", "UNIQUE"]
example_pass: |
  CREATE TABLE tb_post (
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    ...
  );
example_fail: |
  CREATE TABLE tb_post (
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID,  -- ❌ Missing DEFAULT gen_random_uuid() and UNIQUE
    ...
  );
```

**Rule TR-003: Table May Have identifier Column**
```yaml
id: TR-003
name: "Trinity: TEXT Identifier (Optional)"
description: "Tables MAY have 'identifier TEXT UNIQUE' for human-readable slugs"
severity: INFO
applies_to: ["CREATE TABLE"]
verification:
  method: regex
  pattern: |
    identifier\s+TEXT.*?UNIQUE
  optional: true
example_pass: |
  CREATE TABLE tb_post (
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    identifier TEXT UNIQUE,  -- ✅ Optional but recommended
    ...
  );
```

### Step 2: Define JSONB View Rules

**Rule VW-001: View Must Have id Column**
```yaml
id: VW-001
name: "View: Must Expose id Column"
description: "Views must SELECT id column (not in JSONB) for WHERE filtering"
severity: ERROR
applies_to: ["CREATE VIEW", "CREATE OR REPLACE VIEW"]
verification:
  method: sql_parse
  check: |
    SELECT clause must contain 'id' as direct column (not in JSONB)
example_pass: |
  CREATE VIEW v_user AS
  SELECT
    id,  -- ✅ Direct column for WHERE filtering
    jsonb_build_object(
      'id', id,
      'name', name
    ) as data
  FROM tb_user;
example_fail: |
  CREATE VIEW v_user AS
  SELECT
    jsonb_build_object(
      'id', id,  -- ❌ id must be direct column, not only in JSONB
      'name', name
    ) as data
  FROM tb_user;
```

**Rule VW-002: View pk_* Only If Referenced**
```yaml
id: VW-002
name: "View: Include pk_* Only If Referenced"
description: "Views should include pk_<entity> column ONLY if other views JOIN to it"
severity: WARNING
applies_to: ["CREATE VIEW"]
verification:
  method: dependency_analysis
  check: |
    - Find all views that JOIN to this view
    - If any JOIN uses pk_<entity>, it must be in SELECT
    - If no JOIN uses pk_<entity>, it should NOT be in SELECT
example_pass: |
  -- v_post is referenced by v_comment (which JOINs on pk_post)
  CREATE VIEW v_post AS
  SELECT
    id,       -- For WHERE filtering
    pk_post,  -- ✅ For parent views to JOIN
    jsonb_build_object(...) as data
  FROM tb_post;

  -- v_user is not referenced by other views
  CREATE VIEW v_user AS
  SELECT
    id,  -- Only id needed
    jsonb_build_object(...) as data
  FROM tb_user;
  -- ✅ No pk_user because no other views JOIN to this
```

**Rule VW-003: JSONB Must NOT Contain pk_***
```yaml
id: VW-003
name: "JSONB: Never Expose pk_* Fields"
description: "jsonb_build_object() must NEVER include pk_<entity> fields (internal only)"
severity: ERROR
applies_to: ["CREATE VIEW"]
verification:
  method: regex
  pattern: |
    jsonb_build_object\([^)]*'pk_\w+'
  negative_match: true  # This pattern should NOT be found
example_pass: |
  CREATE VIEW v_user AS
  SELECT
    id,
    pk_user,  -- ✅ OK as direct column (for JOINs)
    jsonb_build_object(
      'id', id,          -- ✅ Public UUID
      'name', name
      -- pk_user NOT here ✅
    ) as data
  FROM tb_user;
example_fail: |
  CREATE VIEW v_user AS
  SELECT
    id,
    jsonb_build_object(
      'id', id,
      'pk_user', pk_user,  -- ❌ NEVER expose pk_* in JSONB!
      'name', name
    ) as data
  FROM tb_user;
```

### Step 3: Define Foreign Key Rules

**Rule FK-001: Foreign Keys Must Reference pk_***
```yaml
id: FK-001
name: "FK: Must Reference INTEGER pk_*"
description: "FOREIGN KEY constraints must reference pk_<entity> columns, not id (UUID)"
severity: ERROR
applies_to: ["CREATE TABLE"]
verification:
  method: regex
  pattern: |
    REFERENCES\s+\w+\s*\(\s*pk_\w+\s*\)
example_pass: |
  CREATE TABLE tb_post (
    fk_user INTEGER REFERENCES tb_user(pk_user),  -- ✅ References pk_user
    ...
  );
example_fail: |
  CREATE TABLE tb_post (
    fk_user UUID REFERENCES tb_user(id),  -- ❌ Must reference pk_user, not id
    ...
  );
```

**Rule FK-002: FK Column Must Be INTEGER**
```yaml
id: FK-002
name: "FK: Column Must Be INTEGER Type"
description: "Foreign key columns must be INTEGER (matching pk_*), not UUID"
severity: ERROR
applies_to: ["CREATE TABLE"]
verification:
  method: sql_parse
  check: |
    - Find all FK columns (fk_*)
    - Each must be INTEGER type
    - Each REFERENCES clause must point to pk_* column
example_pass: |
  CREATE TABLE tb_post (
    fk_user INTEGER REFERENCES tb_user(pk_user),  -- ✅ INTEGER → INTEGER
    ...
  );
example_fail: |
  CREATE TABLE tb_post (
    fk_user UUID REFERENCES tb_user(id),  -- ❌ UUID FK (inefficient, wrong pattern)
    ...
  );
```

### Step 4: Define Helper Function Rules

**Rule HF-001: Helper Function Naming**
```yaml
id: HF-001
name: "Helper: Correct Naming Convention"
description: "Helper functions must follow: core.get_pk_<entity>() or core.get_<entity>_id()"
severity: ERROR
applies_to: ["CREATE FUNCTION"]
verification:
  method: function_signature
  patterns:
    - "core\\.get_pk_\\w+\\(.*\\)"
    - "core\\.get_\\w+_id\\(.*\\)"
example_pass: |
  CREATE FUNCTION core.get_pk_user(p_tenant_id UUID, p_user_id UUID)
  RETURNS INTEGER ...

  CREATE FUNCTION core.get_user_id(p_tenant_id UUID, p_pk_user INTEGER)
  RETURNS UUID ...
example_fail: |
  CREATE FUNCTION core.get_user_pk(...)  -- ❌ Should be get_pk_user
  CREATE FUNCTION core.fetch_user_id(...)  -- ❌ Should be get_user_id
```

**Rule HF-002: Variable Naming in Functions**
```yaml
id: HF-002
name: "Variables: Follow Naming Convention"
description: |
  Function variables must follow patterns:
  - v_<entity>_pk INTEGER (resolved pk)
  - v_<entity>_id UUID (resolved id)
  - p_<entity>_id UUID (parameter)
  - p_<entity>_ids UUID[] (parameter array)
severity: WARNING
applies_to: ["CREATE FUNCTION"]
verification:
  method: variable_analysis
  patterns:
    parameter: "p_\\w+_ids?\\s+(UUID|INTEGER|TEXT)"
    variable_pk: "v_\\w+_pks?\\s+INTEGER"
    variable_id: "v_\\w+_ids?\\s+UUID"
example_pass: |
  CREATE FUNCTION app.fn_create_post(p_input_data JSONB) ...
  DECLARE
    v_user_id UUID;
    v_user_pk INTEGER;
    v_post_id UUID;
  BEGIN
    v_user_id := (p_input_data->>'user_id')::UUID;
    v_user_pk := core.get_pk_user(v_tenant_id, v_user_id);
    ...
example_fail: |
  DECLARE
    user_id_pk INTEGER;  -- ❌ Should be v_user_pk
    userId UUID;          -- ❌ Should be v_user_id (snake_case)
```

### Step 5: Define Mutation Function Rules

**Rule MF-001: Mutations Must Return JSONB**
```yaml
id: MF-001
name: "Mutation: Return JSONB Structure"
description: "Mutation functions must RETURNS JSONB with success/error structure"
severity: ERROR
applies_to: ["CREATE FUNCTION fn_*", "CREATE FUNCTION app.fn_*"]
verification:
  method: function_signature
  check: |
    RETURNS JSONB
example_pass: |
  CREATE FUNCTION fn_create_user(...) RETURNS JSONB AS $$
  BEGIN
    RETURN jsonb_build_object(
      'success', true,
      'user_id', v_user_id
    );
  END;
  $$ LANGUAGE plpgsql;
```

**Rule MF-002: Explicit tv_* Sync Calls**
```yaml
id: MF-002
name: "Mutation: Explicit Sync for tv_* Tables"
description: "Mutations modifying data must explicitly call fn_sync_tv_<entity>() functions"
severity: ERROR
applies_to: ["CREATE FUNCTION fn_*"]
verification:
  method: function_body_analysis
  check: |
    - If function modifies tb_* table
    - And corresponding tv_* table exists
    - Must contain PERFORM fn_sync_tv_<entity>() call
example_pass: |
  CREATE FUNCTION fn_create_user(...) RETURNS JSONB AS $$
  BEGIN
    INSERT INTO tb_user (...) VALUES (...);
    PERFORM fn_sync_tv_user();  -- ✅ Explicit sync
    RETURN ...;
  END;
  $$;
example_fail: |
  CREATE FUNCTION fn_create_user(...) RETURNS JSONB AS $$
  BEGIN
    INSERT INTO tb_user (...) VALUES (...);
    -- ❌ Missing PERFORM fn_sync_tv_user() call!
    RETURN ...;
  END;
  $$;
```

### Step 6: Build Golden Pattern Reference

Extract actual patterns from `examples/blog_api/` (well-tested):

**Golden Pattern: Complete Trinity Table**
```sql
-- From: examples/blog_api/db/0_schema/01_write/011_tb_user.sql
CREATE TABLE tb_user (
    -- Trinity Pattern (all 3 identifiers)
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    identifier TEXT UNIQUE NOT NULL,

    -- Business fields
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    bio TEXT,
    avatar_url TEXT,
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],

    -- Audit fields (standard pattern)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Golden Pattern: Complete JSONB View**
```sql
-- From: examples/blog_api/db/0_schema/02_read/021_user/0211_v_user.sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    u.id,  -- ✅ Direct column for WHERE filtering
    jsonb_build_object(
        'id', u.id::text,              -- ✅ Public UUID
        'identifier', u.identifier,     -- ✅ Human-readable
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'avatar_url', u.avatar_url,
        'is_active', u.is_active,
        'roles', u.roles,
        'created_at', u.created_at,
        'updated_at', u.updated_at
        -- ✅ No pk_user in JSONB
    ) AS data
FROM tb_user u;
```

**Golden Pattern: View Referenced by Others**
```sql
-- From: examples/blog_api/db/0_schema/02_read/022_post/0221_v_post.sql
CREATE OR REPLACE VIEW v_post AS
SELECT
    p.id,       -- ✅ For WHERE filtering
    p.pk_post,  -- ✅ Included because v_comment JOINs to this
    jsonb_build_object(
        'id', p.id::text,
        'identifier', p.identifier,
        'title', p.title,
        ...
        'author', vu.data  -- ✅ Nested JSONB from v_user
    ) AS data
FROM tb_post p
JOIN tb_user u ON u.pk_user = p.fk_user  -- ✅ JOIN on INTEGER pk_user
JOIN v_user vu ON vu.id = u.id;
```

## Verification Commands

### Extract Pattern Rules from Documentation
```bash
# Count pattern examples in concepts-glossary.md
grep -c "example_pass\|example_fail\|CREATE TABLE\|CREATE VIEW" \
  docs/core/concepts-glossary.md
# Expected: 20-40 examples
```

### Validate Golden Examples
```bash
# Check blog_api follows all patterns
cd examples/blog_api/db/

# Verify all tables have Trinity pattern
grep -c "pk_\w\+ INTEGER GENERATED" 0_schema/01_write/*.sql
# Expected: 3 (tb_user, tb_post, tb_comment)

# Verify all views have id column
grep -c "SELECT.*id," 0_schema/02_read/*/*.sql
# Expected: 3 (v_user, v_post, v_comment)
```

### Test Rule Execution
```bash
# Test TR-001 rule on good example
grep "pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY" \
  examples/blog_api/db/0_schema/01_write/011_tb_user.sql
# Should match

# Test VW-003 rule on good example (should NOT match)
grep "jsonb_build_object.*'pk_" \
  examples/blog_api/db/0_schema/02_read/021_user/0211_v_user.sql
# Should return empty (no match = good)
```

## Expected Output

### rules.yaml
Complete rule definitions (40-60 rules) covering:
- Trinity pattern rules (10-15 rules)
- JSONB view rules (10-15 rules)
- Foreign key rules (5-10 rules)
- Helper function rules (5-10 rules)
- Mutation function rules (5-10 rules)
- Python type exposure rules (5-10 rules)

### golden-patterns.md
Reference document with:
- Complete working examples from blog_api
- Annotated code showing correct patterns
- Common mistakes and how to avoid them
- Copy-pasteable templates

### sql-parser.py
Python utilities for:
- Parsing SQL files into AST
- Extracting table/view/function definitions
- Analyzing JSONB structure
- Detecting pattern violations

## Acceptance Criteria

- [ ] All rules defined with examples (40+ rules)
- [ ] Golden patterns extracted from blog_api
- [ ] SQL parser built and tested
- [ ] Rules validated against blog_api (100% pass)
- [ ] Rule priority/severity assigned
- [ ] Ready for Phase 3 (Automated Verification)

## DO NOT

- ❌ Do NOT create rules without examples
- ❌ Do NOT assume patterns (verify from docs/code)
- ❌ Do NOT make rules too strict (allow flexibility)
- ❌ Do NOT skip severity levels (ERROR vs WARNING matters)
