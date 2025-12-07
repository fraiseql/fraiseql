# Work Package: Fix Database Documentation SQL Naming

**Package ID:** WP-002
**Assignee Role:** Technical Writer - Core Docs (TW-CORE)
**Priority:** P0 - Critical
**Estimated Hours:** 8 hours
**Dependencies:** WP-001 (blocks WP-003, WP-005)

---

## Objective

Fix the AUTHORITATIVE database documentation files to clearly recommend `tb_{entity}`, `v_{entity}`, `tv_{entity}` topology and eliminate contradictions that confuse users about which naming pattern to use.

---

## Scope

### Included
- Fix `docs/database/TABLE_NAMING_CONVENTIONS.md` - **CRITICAL:** Make clear recommendation
- Fix `docs/database/DATABASE_LEVEL_CACHING.md` - Replace `users` with `tb_user`
- Fix `docs/database/VIEW_STRATEGIES.md` - Ensure tv_ pattern consistency
- Move `docs/patterns/trinity_identifiers.md` → `docs/database/trinity-identifiers.md`
- Fix moved file to use `tb_product` instead of `products`

### Excluded
- Advanced patterns (handled by WP-005)
- Example applications (handled by WP-006)
- Core conceptual docs (handled by WP-001)

---

## Deliverables

- [ ] File 1: `docs/database/TABLE_NAMING_CONVENTIONS.md` - Clear "Recommended: tb_/v_/tv_" section
- [ ] File 2: `docs/database/DATABASE_LEVEL_CACHING.md` - All `users` → `tb_user`
- [ ] File 3: `docs/database/VIEW_STRATEGIES.md` - Consistent tv_ pattern
- [ ] File 4: `docs/database/trinity-identifiers.md` - Moved from patterns/, fixed examples
- [ ] Updated navigation links (any docs linking to old patterns/trinity_identifiers.md)

---

## Acceptance Criteria

### Must Pass All:

- [ ] **TABLE_NAMING_CONVENTIONS.md has clear recommendation:**
  - New section: "Recommended Pattern for Production" → tb_/v_/tv_
  - Optional section: "Simple Pattern for Prototypes" → users, posts (clearly labeled as NOT recommended for production)
  - Decision tree: "When to use which pattern"
- [ ] **Zero contradictory statements** (no "use tb_user" in one paragraph and "use users" in another)
- [ ] **DATABASE_LEVEL_CACHING.md uses tb_user** (lines 77, 539, 644 fixed)
- [ ] **VIEW_STRATEGIES.md consistent** (all computed views use tv_ prefix)
- [ ] **trinity-identifiers.md moved and fixed:**
  - New location: `docs/database/trinity-identifiers.md`
  - All examples use tb_product, v_product, tv_product_with_stats
  - No references to `products` table (line 57 fixed)
- [ ] **Links updated** (all internal links to trinity_identifiers.md point to new location)
- [ ] **Follows style guide** (active voice, code blocks specify language, time estimates)
- [ ] **No broken links**

---

## Resources

### Source Code to Reference
- `/home/lionel/code/fraiseql/examples/blog_simple/db/setup.sql` - **CORRECT** trinity pattern usage
- `/home/lionel/code/fraiseql/tests/integration/` - Tests use correct naming

### Files to Update
1. `/home/lionel/code/fraiseql/docs/database/TABLE_NAMING_CONVENTIONS.md`
2. `/home/lionel/code/fraiseql/docs/database/DATABASE_LEVEL_CACHING.md`
3. `/home/lionel/code/fraiseql/docs/database/VIEW_STRATEGIES.md`
4. `/home/lionel/code/fraiseql/docs/patterns/trinity_identifiers.md` (move to database/)

### Related Work Packages
- **Depends on:** WP-001 (core docs fixed first for consistency)
- **Blocks:** WP-003 (trinity migration guide), WP-005 (advanced patterns)
- **Related:** WP-001 (core/trinity-pattern.md)

---

## Implementation Steps

### Step 1: Analyze Current Contradictions (1 hour)

**Read current files and identify contradictions:**

1. Open `docs/database/TABLE_NAMING_CONVENTIONS.md`
2. Search for all mentions of "users", "posts", "tb_user", "tb_post"
3. Note where it says "use tb_" vs "use users"
4. Identify the contradictory sections (likely around lines 527, 592-593)

**Expected finding:**
- Some sections recommend tb_ prefix
- Other sections show `users` table examples
- No clear "RECOMMENDED" vs "OPTIONAL" distinction

**Output:** List of contradictory statements to fix

---

### Step 2: Fix TABLE_NAMING_CONVENTIONS.md (3 hours)

**Current problematic pattern:**
File shows both naming styles without clear guidance on which to use when.

**Target structure:**
```markdown
# Database Table Naming Conventions

## Recommended Pattern: Trinity Topology (tb_/v_/tv_)

**Use this for:** Production applications, team projects, long-term codebases

### Base Tables (tb_*)
Base tables store actual data and are NOT exposed directly to GraphQL.

```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Views (v_*)
Views are what GraphQL exposes. They can filter, transform, or secure data.

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

### Computed Views (tv_*)
Computed views include aggregations, joins, or expensive computations.

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

## Alternative Pattern: Simple Tables (PROTOTYPES ONLY)

**Use this for:** Quick prototypes, learning exercises, temporary demos

**⚠️ WARNING:** Not recommended for production. Migrate to trinity pattern before deploying.

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT,
    email TEXT
);
```

**Why not recommended for production:**
- No separation of storage vs. exposure
- Harder to add filtering/transformation later
- Migrations more risky (changing exposed schema)
- No optimization layer (can't add computed views easily)

## Migration Path

If you started with simple tables, see [Migration Guide](migrations.md).

## Decision Tree

```
Are you building a production application?
├─ YES → Use trinity pattern (tb_/v_/tv_)
└─ NO → Is this a temporary prototype?
    ├─ YES → Simple tables OK (but plan to migrate)
    └─ NO → Use trinity pattern (future-proof)
```

## Next Steps

- [Trinity Pattern Deep Dive](trinity-identifiers.md)
- [Migration from Simple to Trinity](migrations.md)
- [View Strategies](VIEW_STRATEGIES.md)
```

**Changes:**
1. Add clear "Recommended" vs "Alternative" sections
2. Add warnings about simple pattern (not for production)
3. Add decision tree
4. Make tb_/v_/tv_ examples prominent
5. Move simple pattern to secondary section with warnings

---

### Step 3: Fix DATABASE_LEVEL_CACHING.md (1.5 hours)

**Files to update:** `docs/database/DATABASE_LEVEL_CACHING.md`

**Current problems:**
- Line 77: Uses `users` table
- Line 539: Uses `users` table
- Line 644: Uses `users` table

**Fix pattern:**

**OLD (line 77):**
```sql
CREATE MATERIALIZED VIEW users_cached AS
SELECT * FROM users;
```

**NEW:**
```sql
CREATE MATERIALIZED VIEW tv_user_cached AS
SELECT * FROM v_user;
```

**OLD (line 539):**
```sql
REFRESH MATERIALIZED VIEW users_cached;
```

**NEW:**
```sql
REFRESH MATERIALIZED VIEW tv_user_cached;
```

**Apply same pattern to all instances.**

**Verification:**
- Search entire file for `users` (should only appear in migration examples)
- All caching examples use tb_/tv_ prefix

---

### Step 4: Fix VIEW_STRATEGIES.md (1 hour)

**File to update:** `docs/database/VIEW_STRATEGIES.md`

**Current problem:** Mentions views but not consistent with tv_ pattern for computed views

**Changes needed:**
1. Ensure all computed view examples use `tv_` prefix
2. Ensure all simple view examples use `v_` prefix
3. Add section explaining when to use `v_` vs `tv_`

**Example addition:**
```markdown
## Naming Convention for Views

### Simple Views (v_*)
Use `v_` prefix for views that:
- Filter rows (WHERE clause)
- Select subset of columns
- Do simple transformations
- No aggregations or joins

### Computed Views (tv_*)
Use `tv_` prefix for views that:
- Include JOIN operations
- Include aggregations (COUNT, SUM, AVG)
- Compute derived values
- Expensive to recalculate
```

---

### Step 5: Move and Fix trinity_identifiers.md (1.5 hours)

**Current location:** `docs/patterns/trinity_identifiers.md`
**New location:** `docs/database/trinity-identifiers.md`

**Steps:**
1. Move file: `mv docs/patterns/trinity_identifiers.md docs/database/trinity-identifiers.md`
2. Fix line 57: Replace `products` with `tb_product`
3. Fix all other examples to use tb_/v_/tv_ pattern
4. Add link from TABLE_NAMING_CONVENTIONS.md to this file
5. Update all internal links pointing to old location

**Current problematic example (line 57):**
```sql
CREATE TABLE products (
    trinity_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    legacy_id INTEGER UNIQUE,
    external_id TEXT UNIQUE
);
```

**Fixed example:**
```sql
CREATE TABLE tb_product (
    trinity_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    legacy_id INTEGER UNIQUE,
    external_id TEXT UNIQUE
);

CREATE VIEW v_product AS
SELECT * FROM tb_product;

-- Expose v_product (not tb_product) to GraphQL
```

**Find and update all links:**
```bash
# Find all files linking to old location
grep -r "patterns/trinity_identifiers" docs/

# Update each file to point to new location:
# patterns/trinity_identifiers.md → database/trinity-identifiers.md
```

---

### Step 6: Update Navigation Links (1 hour)

**Find all broken links:**
```bash
cd /home/lionel/code/fraiseql
grep -r "patterns/trinity_identifiers" docs/
```

**For each file found, update link:**
- Old: `[Trinity Identifiers](../patterns/trinity_identifiers.md)`
- New: `[Trinity Identifiers](../database/trinity-identifiers.md)`

**Common files that may link to it:**
- `docs/core/trinity-pattern.md` (created in WP-001)
- `docs/database/TABLE_NAMING_CONVENTIONS.md`
- `docs/advanced/*.md` files

**Verification:**
```bash
# Should return no results:
grep -r "patterns/trinity_identifiers" docs/
```

---

## Success Metrics

### For This Work Package

- **Files updated:** 4 files (naming conventions, caching, views, trinity identifiers)
- **Files moved:** 1 (trinity_identifiers.md → database/)
- **SQL naming errors:** 0 (after completion)
- **Contradictions:** 0 (clear recommendation in TABLE_NAMING_CONVENTIONS.md)
- **Broken links:** 0 (all navigation updated)
- **Quality score:** 5/5 (authoritative, no room for misinterpretation)

### Reader Impact

**Before:** Users read TABLE_NAMING_CONVENTIONS.md and are confused:
- "Do I use `users` or `tb_user`?"
- "When is each pattern appropriate?"
- "What's the difference?"

**After:** Users read TABLE_NAMING_CONVENTIONS.md and know exactly what to do:
- "Production apps: use tb_/v_/tv_"
- "Prototypes: can use simple tables, but migrate before production"
- Clear decision tree guides choice

**Success Metric:** No user questions about naming conventions (post-release)

---

## Risks & Mitigation

### Risk 1: Breaking External Links

**Likelihood:** Medium (if other projects link to patterns/trinity_identifiers.md)
**Impact:** Medium (broken links for external users)
**Mitigation:**
- Add redirect/note in old location (temporary file explaining move)
- Announce move in release notes
- Keep old file as stub for 6 months with redirect

### Risk 2: Contradictions Still Exist

**Likelihood:** Low (careful review process)
**Impact:** High (defeats purpose of work package)
**Mitigation:**
- Have ENG-QA review for contradictions (WP-022)
- Use automated conflict detection
- Persona review (junior dev should not be confused)

### Risk 3: Too Prescriptive (Alienates Prototype Users)

**Likelihood:** Low
**Impact:** Low (documentation can allow flexibility)
**Mitigation:**
- Keep "Alternative Pattern" section for prototypes
- Frame as "Recommended" vs "Alternative" (not "Right" vs "Wrong")
- Explain WHY trinity is better (not just mandate it)

---

## Checklist Before Submission

### Pre-Review (Self-Check)

- [ ] TABLE_NAMING_CONVENTIONS.md has clear "Recommended" section
- [ ] All SQL examples use `tb_`, `v_`, `tv_` prefixes (except in "Alternative" section)
- [ ] No instances of `CREATE TABLE users` in recommended patterns
- [ ] DATABASE_LEVEL_CACHING.md fixed (lines 77, 539, 644)
- [ ] VIEW_STRATEGIES.md has v_ vs tv_ guidance
- [ ] trinity_identifiers.md moved to database/
- [ ] All examples in trinity_identifiers.md use tb_product
- [ ] All links updated (no broken links to old location)
- [ ] Decision tree added to TABLE_NAMING_CONVENTIONS.md
- [ ] Spell-checked

### Team Lead Review (TW-LEAD)

- [ ] Follows style guide
- [ ] No contradictory statements
- [ ] Clear, authoritative guidance
- [ ] Quality score: 5/5 (this is the source of truth)

### ENG-QA Review

- [ ] No broken links (automated check)
- [ ] SQL examples are valid PostgreSQL
- [ ] No contradictions with WP-001 (core/trinity-pattern.md)

### Architect Review

- [ ] Meets all acceptance criteria
- [ ] Eliminates confusion about naming
- [ ] Quality score: 5/5
- [ ] Ready for merge

---

## Timeline

**Total Hours:** 8 hours

| Task | Hours | Completion |
|------|-------|------------|
| Analyze contradictions | 1 | Day 1 AM |
| Fix TABLE_NAMING_CONVENTIONS.md | 3 | Day 1 PM |
| Fix DATABASE_LEVEL_CACHING.md | 1.5 | Day 2 AM |
| Fix VIEW_STRATEGIES.md | 1 | Day 2 AM |
| Move/fix trinity_identifiers.md | 1.5 | Day 2 PM |
| Update navigation links | 1 | Day 2 PM |

**Deadline:** End of Day 2 (Week 1)
**Dependency:** WP-001 must be complete first

---

**End of Work Package WP-002**
