# Documentation Alignment Execution Plan

**Purpose**: Step-by-step plan for a diligent agent to audit and align FraiseQL documentation according to the Documentation Orchestrator guidelines.

**Target Agent**: Not sophisticated but methodical and detail-oriented.

**Estimated Total Time**: 8-12 hours across 5 phases.

---

## 📋 Pre-Flight Checklist

Before starting, ensure:

- [ ] You have read `docs/DOCUMENTATION_ORCHESTRATOR_PROMPT.md` completely
- [ ] You understand the 4 pillars: ⚡ Rust, 🔒 Security, 🤖 AI-Native, 💰 Cost
- [ ] You know the architecture: `PostgreSQL (JSONB views) → Rust pipeline → HTTP Response`
- [ ] You have access to grep, read, and edit tools
- [ ] You will document all findings as you go

---

## Phase 1: Discovery & Systematic Audit (3-4 hours)

### Goal
Read all documentation files and catalog every inconsistency, inaccuracy, or issue found.

### Step 1.1: Set Up Your Audit Document

**Action**: Create `docs/AUDIT_REPORT.md` to track findings.

**Structure to use**:
```markdown
# FraiseQL Documentation Audit Report

**Audit Date**: [Today's date]
**Auditor**: Claude Agent

## Critical Issues (Fix Immediately)
[List here as you find them]

## High Priority Issues (Fix Soon)
[List here as you find them]

## Medium Priority Issues (Improve)
[List here as you find them]

## Low Priority Issues (Nice to Have)
[List here as you find them]

## Summary Statistics
- Total files audited: 0
- Critical issues: 0
- High priority issues: 0
- Medium priority issues: 0
- Low priority issues: 0
```

### Step 1.2: Run Initial Grep Searches

**Action**: Run these searches and record ALL findings in your audit report.

#### Search 1: Find Old Taglines
```bash
grep -r "fastest Python GraphQL" docs/
```

**What to look for**: Any mention of "fastest Python GraphQL framework" should be flagged.
**Correct message**: "GraphQL for the LLM era. Rust-fast."

#### Search 2: Find Old Cost Claims
```bash
grep -r "month" docs/ | grep -E "\$[0-9]+"
```

**What to look for**: Monthly cost numbers like "$300-3,000/month"
**Correct message**: "$5,400 - $48,000 annual savings"

#### Search 3: Find DataLoader Anti-Pattern
```bash
grep -ri "dataloader\|data.loader\|batch.*load" docs/ examples/
```

**What to look for**: ANY mention of DataLoader
**Critical**: DataLoader breaks the Rust pipeline (PostgreSQL → Python → DataLoader instead of PostgreSQL → Rust)
**Action if found**: Flag as CRITICAL - this is an anti-pattern

#### Search 4: Find Documentation Hygiene Issues
```bash
grep -ri "updated on\|recently added\|NEW:\|EDIT:\|was enhanced" docs/
grep -ri "as of v[0-9]\|new in v[0-9]\|added in v[0-9]" docs/
grep -ri "previously.*now\|we changed\|we rewrote" docs/
```

**What to look for**: Any timestamps, version annotations, or editing traces
**Action if found**: Mark for removal - documentation should read timeless

#### Search 5: Find tv_* Misconceptions (CRITICAL)
```bash
grep -ri "tv_.*GENERATED ALWAYS.*STORED" docs/ examples/
grep -ri "tv_.*auto.*update\|automatic.*sync" docs/
grep -ri "trigger.*sync_tv" docs/
```

**What to look for**: Claims that tv_* tables auto-update
**Critical fact**: tv_* tables require EXPLICIT `fn_sync_tv_*()` calls in mutations
**Common error**: "GENERATED ALWAYS" for cross-table JSONB doesn't work in PostgreSQL

#### Search 6: Find Performance Claims
```bash
grep -r "faster\|blazing\|lightning\|performance" docs/ | grep -v ".pyc"
```

**What to look for**:
- Specific millisecond claims (0.5-2ms) without context
- "2-4x faster than X" without benchmarks
- "Blazing fast" without architectural explanation

**Correct approach**: Explain Rust pipeline architecture, not raw numbers

#### Search 7: Find Architecture Descriptions
```bash
grep -r "PostgreSQL.*JSON\|JSON.*PostgreSQL\|Rust.*pipeline" docs/
```

**What to look for**: Does the flow match "PostgreSQL → Rust → HTTP"?
**Incorrect**: Any path that goes through Python JSON serialization

#### Search 8: Find Trinity Identifier References
```bash
grep -r "pk_\|trinity\|identifier.*slug" docs/
```

**What to look for**: Is the pk_*/id/identifier pattern explained correctly?
**Should mention**:
- `pk_*` = internal integer (never exposed)
- `id` = public UUID (always exposed)
- `identifier` = human-readable slug (optional)

#### Search 9: Find Specialized Where Operator Mentions
```bash
grep -ri "distance_within\|inSubnet\|ancestor_of\|where.*input" docs/
```

**What to look for**: Are specialized operators documented?
**Should cover**:
- Coordinates: `distance_within`
- Network: `inSubnet`, `isPrivate`, `isIPv4`
- LTree: `ancestor_of`, `descendant_of`

#### Search 10: Find Auto-Documentation Mentions
```bash
grep -ri "docstring\|inline.*comment\|field.*description.*automatic" docs/
```

**What to look for**: Is auto-documentation from docstrings/comments mentioned?

### Step 1.3: Read Priority Files Systematically

**Action**: Read each file below IN ORDER and document findings.

For EACH file, check:
1. ✅ **Messaging consistency** - Does it match the 4 pillars?
2. ✅ **Technical accuracy** - Are code examples correct?
3. ✅ **Architecture correctness** - PostgreSQL → Rust → HTTP?
4. ✅ **Security mentions** - Is security by architecture explained?
5. ✅ **Cost claims** - Annual ($5-48K) not monthly?
6. ✅ **Documentation hygiene** - No timestamps/editing traces?
7. ✅ **Anti-patterns absent** - No DataLoader promotion?

#### File 1: README.md
```bash
# Read the file
Read docs/../README.md
```

**Checklist for README.md**:
- [ ] Hero section says "GraphQL for the LLM era. Rust-fast."
- [ ] 4 pillars present: Rust, Security, AI, Cost
- [ ] Security by Architecture section exists
- [ ] Recursion protection explained
- [ ] Cost savings are annual ($5-48K)
- [ ] No unsubstantiated benchmarks
- [ ] Architecture flow is PostgreSQL → Rust → HTTP

**Record findings**: Note any issues in AUDIT_REPORT.md under appropriate severity.

#### File 2: docs/FIRST_HOUR.md
```bash
Read docs/FIRST_HOUR.md
```

**Checklist for FIRST_HOUR.md**:
- [ ] Tutorial uses v1.0.0 API patterns
- [ ] Code examples are complete (imports, setup)
- [ ] Examples use v_* for queries, fn_* for mutations
- [ ] If tv_* mentioned, explicit sync is shown
- [ ] No DataLoader usage
- [ ] Security features demonstrated
- [ ] No timestamps like "Updated on..."
- [ ] Trinity identifiers explained if used

**Record findings**: Document each issue with file:line reference.

#### File 3: docs/UNDERSTANDING.md
```bash
Read docs/UNDERSTANDING.md
```

**Checklist for UNDERSTANDING.md**:
- [ ] Architecture overview matches README
- [ ] CQRS pattern explained (queries=views, mutations=functions)
- [ ] JSONB composition emphasized
- [ ] Rust pipeline mentioned
- [ ] Security advantages explained
- [ ] AI-native positioning clear
- [ ] No ORM references
- [ ] Timeless writing (no "recently" or dates)

**Record findings**: Note discrepancies with README.md.

#### File 4: docs/quickstart.md
```bash
Read docs/quickstart.md
```

**Checklist for quickstart.md**:
- [ ] 5-minute scope maintained
- [ ] Copy-paste code is complete
- [ ] Uses current v1.0.0 API
- [ ] Mentions key differentiators
- [ ] Links to deeper tutorials correctly
- [ ] No outdated installation instructions

**Record findings**: Test if code examples would actually work.

#### File 5: docs/GETTING_STARTED.md
```bash
Read docs/GETTING_STARTED.md
```

**Checklist for GETTING_STARTED.md**:
- [ ] Installation instructions current
- [ ] Prerequisites clear
- [ ] No conflicting setup paths
- [ ] Links to detailed installation work
- [ ] First code example uses best practices

**Record findings**: Flag any conflicting instructions.

#### File 6: docs/core/concepts-glossary.md
```bash
Read docs/core/concepts-glossary.md
```

**Checklist for concepts-glossary.md**:
- [ ] All key terms defined
- [ ] Trinity identifiers explained
- [ ] tv_* (projection tables) correct definition
- [ ] v_* (views) explained
- [ ] fn_* (mutation functions) explained
- [ ] tb_* (base tables) explained
- [ ] APQ explained
- [ ] CQRS pattern defined
- [ ] No incorrect GENERATED ALWAYS claims for tv_*

**Record findings**: This is terminology source of truth.

#### File 7: docs/reference/quick-reference.md
```bash
Read docs/reference/quick-reference.md
```

**Checklist for quick-reference.md**:
- [ ] API examples current
- [ ] Syntax correct
- [ ] Where operators documented
- [ ] Specialized types included (coordinates, network, ltree)
- [ ] Mutation patterns shown
- [ ] Query patterns shown

**Record findings**: This is what developers copy from.

#### File 8: docs/performance/index.md
```bash
Read docs/performance/index.md
```

**Checklist for performance/index.md**:
- [ ] Performance claims factual
- [ ] Architecture-based explanations (not just numbers)
- [ ] Rust pipeline advantage explained
- [ ] JSONB benefits clear
- [ ] No N+1 query problems mentioned
- [ ] tv_* performance trade-offs explained
- [ ] Benchmarks removed or contextualized

**Record findings**: Must align with README performance messaging.

#### File 9-11: docs/diagrams/*.md
```bash
Read docs/diagrams/request-flow.md
Read docs/diagrams/cqrs-pattern.md
Read docs/diagrams/apq-cache-flow.md
```

**Checklist for diagrams**:
- [ ] Visual flows match text descriptions
- [ ] PostgreSQL → Rust → HTTP shown correctly
- [ ] No Python JSON serialization in main path
- [ ] CQRS pattern clear (v_*/tv_* → reads, fn_* → writes)
- [ ] APQ flow accurate

**Record findings**: Diagrams must not contradict text.

#### File 12+: Scan Remaining Documentation
```bash
# List all remaining docs
Glob pattern: "docs/**/*.md"
```

**For each file**, quickly scan for:
1. Old taglines
2. Monthly cost claims
3. DataLoader mentions
4. Timestamps/editing traces
5. Incorrect tv_* patterns
6. Outdated API usage

**Record findings**: Prioritize based on severity.

### Step 1.4: Check Test Coverage for Documented Features

**Action**: Verify that documented features actually have tests.

```bash
# Trinity identifiers
Glob pattern: "tests/**/test_trinity*.py"
Glob pattern: "tests/**/test_*trinity*.py"

# Projection tables (tv_*)
Grep pattern: "tv_" in path: "tests/"

# APQ
Glob pattern: "tests/**/test_apq*.py"

# Where operators
Glob pattern: "tests/**/test_*where*.py"
Grep pattern: "distance_within|inSubnet|ancestor_of" in path: "tests/"

# Connections
Glob pattern: "tests/**/test_*connection*.py"
```

**For each documented feature**:
- ✅ If tests exist: Note test coverage is good
- ⚠️ If no tests found: Flag feature as "needs verification"
- 📝 Document in audit report

### Step 1.5: Check Examples Directory

**Action**: Verify example code follows best practices.

```bash
# List all examples
Glob pattern: "examples/**/*.py"
Glob pattern: "examples/**/*.sql"
```

**For each example**:
- [ ] Uses v1.0.0 API
- [ ] No DataLoader usage
- [ ] Follows naming conventions (v_*, fn_*, tv_*, tb_*)
- [ ] If tv_* used, shows explicit sync
- [ ] Security patterns demonstrated
- [ ] Complete and runnable

**Record findings**: Examples teach by demonstration.

### Step 1.6: Compile Audit Report Summary

**Action**: Update the Summary Statistics section in AUDIT_REPORT.md.

Count issues by severity:
- Critical: Technical errors, security misrepresentation, anti-patterns
- High: Messaging conflicts, outdated claims, missing key features
- Medium: Polish needs, better examples, cross-reference issues
- Low: Typos, formatting, minor improvements

**Example summary**:
```markdown
## Summary Statistics
- Total files audited: 25
- Critical issues: 3 (DataLoader in examples, incorrect tv_* pattern, wrong architecture)
- High priority issues: 8 (old taglines, monthly costs, missing security)
- Medium priority issues: 12 (hygiene issues, better examples needed)
- Low priority issues: 5 (typos, formatting)
```

---

## Phase 2: Prioritization & Planning (1 hour)

### Goal
Create a prioritized task list with dependencies and effort estimates.

### Step 2.1: Create Alignment Plan Document

**Action**: Create `docs/ALIGNMENT_PLAN.md`.

**Template**:
```markdown
# Documentation Alignment Plan

**Based on**: AUDIT_REPORT.md findings
**Total Issues**: [Count from audit]

## Critical Tasks (Do First - Blocks Everything)

### Task C1: [Description]
**Files affected**: [List]
**Issue**: [What's wrong]
**Fix**: [What to do]
**Estimated effort**: [30min/1hr/2hr]
**Dependencies**: None (critical path)

## High Priority Tasks (Do Next)

### Task H1: [Description]
**Files affected**: [List]
**Issue**: [What's wrong]
**Fix**: [What to do]
**Estimated effort**: [Time]
**Dependencies**: [C1, C2, etc.]

## Medium Priority Tasks (Improvements)

### Task M1: [Description]
**Files affected**: [List]
**Issue**: [What's wrong]
**Fix**: [What to do]
**Estimated effort**: [Time]
**Dependencies**: [Previous tasks]

## Low Priority Tasks (Polish)

### Task L1: [Description]
**Files affected**: [List]
**Issue**: [What's wrong]
**Fix**: [What to do]
**Estimated effort**: [Time]
**Dependencies**: None

## Execution Order

1. Phase 2A: Critical tasks (C1, C2, C3) - Sequential
2. Phase 2B: High priority tasks (H1-H5) - Can parallelize some
3. Phase 2C: Medium priority tasks (M1-M8) - Can parallelize
4. Phase 2D: Low priority tasks (L1-L5) - Final polish

**Total estimated effort**: [Sum up time estimates]
```

### Step 2.2: Categorize Each Issue from Audit

**Action**: Go through your AUDIT_REPORT.md and assign each issue to a task.

**Decision tree for severity**:

```
Issue found
├─ Does it break functionality or teach wrong pattern?
│  └─ YES → CRITICAL
│      Examples: DataLoader usage, incorrect tv_* pattern, wrong architecture
├─ Does it conflict with README messaging or mislead users?
│  └─ YES → HIGH
│      Examples: Old taglines, monthly costs, missing security, outdated API
├─ Does it need improvement for clarity/completeness?
│  └─ YES → MEDIUM
│      Examples: Hygiene issues, better examples, cross-references
└─ Is it just polish?
   └─ YES → LOW
       Examples: Typos, formatting, minor wording
```

### Step 2.3: Identify Dependencies

**Action**: Mark which tasks must happen before others.

**Common dependencies**:
- Fix README first (it's the source of truth)
- Fix core concepts-glossary second (terminology reference)
- Then fix tutorials (FIRST_HOUR, UNDERSTANDING)
- Then fix examples (they reference tutorials)
- Finally polish (when content is correct)

**Example**:
```markdown
### Task H2: Align FIRST_HOUR with README messaging
**Dependencies**: C1 (Fix README inconsistencies first)
```

### Step 2.4: Estimate Effort

**Action**: Add time estimates to each task.

**Estimation guide**:
- Simple find/replace: 15-30 minutes
- Rewriting a section: 30-60 minutes
- Creating new examples: 1-2 hours
- Full file restructure: 2-3 hours

**Be realistic**: Include time for:
- Reading the file
- Making changes
- Re-reading to verify
- Running tests if code changes

### Step 2.5: Create Execution Checklist

**Action**: Add a checklist section at the end of ALIGNMENT_PLAN.md.

```markdown
## Execution Progress Tracker

### Critical Tasks
- [ ] C1: [Description]
- [ ] C2: [Description]
- [ ] C3: [Description]

### High Priority Tasks
- [ ] H1: [Description]
- [ ] H2: [Description]
...

### Medium Priority Tasks
- [ ] M1: [Description]
...

### Low Priority Tasks
- [ ] L1: [Description]
...

## Completion Criteria
- [ ] All critical tasks completed
- [ ] All high priority tasks completed
- [ ] Medium priority tasks at 80%+
- [ ] Low priority tasks attempted (if time allows)
- [ ] All changes tested
- [ ] Cross-references verified
- [ ] Git commits are clean with good messages
```

---

## Phase 3: Execute Critical Fixes (2-3 hours)

### Goal
Fix all CRITICAL issues that block other work or teach dangerous patterns.

### Step 3.1: Start with Critical Task C1

**Action**: Open your ALIGNMENT_PLAN.md and start with the first critical task.

**For EACH critical task, follow this workflow**:

#### 1. Read the File
```bash
Read [file_path]
```

#### 2. Locate the Problem
- Find the exact lines mentioned in your audit
- Understand the context around the issue
- Verify the problem is what you documented

#### 3. Determine the Fix
- Reference DOCUMENTATION_ORCHESTRATOR_PROMPT.md for correct patterns
- Check README.md for correct messaging
- Look at concepts-glossary.md for terminology

#### 4. Make the Change
```bash
Edit file_path
old_string: [exact text to replace]
new_string: [corrected text]
```

#### 5. Verify the Change
```bash
Read [file_path] (re-read the section you changed)
```

- [ ] Did the edit apply correctly?
- [ ] Does the new text make sense in context?
- [ ] Are there other instances of the same issue nearby?

#### 6. Check for Ripple Effects
```bash
grep -r "[key term from your fix]" docs/
```

- Does the same issue appear in other files?
- If yes, add them to your task list

#### 7. Mark Task Complete
Update ALIGNMENT_PLAN.md:
```markdown
- [x] C1: [Description] ✅ Completed [timestamp]
```

### Step 3.2: Common Critical Fixes

Here are the most likely critical issues and how to fix them:

#### Critical Fix Pattern A: Remove DataLoader References

**If you find**:
```python
# DataLoader example
from strawberry.dataloader import DataLoader

async def load_users(keys):
    return await User.objects.filter(id__in=keys)

user_loader = DataLoader(load_fn=load_users)
```

**Replace with**:
```python
# Use JSONB views instead (preserves Rust pipeline)
# PostgreSQL view composes data:
# CREATE VIEW v_user AS
# SELECT id, jsonb_build_object(
#     'id', id,
#     'name', name,
#     'posts', (SELECT jsonb_agg(data) FROM v_post WHERE user_id = tb_user.id)
# ) AS data
# FROM tb_user;

# Python resolver just queries the view:
def resolve_user(id: UUID) -> User:
    return User.from_view(id)  # Single query, complete data
```

**Why**: DataLoader breaks "PostgreSQL → Rust → HTTP" by forcing "PostgreSQL → Python objects → DataLoader → Python JSON".

#### Critical Fix Pattern B: Correct tv_* Pattern

**If you find**:
```sql
-- WRONG: This doesn't work!
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB GENERATED ALWAYS AS (
        SELECT data FROM v_user WHERE v_user.id = tv_user.id
    ) STORED
);
```

**Replace with**:
```sql
-- CORRECT: Regular table + explicit sync
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,  -- Regular column, NOT generated
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sync function (explicit)
CREATE FUNCTION fn_sync_tv_user(p_id UUID) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user WHERE id = p_id
    ON CONFLICT (id) DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Mutations call sync explicitly
CREATE FUNCTION fn_update_user(...) RETURNS JSONB AS $$
DECLARE v_user_id UUID;
BEGIN
    UPDATE tb_user SET ... WHERE id = p_id RETURNING id INTO v_user_id;
    PERFORM fn_sync_tv_user(v_user_id);  -- ← CRITICAL STEP
    RETURN (SELECT data FROM tv_user WHERE id = v_user_id);
END;
$$ LANGUAGE plpgsql;
```

**Why**: PostgreSQL GENERATED columns can't reference other tables. The explicit sync pattern is required.

#### Critical Fix Pattern C: Fix Architecture Description

**If you find**:
```markdown
FraiseQL uses Python to serialize JSON responses for optimal performance.
```

**Replace with**:
```markdown
FraiseQL uses a Rust pipeline: PostgreSQL returns JSONB → Rust transforms it → HTTP response. Python never touches the JSON, eliminating serialization overhead (7-10x faster).
```

**Why**: The Rust pipeline is the key architectural differentiator.

### Step 3.3: Test After Each Critical Fix

**Action**: If your fix touched code examples, verify they would work.

**For SQL examples**:
- Check syntax is valid PostgreSQL
- Verify function signatures make sense
- Ensure RETURNS types match

**For Python examples**:
- Check imports would work
- Verify API matches v1.0.0
- Ensure types are correct

**For architecture descriptions**:
- Cross-check with README.md
- Verify flow matches: PostgreSQL → Rust → HTTP

### Step 3.4: Document Critical Fixes Completed

**Action**: Update AUDIT_REPORT.md with a "Fixes Applied" section.

```markdown
## Fixes Applied

### Critical Fixes (Phase 3)
- [x] **C1**: Removed DataLoader example from examples/social_network.py
  - Why: Broke Rust pipeline
  - Replaced with: JSONB view pattern
  - Files changed: examples/social_network.py, examples/README.md

- [x] **C2**: Corrected tv_* pattern in docs/core/concepts-glossary.md
  - Why: GENERATED ALWAYS doesn't work for cross-table JSONB
  - Replaced with: Explicit sync function pattern
  - Files changed: docs/core/concepts-glossary.md, docs/reference/quick-reference.md
```

---

## Phase 4: Execute High Priority Fixes (2-3 hours)

### Goal
Align all messaging with README, fix outdated content, ensure consistency.

### Step 4.1: Messaging Alignment Tasks

**Common high priority tasks**:

#### High Priority Task: Update Taglines

**Search for old taglines**:
```bash
grep -r "fastest Python GraphQL" docs/
```

**For each occurrence**:
- Read the file
- Replace with: "GraphQL for the LLM era. Rust-fast." OR "FraiseQL is a Python GraphQL framework built for the LLM era"
- Ensure context makes sense

#### High Priority Task: Update Cost Claims

**Search for monthly costs**:
```bash
grep -r "\$300-3,000/month\|\$5-400/month" docs/
```

**Replace with**:
- "$5,400 - $48,000 annual savings"
- "Replace Redis, Sentry, and APM with PostgreSQL"
- "70% fewer services to deploy and monitor"

#### High Priority Task: Add Missing Security Content

**Check each tutorial/guide for security mentions**:

**If security is missing, add a section**:
```markdown
## Security by Architecture

FraiseQL uses **explicit field whitelisting** through JSONB views:

- Views define exactly what data can be queried
- No accidental field exposure (common ORM problem)
- Database enforces security boundary, not just application code
- Recursion protection built into view structure (no middleware needed)

Example:
```sql
-- Only expose safe fields in view
CREATE VIEW v_user_public AS
SELECT id, jsonb_build_object(
    'id', id,
    'name', name,
    'avatar', avatar_url
    -- email, password_hash, etc. NOT exposed
) AS data
FROM tb_user;
```
```

#### High Priority Task: Clean Up Documentation Hygiene

**Search for editing traces**:
```bash
grep -ri "updated on\|recently added\|NEW:\|EDIT:" docs/
```

**For each occurrence**:
- Read the file
- Remove the timestamp/annotation
- Rewrite to be timeless

**Example fixes**:

❌ Before:
```markdown
## Auto-Documentation (NEW in v1.0)

Recently added: Field descriptions are now automatically extracted from docstrings.

Updated on: October 24, 2025
```

✅ After:
```markdown
## Auto-Documentation

Field descriptions are automatically extracted from docstrings, inline comments, and type annotations:

```python
class User:
    id: UUID  # Public identifier
    name: Annotated[str, "User's full name"]
```
```

#### High Priority Task: Ensure API Version Consistency

**Check for v0.x patterns**:
```bash
grep -ri "v0\.\|version 0\.\|deprecated" docs/
```

**For each occurrence**:
- If showing migration: Keep in dedicated migration docs only
- If showing current API: Update to v1.0.0 patterns
- Remove "new in v1.0" language (just document current state)

### Step 4.2: Add Missing Core Feature Documentation

**Check if these features are explained in tutorials**:

**Feature checklist**:
- [ ] Trinity identifiers (pk_*/id/identifier)
- [ ] Projection tables (tv_*) with explicit sync
- [ ] Auto-documentation from docstrings
- [ ] Specialized where operators (coordinates, network, ltree)
- [ ] APQ with dual backends
- [ ] Security by architecture
- [ ] Recursion protection
- [ ] Zero N+1 queries via JSONB

**For each missing feature**:
1. Check concepts-glossary.md has definition
2. Add explanation to FIRST_HOUR or UNDERSTANDING
3. Add example to quick-reference.md
4. Verify test coverage exists

### Step 4.3: Fix Cross-References

**Check all markdown links**:
```bash
grep -r "\[.*\](.*\.md)" docs/
```

**For each link**:
- Verify target file exists
- Check anchor links work (e.g., #section-name)
- Update broken links

**Common patterns**:
- `[Getting Started](./GETTING_STARTED.md)` → Verify path
- `[Concepts](../core/concepts-glossary.md#trinity-identifiers)` → Check anchor exists

### Step 4.4: Update Examples to Best Practices

**For each example in examples/ directory**:

**Best practices checklist**:
- [ ] Uses v1.0.0 API
- [ ] Follows naming: v_* (views), fn_* (functions), tv_* (projections), tb_* (tables)
- [ ] No DataLoader usage
- [ ] Security patterns demonstrated
- [ ] If tv_* used, explicit sync shown
- [ ] Trinity identifiers used correctly
- [ ] Comments explain why, not just what
- [ ] Complete imports and setup
- [ ] README explains what example demonstrates

**Template for example README**:
```markdown
# [Example Name]

**Demonstrates**: [Key features shown]
**Concepts**: [Link to relevant docs]
**Difficulty**: [Beginner/Intermediate/Advanced]

## What This Shows

- Feature 1: [Brief explanation]
- Feature 2: [Brief explanation]

## Key Files

- `schema.py` - [What it contains]
- `database.sql` - [What it defines]
- `queries.graphql` - [Example queries]

## Run This Example

\```bash
# Setup
uv venv
source .venv/bin/activate
uv pip install -r requirements.txt

# Initialize database
psql < database.sql

# Run
python app.py
\```

## Key Patterns

### Pattern 1: [Name]
[Code snippet with explanation]

### Pattern 2: [Name]
[Code snippet with explanation]
```

### Step 4.5: Verify Performance Messaging Consistency

**Check all performance claims**:
```bash
grep -ri "faster\|performance\|speed\|millisecond\|ms" docs/
```

**Approved messages**:
✅ "Rust pipeline provides 7-10x faster JSON processing vs Python"
✅ "PostgreSQL → Rust → HTTP eliminates Python serialization overhead"
✅ "Architectural efficiency through JSONB passthrough"
✅ "Zero N+1 query problems through JSONB composition"

**Prohibited messages**:
❌ "0.5-2ms response times" (without architectural context)
❌ "2-4x faster than Framework X" (no benchmarks available)
❌ "Blazing fast" (meaningless superlative)

**If you find prohibited claims**:
- Replace with architectural explanation
- Focus on "how" not "how much"

---

## Phase 5: Polish & Verification (1-2 hours)

### Goal
Final improvements, consistency checks, and comprehensive verification.

### Step 5.1: Create Documentation Style Guide

**Action**: Create `docs/DOCUMENTATION_STYLE_GUIDE.md` based on your work.

**Template**:
```markdown
# FraiseQL Documentation Style Guide

**Purpose**: Ensure consistency across all documentation.

## Messaging Standards

### Tagline
✅ "GraphQL for the LLM era. Rust-fast."

### The 4 Pillars (Always in This Order)
1. ⚡ **Fastest** - Rust pipeline for compiled performance
2. 🔒 **Safest** - Explicit field contracts, view-enforced security
3. 🤖 **Smartest** - Built for AI/LLM era, clear SQL context
4. 💰 **Cheapest** - PostgreSQL-native everything, $5-48K/year savings

### Architecture Messaging
✅ "PostgreSQL (JSONB views) → Rust pipeline → HTTP response"
✅ "Database-first CQRS: queries use views (v_*, tv_*), mutations use functions (fn_*)"
✅ "Zero Python JSON serialization overhead"

[... continue with all approved messaging patterns ...]

## Code Standards

### SQL Examples
- Always use explicit naming: v_* (views), fn_* (functions), tv_* (projection tables), tb_* (base tables)
- Show complete function definitions
- Include RETURNS types
- Comment complex logic

### Python Examples
- Include all imports
- Use type hints
- Follow PEP 8
- Comment architectural decisions

### GraphQL Examples
- Show complete queries
- Use descriptive variable names
- Include operation names

## Terminology

[Import from concepts-glossary.md]

## Cross-References

- Link to concepts when first mentioned
- Use relative paths: `../core/concepts-glossary.md#term`
- Verify links work

## Documentation Hygiene

❌ Never include:
- Timestamps ("Updated on...")
- Version annotations ("New in v1.0")
- Editing traces ("Recently added")
- Meta-commentary ("This section was enhanced...")

✅ Always:
- Write timelessly
- Present as authoritative reference
- Focus on teaching, not history
```

### Step 5.2: Run Final Consistency Checks

**Action**: Re-run all your Phase 1 grep searches to verify fixes.

```bash
# Should find ZERO results now:
grep -r "fastest Python GraphQL" docs/
grep -r "\$[0-9]+-[0-9]+/month" docs/
grep -ri "dataloader" docs/ examples/
grep -ri "updated on\|NEW:\|EDIT:" docs/
grep -ri "tv_.*GENERATED ALWAYS.*STORED" docs/
```

**For each search**:
- ✅ If zero results: Mark verified
- ⚠️ If results found: Investigate
  - Is it a new issue you missed?
  - Is it a valid use case?
  - Document decision

### Step 5.3: Verify All Code Examples

**Action**: Check that code examples would actually work.

**For SQL examples**:
```sql
-- Run through mental checklist:
-- 1. Syntax valid?
-- 2. Functions have RETURNS?
-- 3. Types consistent?
-- 4. Would this work in PostgreSQL 14+?
```

**For Python examples**:
```python
# Mental checklist:
# 1. All imports present?
# 2. Types make sense?
# 3. API matches v1.0.0?
# 4. No deprecated patterns?
```

**For GraphQL examples**:
```graphql
# Mental checklist:
# 1. Correct syntax?
# 2. Fields exist in schema?
# 3. Variables typed correctly?
```

**If example won't work**:
- Fix it or remove it
- Don't leave broken examples

### Step 5.4: Verify Cross-References

**Action**: Test all internal links.

**Process**:
1. Find all markdown links:
   ```bash
   grep -r "\[.*\](.*\.md)" docs/
   ```

2. For each link, verify:
   - File exists at that path
   - Anchor exists if specified (#section)
   - Link makes sense in context

3. Fix broken links:
   - Update paths
   - Update anchors
   - Remove if target no longer relevant

### Step 5.5: Check Navigation Flow

**Action**: Verify learning path makes sense.

**User journey**:
1. README.md → First impression, value prop
2. quickstart.md → 5-minute quick win
3. GETTING_STARTED.md → Full setup
4. FIRST_HOUR.md → Deep hands-on tutorial
5. UNDERSTANDING.md → Conceptual model
6. reference/quick-reference.md → Copy-paste reference
7. Specialized guides → Deep dives

**Verify**:
- [ ] Each step links to the next
- [ ] No conflicting instructions
- [ ] Progressive complexity (simple → advanced)
- [ ] No dead ends

### Step 5.6: Medium Priority Cleanup

**If time allows**, tackle medium priority tasks:

**Common medium tasks**:
- Improve examples with better comments
- Add more cross-references
- Expand explanations
- Add diagrams
- Better formatting

**Don't spend too much time** - These are improvements, not fixes.

### Step 5.7: Create Final Summary

**Action**: Update AUDIT_REPORT.md with final summary.

**Add section**:
```markdown
## Final Summary

**Audit completed**: [Date]
**Fixes completed**: [Date]

### Work Completed

#### Critical Issues: [X/X completed]
- All DataLoader references removed
- tv_* pattern corrected
- Architecture descriptions fixed
- [List all critical fixes]

#### High Priority Issues: [X/X completed]
- Taglines updated
- Cost claims standardized
- Documentation hygiene cleaned
- [List all high priority fixes]

#### Medium Priority Issues: [X/Y completed]
- [List completed medium tasks]

#### Low Priority Issues: [X/Y completed]
- [List completed low tasks]

### Improvements Made

**Quantitative**:
- Files updated: [Count]
- Lines changed: [Estimate]
- Broken links fixed: [Count]
- Examples corrected: [Count]

**Qualitative**:
- Messaging now consistent with 4 pillars
- All architecture descriptions accurate
- Security positioning strengthened
- Anti-patterns removed
- Documentation hygiene pristine

### Verification

- [x] All grep searches return zero problematic results
- [x] Code examples reviewed for correctness
- [x] Cross-references verified
- [x] Navigation flow logical
- [x] Style guide created

### Remaining Work (Optional/Future)

[List any medium/low priority items you didn't get to]

### Recommendations

[Any suggestions for future documentation improvements]
```

---

## Phase 6: Git Commit Strategy

### Goal
Create clean, well-documented commits for all changes.

### Step 6.1: Review Git Status

**Action**: Check what files you've modified.

```bash
git status
```

### Step 6.2: Group Changes by Type

**Don't commit everything at once!** Group related changes.

**Suggested commit groups**:

#### Commit 1: Critical fixes
```bash
git add docs/core/concepts-glossary.md
git add examples/social_network.py
git commit -m "docs: fix critical anti-patterns and technical inaccuracies

- Remove DataLoader examples (breaks Rust pipeline)
- Correct tv_* projection table pattern (requires explicit sync)
- Fix architecture descriptions (PostgreSQL → Rust → HTTP)

These were critical fixes preventing users from understanding or correctly implementing core FraiseQL patterns."
```

#### Commit 2: Messaging alignment
```bash
git add README.md
git add docs/FIRST_HOUR.md
git add docs/UNDERSTANDING.md
git add docs/quickstart.md
git commit -m "docs: align messaging with 4 pillars across all guides

- Update tagline to 'GraphQL for the LLM era. Rust-fast.'
- Standardize cost claims to annual savings ($5-48K/year)
- Emphasize security by architecture consistently
- Highlight AI-native positioning throughout

All documentation now presents unified value proposition."
```

#### Commit 3: Documentation hygiene
```bash
git add docs/**/*.md
git commit -m "docs: clean up documentation hygiene issues

- Remove timestamps and 'Updated on' annotations
- Remove 'NEW:' and version-specific language
- Eliminate editing process meta-commentary
- Rewrite content to be timeless and authoritative

Documentation now reads as professional reference material."
```

#### Commit 4: Examples and code
```bash
git add examples/**/*
git commit -m "docs: update examples to v1.0.0 best practices

- Fix naming conventions (v_*, fn_*, tv_*, tb_*)
- Add explicit tv_* sync patterns
- Demonstrate security features
- Ensure all examples are complete and runnable

All examples now teach correct patterns."
```

#### Commit 5: Cross-references and navigation
```bash
git add docs/**/*.md
git commit -m "docs: fix cross-references and improve navigation

- Update broken markdown links
- Add missing cross-references
- Improve learning path flow
- Fix anchor links

Users can now navigate documentation smoothly."
```

#### Commit 6: Style guide and audit reports
```bash
git add docs/AUDIT_REPORT.md
git add docs/ALIGNMENT_PLAN.md
git add docs/DOCUMENTATION_STYLE_GUIDE.md
git commit -m "docs: add documentation audit reports and style guide

- AUDIT_REPORT.md: Complete findings from documentation review
- ALIGNMENT_PLAN.md: Prioritized fix plan with dependencies
- DOCUMENTATION_STYLE_GUIDE.md: Standards for future docs

These meta-documents help maintain documentation quality going forward."
```

### Step 6.3: Verify Commits

**Action**: Review your commits before pushing.

```bash
git log --oneline -6
```

**Check**:
- [ ] Commit messages are clear
- [ ] Changes are grouped logically
- [ ] No accidental files included

---

## Post-Completion Checklist

### Documentation Quality

- [ ] All critical issues fixed
- [ ] All high priority issues fixed
- [ ] Medium priority at 80%+ completion
- [ ] README.md is source of truth
- [ ] Messaging consistent (4 pillars everywhere)
- [ ] Architecture descriptions accurate
- [ ] Code examples work
- [ ] Cross-references valid
- [ ] Navigation flow logical

### Technical Accuracy

- [ ] No DataLoader anti-patterns
- [ ] tv_* pattern correct (explicit sync)
- [ ] Trinity identifiers explained
- [ ] Performance claims factual
- [ ] Security features highlighted
- [ ] Test coverage verified

### Documentation Hygiene

- [ ] No timestamps or dates
- [ ] No "NEW:" or version annotations
- [ ] No editing process traces
- [ ] Timeless, authoritative tone
- [ ] Professional formatting

### Deliverables Complete

- [ ] AUDIT_REPORT.md created
- [ ] ALIGNMENT_PLAN.md created
- [ ] DOCUMENTATION_STYLE_GUIDE.md created
- [ ] All priority fixes implemented
- [ ] Git commits are clean
- [ ] Changes documented

---

## Troubleshooting Common Issues

### Issue: Too Many Findings to Track

**Solution**: Focus on critical first.
- Don't try to fix everything at once
- Complete critical phase before moving on
- Use ALIGNMENT_PLAN.md to track progress

### Issue: Conflicting Information Between Documents

**Solution**: README.md is source of truth.
- Always defer to README.md for messaging
- If README is wrong, flag for review
- Update all other docs to match README

### Issue: Don't Know the Correct Pattern

**Solution**: Check these in order:
1. DOCUMENTATION_ORCHESTRATOR_PROMPT.md (this document)
2. README.md (messaging)
3. docs/core/concepts-glossary.md (terminology)
4. tests/ directory (working examples)
5. Ask user if still unclear

### Issue: Change Affects Many Files

**Solution**: Make a tracking list.
- Use grep to find all occurrences
- Create a subtask for each file
- Fix systematically
- Verify with grep after

### Issue: Example Code Might Not Work

**Solution**: Check test directory.
- Look for tests of the same feature
- Copy patterns from tests
- If no tests exist, flag as "needs verification"
- Don't claim it works without evidence

### Issue: Running Out of Time

**Solution**: Prioritize ruthlessly.
- Critical must be done
- High priority should be done
- Medium priority is optional
- Low priority can wait
- Document what's left in AUDIT_REPORT

---

## Success Criteria

You've succeeded when:

✅ **Users can trust the documentation** - No technical errors or misleading claims
✅ **Messaging is unified** - 4 pillars consistent everywhere
✅ **Examples teach correctly** - No anti-patterns promoted
✅ **Navigation is clear** - Users know where to go
✅ **Documentation is professional** - Timeless, authoritative, clean
✅ **Future maintenance is easier** - Style guide exists, patterns clear

---

## Notes for the Diligent Agent

### Your Strengths
- You follow instructions carefully
- You document as you go
- You verify your work
- You don't skip steps

### How to Use This Plan
1. **Read each phase completely before starting**
2. **Follow steps in order** - Don't jump ahead
3. **Document everything** - Use audit report and alignment plan
4. **Verify each change** - Re-read after editing
5. **Ask when uncertain** - Better to clarify than guess

### Time Management
- Phase 1 (Audit): 3-4 hours - Don't rush this
- Phase 2 (Planning): 1 hour - Organization pays off
- Phase 3 (Critical): 2-3 hours - Must be thorough
- Phase 4 (High Priority): 2-3 hours - Most volume here
- Phase 5 (Polish): 1-2 hours - Optional improvements
- Total: 9-13 hours - Break into multiple sessions if needed

### When to Take Breaks
- After completing each phase
- After fixing 5-7 issues
- When feeling overwhelmed
- Before making big structural changes

### Quality Over Speed
- Correct is better than fast
- One thorough pass is better than three quick ones
- Test your changes
- Don't leave issues half-fixed

---

## Quick Reference: Key Commands

### Search Commands
```bash
# Find old taglines
grep -r "fastest Python GraphQL" docs/

# Find cost claims
grep -r "month" docs/ | grep -E "\$[0-9]+"

# Find DataLoader
grep -ri "dataloader" docs/ examples/

# Find hygiene issues
grep -ri "updated on\|NEW:\|EDIT:" docs/

# Find tv_* issues
grep -ri "tv_.*GENERATED ALWAYS" docs/

# Find performance claims
grep -ri "faster\|millisecond" docs/

# Find all markdown files
find docs/ -name "*.md"
```

### Verification Commands
```bash
# Check git status
git status

# See what changed
git diff docs/FIRST_HOUR.md

# Count issues by type
grep -c "CRITICAL" docs/AUDIT_REPORT.md
```

### Progress Tracking
```bash
# View your audit report
cat docs/AUDIT_REPORT.md

# View your plan
cat docs/ALIGNMENT_PLAN.md

# Check progress
git log --oneline
```

---

**END OF EXECUTION PLAN**

Good luck! You've got this. Follow the plan, document your work, and you'll produce high-quality, consistent documentation that makes FraiseQL shine. 🚀
