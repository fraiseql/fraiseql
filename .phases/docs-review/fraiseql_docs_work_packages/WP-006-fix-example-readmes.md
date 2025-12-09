# Work Package: Fix Example Application READMEs

**Package ID:** WP-006
**Assignee Role:** Technical Writer - API/Examples (TW-API)
**Priority:** P0 - Critical
**Estimated Hours:** 4 hours
**Dependencies:** WP-001

---

## Objective

Fix contradiction where example READMEs use old naming (`users`, `posts`) but actual SQL files use trinity pattern (`tb_user`, `tb_post`).

---

## Files to Update

1. **`examples/blog_simple/README.md`** (lines 80-129)
   - Update schema documentation to use `tb_user`, `tb_post`, `tb_comment`
   - Add section explaining trinity pattern
   - Ensure README matches actual `db/setup.sql`

2. **`examples/mutations_demo/README.md`** (line 72)
   - Replace `users` → `tb_user`

---

## Acceptance Criteria

- [ ] READMEs match SQL files (no contradictions)
- [ ] Trinity pattern explained in context
- [ ] Links to `docs/core/trinity-pattern.md`
- [ ] No confusion between documentation and code

---

## Implementation Steps

### Step 1: Verify Actual SQL Schema (1 hour)

**Read actual SQL files to understand current state:**
```bash
# Check blog_simple schema
cat examples/blog_simple/db/setup.sql | grep -E "CREATE TABLE|CREATE VIEW"

# Check mutations_demo schema
cat examples/mutations_demo/v2_init.sql | grep -E "CREATE TABLE|CREATE VIEW"
```

**Document findings:**
- What table names are actually used in SQL?
- What view names are used?
- Are there any v_* or tv_* views defined?

---

### Step 2: Update READMEs to Match (2 hours)

**For blog_simple/README.md:**

**OLD (incorrect):**
```markdown
## Schema

Tables:
- users (id, name, email)
- posts (id, user_id, title, content)
- comments (id, post_id, user_id, content)
```

**NEW (correct):**
```markdown
## Schema

This example uses FraiseQL's [Trinity Pattern](../../../docs/core/trinity-pattern.md) for database design.

**Base Tables** (data storage):
- `tb_user` - User accounts
- `tb_post` - Blog posts
- `tb_comment` - Post comments

**Views** (GraphQL API layer):
- `v_user` - Exposed user data
- `v_post` - Exposed post data
- `v_comment` - Exposed comment data

**Note:** GraphQL queries use view names (`v_user`), mutations use base tables (`tb_user`).
```

**For mutations_demo/README.md:**

Replace all instances of simple table names with trinity pattern equivalents.

---

### Step 3: Add Trinity Pattern Explanation (1 hour)

Add to each README:

```markdown
### Why Trinity Pattern?

This example uses FraiseQL's recommended trinity pattern:
- **Base tables** (`tb_*`) - Store raw data, handle mutations
- **Views** (`v_*`) - Expose filtered data to GraphQL
- **Computed views** (`tv_*`) - Pre-join expensive queries

Learn more: [Trinity Pattern Guide](../../../docs/core/trinity-pattern.md)
```

---

## 🤖 Local Model Execution Instructions

**This work package is SUITABLE for local 8B models** (search & replace with context awareness)

**Execution Strategy:**
1. **Preparation** (Claude does this - 15 minutes)
2. **Transformation** (Local model executes - 30 minutes)
3. **Verification** (Claude checks - 15 minutes)

**Total time:** ~60 minutes (vs 4 hours manual)

---

### Step 1: Preparation (Claude)

**Verify target files exist:**
```bash
cd /home/lionel/code/fraiseql

# Check README files
ls -la examples/blog_simple/README.md
ls -la examples/mutations_demo/README.md

# Check SQL files to understand actual schema
cat examples/blog_simple/db/setup.sql | grep -E "CREATE TABLE|CREATE VIEW"
cat examples/mutations_demo/v2_init.sql | grep -E "CREATE TABLE|CREATE VIEW"
```

**Count instances to fix:**
```bash
# blog_simple/README.md
grep -n "users\|posts\|comments" examples/blog_simple/README.md | grep -v "tb_\|v_\|tv_"

# mutations_demo/README.md
grep -n "users" examples/mutations_demo/README.md | grep -v "tb_user"
```

**Document actual schema:**
Take note of what table/view names are ACTUALLY used in SQL files, so README can match.

---

### Step 2: Transformation Patterns (Local Model)

**Pattern 1: Update schema documentation in blog_simple/README.md**

```
Location: examples/blog_simple/README.md (lines 80-129 approximately)

Search for section starting with:
## Schema

Tables:
- users
- posts
- comments

Replace entire section with:
## Schema

This example uses FraiseQL's [Trinity Pattern](../../../docs/core/trinity-pattern.md) for database design.

**Base Tables** (data storage):
- `tb_user` - User accounts (id, name, email, created_at)
- `tb_post` - Blog posts (id, user_id, title, content, created_at)
- `tb_comment` - Post comments (id, post_id, user_id, content, created_at)

**Views** (GraphQL API layer):
- `v_user` - Exposed user data
- `v_post` - Exposed post data
- `v_comment` - Exposed comment data

**Foreign Keys:**
- `tb_post.user_id` → `tb_user.id`
- `tb_comment.post_id` → `tb_post.id`
- `tb_comment.user_id` → `tb_user.id`

**Note:** GraphQL queries use view names (`v_user`, `v_post`), mutations use base tables (`tb_user`, `tb_post`).

### Why Trinity Pattern?

This example uses FraiseQL's recommended trinity pattern:
- **Base tables** (`tb_*`) - Store raw data, handle mutations
- **Views** (`v_*`) - Expose filtered data to GraphQL
- **Computed views** (`tv_*`) - Pre-join expensive queries (optional)

Learn more: [Trinity Pattern Guide](../../../docs/core/trinity-pattern.md)
```

**Pattern 2: Fix mutations_demo/README.md**

```
File: examples/mutations_demo/README.md

Search:  users
Replace: tb_user

Search:  posts
Replace: tb_post

Search:  comments
Replace: tb_comment

(But preserve phrases like "allows users to" - only replace table references)
```

**Pattern 3: Update code examples in READMEs**

```
In SQL code blocks:

Search:  CREATE TABLE users
Replace: CREATE TABLE tb_user

Search:  CREATE TABLE posts
Replace: CREATE TABLE tb_post

Search:  CREATE TABLE comments
Replace: CREATE TABLE tb_comment

Search:  FROM users
Replace: FROM v_user

Search:  FROM posts
Replace: FROM v_post

Search:  INSERT INTO users
Replace: INSERT INTO tb_user

Search:  INSERT INTO posts
Replace: INSERT INTO tb_post
```

---

### Step 3: Execution Commands (Local Model)

**Option A: Using Edit tool (recommended for README changes)**

Due to the structural nature of README changes (replacing entire sections), use Edit tool with clear context:

```python
# Pseudocode for local model
Edit(
    file="examples/blog_simple/README.md",
    old_string="## Schema\n\nTables:\n- users (id, name, email)\n- posts (...)\n- comments (...)",
    new_string="## Schema\n\nThis example uses FraiseQL's [Trinity Pattern](...)\n..."
)
```

**Option B: Using sed for simple replacements**

```bash
cd /home/lionel/code/fraiseql

# mutations_demo/README.md (simpler - just search/replace)
sed -i 's/\btable users\b/table tb_user/g' examples/mutations_demo/README.md
sed -i 's/\`users\`/`tb_user`/g' examples/mutations_demo/README.md
sed -i 's/CREATE TABLE users/CREATE TABLE tb_user/g' examples/mutations_demo/README.md
sed -i 's/FROM users\b/FROM v_user/g' examples/mutations_demo/README.md
```

**Important:** Be careful not to replace "users" in phrases like "allows users to create posts"!

---

### Step 4: Verification (Claude)

**Check READMEs match SQL files:**
```bash
# Extract schema from SQL
echo "=== blog_simple SQL schema ==="
grep "CREATE TABLE\|CREATE VIEW" examples/blog_simple/db/setup.sql

# Extract schema from README
echo "=== blog_simple README schema ==="
grep -A 20 "## Schema" examples/blog_simple/README.md | head -30

# They should match!
```

**Count remaining issues:**
```bash
# Should find 0 instances of incorrect table references
grep -n "\`users\`\|\`posts\`\|\`comments\`" examples/*/README.md | grep -v "tb_\|v_\|allows users\|for users"
```

**Manual spot-check (Claude reviews):**
- [ ] READMEs use `tb_*` for base tables
- [ ] READMEs mention `v_*` views (if they exist in SQL)
- [ ] Trinity pattern section added
- [ ] Link to trinity-pattern.md included
- [ ] No contradictions between README and SQL files

---

### Step 5: Quality Check (Claude)

**Acceptance criteria check:**
- [ ] READMEs match SQL files (no contradictions)
- [ ] Trinity pattern explained in context
- [ ] Links to `docs/core/trinity-pattern.md` work
- [ ] No confusion between documentation and code
- [ ] Preserved correct usage of "users" in prose (not table names)

**If issues found:**
- Small fixes (1-2 instances): Claude fixes directly
- Large issues: Re-run local model with corrected pattern

---

## Success Metrics

**For local model execution:**
- **Pattern success rate:** >90% (context-aware replacements are trickier)
- **Manual fixes needed:** <10 instances
- **Time savings:** 60 minutes vs 4 hours (75% faster)
- **Cost savings:** $0.00 vs ~$1-2 (Claude tokens)

**Quality:**
- READMEs now accurately reflect actual SQL schema
- Users no longer confused by documentation vs code mismatch
- Trinity pattern clearly explained in examples

---

**Deadline:** End of Week 1

**End of Work Package WP-006**
