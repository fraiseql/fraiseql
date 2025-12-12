# Work Package: Fix Core Documentation SQL Naming

**Package ID:** WP-001
**Assignee Role:** Technical Writer - Core Docs (TW-CORE)
**Priority:** P0 - Critical
**Estimated Hours:** 8 hours
**Dependencies:** None (blocks WP-002, WP-005, WP-006)

---

## Objective

Fix SQL naming conventions in core documentation files to consistently use `tb_{entity}`, `v_{entity}`, `tv_{entity}` topology instead of simple table names like `users`, `posts`, `comments`.

---

## Scope

### Included
- Fix `core/fraiseql-philosophy.md` - Replace `users` with `tb_user`
- Review all other files in `core/` directory for naming consistency
- Ensure all SQL code blocks use trinity pattern
- Update any explanatory text referencing table names

### Excluded
- Advanced patterns (handled by WP-005)
- Database-specific docs (handled by WP-002)
- Example applications (handled by WP-006)

---

## Deliverables

- [ ] File 1: `docs/core/fraiseql-philosophy.md` - All SQL examples use `tb_user`, `v_user`, `tv_user`
- [ ] File 2: `docs/core/queries-and-mutations.md` - Verify naming consistency (likely already correct)
- [ ] File 3: `docs/core/types.md` - Verify naming consistency
- [ ] File 4: `docs/core/schema-discovery.md` - Verify naming consistency
- [ ] File 5: `docs/core/resolvers.md` - Verify naming consistency
- [ ] New File: `docs/core/trinity-pattern.md` - Create introductory guide to tb_/v_/tv_ pattern (3-5 pages)

---

## Acceptance Criteria

### Must Pass All:

- [ ] **Zero old naming:** No instances of `CREATE TABLE users`, `CREATE TABLE posts`, `CREATE TABLE comments` (except when explicitly teaching migration)
- [ ] **Consistent trinity pattern:** All examples use `tb_user`, `v_user`, `tv_user_with_posts` format
- [ ] **All code examples run:** SQL blocks must be valid PostgreSQL syntax
- [ ] **Links work:** All internal links resolve correctly
- [ ] **Follows style guide:**
  - Active voice
  - Code blocks specify language (```sql)
  - Clear prerequisites at top of each file
  - "Next Steps" section at bottom
- [ ] **Technical accuracy:** Reviewed by ENG-QA (WP-021)
- [ ] **No contradictions:** Does not conflict with other documentation
- [ ] **New trinity-pattern.md includes:**
  - Explanation of why trinity pattern exists
  - Examples of tb_user, v_user, tv_user_with_posts
  - When to use base tables vs views vs computed views
  - Link to detailed database/naming-conventions.md
  - Time estimate: "10 minutes to read"

---

## Resources

### Source Code to Reference
- `/home/lionel/code/fraiseql/examples/blog_simple/db/setup.sql` - **CORRECT** trinity pattern usage
- `/home/lionel/code/fraiseql/src/fraiseql/` - Framework internals (for understanding)

### Existing Docs to Review
- `docs/database/table-naming-conventions.md` - Authoritative naming guide (will be fixed in WP-002)
- `docs/patterns/trinity-identifiers.md` - Trinity pattern explanation (will be moved to database/ in WP-002)

### Related Work Packages
- **Depends on:** None (this is the first critical fix)
- **Blocks:** WP-002 (database docs), WP-005 (advanced patterns), WP-006 (example READMEs)
- **Related:** WP-003 (trinity migration guide)

---

## Implementation Steps

### Step 1: Read and Understand (1 hour)

1. Read `examples/blog_simple/db/setup.sql` to see **correct** trinity pattern usage
2. Read `docs/core/fraiseql-philosophy.md` line 139 to see current **incorrect** usage
3. Read `docs/database/table-naming-conventions.md` to understand authoritative guidance (even though it's contradictory currently)

**Output:** Understanding of what needs to change

---

### Step 2: Fix fraiseql-philosophy.md (2 hours)

**Current problematic section (line 139):**
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT,
    email TEXT
);
```

**Should be:**
```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT,
    email TEXT
);

CREATE VIEW v_user AS
SELECT
    id,
    name,
    email
FROM tb_user;

-- Expose v_user (not tb_user) in GraphQL
```

**Changes:**
- Replace all instances of `users` table with `tb_user`
- Add explanation of why base tables are prefixed with `tb_`
- Add explanation that views (prefixed `v_`) are what GraphQL exposes
- Add link to `core/trinity-pattern.md` (new file you'll create)

**Verification:** Run through ENG-QA (no SQL syntax errors, makes sense)

---

### Step 3: Review Other Core Files (2 hours)

**Files to check:**
- `docs/core/queries-and-mutations.md` - Likely already correct, but verify
- `docs/core/types.md` - Check for any SQL examples
- `docs/core/schema-discovery.md` - Check for any SQL examples
- `docs/core/resolvers.md` - Check for any SQL examples

**For each file:**
1. Search for `CREATE TABLE` statements
2. Verify they use `tb_` prefix
3. Verify views use `v_` or `tv_` prefix
4. If incorrect, fix following same pattern as Step 2

**Expected:** Most files likely don't have SQL examples, but must verify

---

### Step 4: Create trinity-pattern.md (3 hours)

**New file:** `docs/core/trinity-pattern.md`

**Structure:**
```markdown
# Trinity Pattern: Tables, Views, and Computed Views

**Time to read:** 10 minutes
**Prerequisites:** Basic PostgreSQL knowledge

## What is the Trinity Pattern?

The trinity pattern is FraiseQL's naming convention for database objects:
- `tb_{entity}` - Base tables (data storage)
- `v_{entity}` - Views (GraphQL exposure)
- `tv_{entity}` - Computed views (with aggregations/joins)

## Why Use This Pattern?

[Explain benefits: clear separation, migration safety, query optimization]

## Base Tables (`tb_*`)

```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

Base tables store data. They are **NOT** exposed directly to GraphQL.

## Views (`v_*`)

```sql
CREATE VIEW v_user AS
SELECT
    id,
    name,
    email,
    created_at
FROM tb_user
WHERE deleted_at IS NULL;  -- Soft delete filtering
```

Views are what GraphQL exposes. They can filter, transform, or secure data.

## Computed Views (`tv_*`)

```sql
CREATE VIEW tv_user_with_stats AS
SELECT
    u.id,
    u.name,
    u.email,
    COUNT(p.id) as post_count,
    MAX(p.created_at) as last_post_at
FROM tb_user u
LEFT JOIN tb_post p ON p.user_id = u.id
GROUP BY u.id, u.name, u.email;
```

Computed views include aggregations, joins, or expensive computations.

## When to Use Each

| Pattern | Use When |
|---------|----------|
| `tb_*` | Storing data, migrations, admin queries |
| `v_*` | Simple GraphQL exposure with filtering |
| `tv_*` | Complex queries with joins/aggregations |

## Next Steps

- [Full naming conventions guide](../database/naming-conventions.md)
- [Trinity identifier deep-dive](../database/trinity-identifiers.md)
- [Migration from simple tables](../database/migrations.md)
```

**Acceptance:**
- Clear, concise (5-10 pages)
- Examples use correct naming
- Links to related docs
- Time estimate included

---

## Success Metrics

### For This Work Package

- **Files updated:** 5-6 files in `core/` directory
- **New files created:** 1 (`trinity-pattern.md`)
- **SQL naming errors:** 0 (after completion)
- **Quality score:** 5/5 (must be authoritative reference)

### Reader Impact (Persona: Junior Developer)

**Before:** Confused about whether to use `users` or `tb_user`, sees conflicting examples

**After:**
- Understands trinity pattern in <10 minutes
- Uses correct naming in first API
- Has clear reference to link back to

**Success Metric:** Junior Developer persona can explain trinity pattern after reading `trinity-pattern.md`

---

## Risks & Mitigation

### Risk 1: Breaking Existing Links

**Likelihood:** Low (we're updating content, not moving files)
**Impact:** Medium (broken links confuse readers)
**Mitigation:** Check all internal links after changes (use link validator)

### Risk 2: Introducing New Inconsistencies

**Likelihood:** Medium (large number of changes)
**Impact:** High (defeats purpose of work package)
**Mitigation:**
- Use find/replace carefully (review each change)
- Have ENG-QA validate all SQL examples run
- Team Lead (TW-LEAD) reviews before architect approval

### Risk 3: Contradicting Database Docs

**Likelihood:** Medium (database docs not fixed yet)
**Impact:** Medium (confusion between core/ and database/)
**Mitigation:**
- Ensure consistency with how WP-002 will fix database docs
- Coordinate with TW-CORE (you're doing both) on consistent terminology

---

## Checklist Before Submission

### Pre-Review (Self-Check)

- [ ] All SQL examples use `tb_`, `v_`, `tv_` prefixes
- [ ] No instances of `CREATE TABLE users` (except in migration context)
- [ ] All code blocks specify language (```sql)
- [ ] All links work (manually clicked)
- [ ] `trinity-pattern.md` is clear and concise
- [ ] Time estimates included where appropriate
- [ ] "Next Steps" sections added
- [ ] Spell-checked

### Team Lead Review (TW-LEAD)

- [ ] Follows style guide
- [ ] No grammar errors
- [ ] Consistent terminology
- [ ] Quality score: 4/5 or higher

### Architect Review

- [ ] Meets all acceptance criteria
- [ ] No contradictions with architecture blueprint
- [ ] Quality score: 4/5 or higher
- [ ] Ready for merge

---

## Timeline

**Total Hours:** 8 hours

| Task | Hours | Completion |
|------|-------|------------|
| Read and understand | 1 | Day 1 AM |
| Fix philosophy.md | 2 | Day 1 PM |
| Review other core files | 2 | Day 2 AM |
| Create trinity-pattern.md | 3 | Day 2 PM |

**Deadline:** End of Day 2 (Week 1)

---

**End of Work Package WP-001**
