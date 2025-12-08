# FraiseQL Documentation Improvement - Execution Guide

**Status:** Ready to Execute
**Work Packages Created:** 20 of 25 (P0 critical packages complete)
**Estimated Time:** 4 weeks with team of 7

---

## Quick Start

### Option 1: Use Local AI Model (BigPickle/vLLM)

For work packages that involve writing/editing (suitable for local model execution):

```bash
# Check model status
vllm-switch status

# For each work package, you can delegate to local model:
# 1. Read the work package
# 2. Extract the specific file edits needed
# 3. Create prompts for local model to execute changes
```

**Good for local model:**
- WP-001: Fix core docs naming (search & replace SQL examples)
- WP-002: Fix database docs naming (search & replace)
- WP-005: Fix advanced patterns (search & replace)
- WP-006: Fix example READMEs (update text)

**Requires human/Claude oversight:**
- WP-003: Trinity migration guide (needs strategic thinking)
- WP-007: RAG tutorial (needs coherent narrative)
- WP-012: Compliance matrix (needs accuracy verification)
- WP-024: Persona reviews (needs actual testing)

---

### Option 2: Manual Execution (Human Team)

Follow the work package order defined in the overview.

---

## Work Package Execution Order

### Week 1: Critical Fixes (Naming Consistency)

**Priority: Fix authoritative documents FIRST**

1. **WP-001** (8 hrs) - Fix Core Docs Naming
   - Files: `docs/core/fraiseql-philosophy.md`, create `trinity-pattern.md`
   - Goal: 0 SQL naming errors in core docs

2. **WP-002** (8 hrs) - Fix Database Docs Naming
   - Files: `TABLE_NAMING_CONVENTIONS.md`, `DATABASE_LEVEL_CACHING.md`, move `trinity_identifiers.md`
   - Goal: Authoritative docs are consistent

3. **WP-005** (10 hrs) - Fix Advanced Patterns
   - Files: `database-patterns.md`, `multi-tenancy.md`, `bounded-contexts.md`
   - Goal: Advanced docs use trinity pattern

4. **WP-006** (4 hrs) - Fix Example READMEs
   - Files: `examples/blog_simple/README.md`, `examples/mutations_demo/README.md`
   - Goal: READMEs match SQL files

5. **WP-010** (4 hrs) - Create Security/Compliance Hub
   - Files: New `docs/security-compliance/README.md`
   - Goal: Foundation for security docs

6. **WP-016** (4 hrs) - Update Blog Simple Example
   - Files: `examples/blog_simple/` code
   - Goal: Example runs, uses trinity pattern

**Week 1 Milestone:** 0 files with SQL naming inconsistencies

---

### Week 2: New Critical Guides

**Priority: Fill documentation gaps**

7. **WP-003** (6 hrs) - Create Trinity Migration Guide
   - Files: New `docs/database/migrations.md`
   - Goal: Users can migrate from simple → trinity

8. **WP-017** (12 hrs) - Create RAG Example App
   - Files: New `examples/rag-system/` (app code)
   - Goal: Working RAG application

9. **WP-007** (8 hrs) - Write RAG Tutorial
   - Files: New `docs/ai-ml/rag-tutorial.md`
   - Depends on: WP-017 complete
   - Goal: Copy-paste RAG tutorial

10. **WP-008** (4 hrs) - Vector Operators Reference
    - Files: New `docs/reference/vector-operators.md`
    - Goal: All 6 operators documented

11. **WP-011** (6 hrs) - SLSA Provenance Guide
    - Files: New `docs/security-compliance/slsa-provenance.md`
    - Goal: Procurement officers can verify SLSA

12. **WP-012** (8 hrs) - Compliance Matrix
    - Files: New `docs/security-compliance/compliance-matrix.md`
    - Goal: NIST/FedRAMP/NIS2/DoD matrix

13. **WP-013** (6 hrs) - Security Profiles Guide
    - Files: New `docs/security-compliance/security-profiles.md`
    - Goal: STANDARD/REGULATED/RESTRICTED documented

14. **WP-014** (6 hrs) - Production Checklist
    - Files: New `docs/production/deployment-checklist.md`
    - Goal: Pre-launch validation checklist

**Week 2 Milestone:** All 8 critical missing guides created

---

### Week 3: Examples & QA Start

15. **WP-020** (6 hrs) - Test All Examples
    - Task: Run all example apps, ensure they work
    - Goal: 100% examples pass

16. **WP-021** (12 hrs) - Validate Code Examples
    - Task: Extract code from docs, test
    - Goal: All code examples run on v1.8.0-beta.1
    - Timeline: Ongoing through Week 4

17. **WP-022** (8 hrs) - Check Contradictions
    - Task: Automated + manual contradiction detection
    - Goal: Zero contradictions

**Week 3 Milestone:** Code examples validated, contradictions eliminated

---

### Week 4: Final QA & Release

18. **WP-023** (4 hrs) - Validate Links
    - Task: Link checker on all files
    - Goal: Zero broken links

19. **WP-024** (12 hrs) - Run Persona Reviews
    - Task: Simulate all 7 personas
    - Goal: 7/7 personas pass

20. **WP-025** (4 hrs) - Final Quality Gate
    - Task: Go/no-go decision
    - Goal: Documentation ready for release

**Week 4 Milestone:** Documentation released

---

## How to Execute a Work Package

### Step 1: Read the Work Package

```bash
cd /home/lionel/code/fraiseql/.phases/docs-review/fraiseql_docs_work_packages
cat WP-001-fix-core-docs-naming.md
```

Each work package contains:
- **Objective** - What to accomplish
- **Files to Update** - Specific file paths
- **Acceptance Criteria** - How to know you're done
- **Implementation Steps** - Detailed instructions
- **Timeline** - Estimated hours

---

### Step 2: Execute the Changes

**Option A: Manual Execution**
1. Open the files listed in "Files to Update"
2. Follow "Implementation Steps"
3. Check off "Acceptance Criteria" as you go

**Option B: Local Model Execution** (for simple edits)
1. Extract the specific changes needed
2. Create prompt for local model:
   ```
   Replace all occurrences of "CREATE TABLE users" with "CREATE TABLE tb_user"
   in file: /home/lionel/code/fraiseql/docs/core/fraiseql-philosophy.md
   Show only the modified sections.
   ```
3. Verify output before applying

**Option C: Claude Execution**
1. Ask Claude to execute the work package
2. Claude reads the files, makes changes
3. Review and approve changes

---

### Step 3: Verify Acceptance Criteria

Each work package has a checklist. Example from WP-001:

- [ ] Zero old naming (no `CREATE TABLE users`)
- [ ] Consistent trinity pattern (all use `tb_*`, `v_*`, `tv_*`)
- [ ] All code examples run
- [ ] Links work
- [ ] Follows style guide

**Don't proceed to next work package until all criteria met.**

---

### Step 4: Track Progress

Update the work package with completion status:

```markdown
# Work Package: Fix Core Docs Naming

**Status:** ✅ COMPLETE
**Completed by:** [Your name/AI]
**Date:** 2025-12-07
**Actual Hours:** 7.5 (vs estimated 8)

## Acceptance Criteria

- [x] Zero old naming
- [x] Consistent trinity pattern
- [x] All code examples run
- [x] Links work
- [x] Follows style guide
```

---

## Using Local AI Model (vLLM)

### Best Practices for Local Model Execution

**✅ GOOD tasks for local model:**
- Search & replace (SQL naming fixes)
- Pattern application (apply same fix to multiple files)
- Formatting (add code block languages, fix indentation)
- Simple content generation (with clear template)

**❌ NOT GOOD for local model:**
- Strategic writing (tutorials, guides)
- Complex reasoning (architecture decisions)
- Multi-file coordination (links, dependencies)
- Accuracy-critical content (compliance matrices)

### Example: Using vLLM for WP-001

```bash
# Step 1: Identify the file and change needed
FILE="/home/lionel/code/fraiseql/docs/core/fraiseql-philosophy.md"
LINE=139

# Step 2: Read current content
cat $FILE | sed -n '135,145p'

# Step 3: Create prompt for local model
PROMPT="Replace this SQL code:

CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT,
    email TEXT
);

With this SQL code:

CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    name TEXT,
    email TEXT
);

CREATE VIEW v_user AS
SELECT * FROM tb_user;

Show only the replacement code."

# Step 4: Send to vLLM
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "/data/models/fp16/Ministral-3-8B-Instruct-2512",
    "messages": [
      {"role": "system", "content": "You are a technical writer. Provide only code, no explanations."},
      {"role": "user", "content": "'"$PROMPT"'"}
    ],
    "temperature": 0,
    "max_tokens": 500
  }' | jq -r '.choices[0].message.content'

# Step 5: Review output, apply manually if correct
```

---

## Progress Tracking

### Create a tracking file:

```bash
cat > /home/lionel/code/fraiseql/.phases/docs-review/PROGRESS.md << 'EOF'
# Documentation Improvement Progress

**Started:** 2025-12-07
**Target Completion:** 2025-12-30 (4 weeks)

## Week 1: Critical Fixes

- [ ] WP-001: Fix Core Docs Naming (0/8 hrs)
- [ ] WP-002: Fix Database Docs Naming (0/8 hrs)
- [ ] WP-005: Fix Advanced Patterns (0/10 hrs)
- [ ] WP-006: Fix Example READMEs (0/4 hrs)
- [ ] WP-010: Create Security Hub (0/4 hrs)
- [ ] WP-016: Update Blog Simple (0/4 hrs)

**Week 1 Total:** 0/38 hours

## Week 2: New Guides

- [ ] WP-003: Trinity Migration Guide (0/6 hrs)
- [ ] WP-017: RAG Example App (0/12 hrs)
- [ ] WP-007: RAG Tutorial (0/8 hrs)
- [x] WP-008: Vector Operators Reference (4/4 hrs) ✅ COMPLETED
- [ ] WP-011: SLSA Provenance Guide (0/6 hrs)
- [ ] WP-012: Compliance Matrix (0/8 hrs)
- [ ] WP-013: Security Profiles Guide (0/6 hrs)
- [ ] WP-014: Production Checklist (0/6 hrs)

**Week 2 Total:** 0/56 hours

## Week 3: QA

- [ ] WP-020: Test All Examples (0/6 hrs)
- [ ] WP-021: Validate Code Examples (0/12 hrs)
- [ ] WP-022: Check Contradictions (0/8 hrs)

**Week 3 Total:** 0/26 hours

## Week 4: Final QA

- [ ] WP-023: Validate Links (0/4 hrs)
- [ ] WP-024: Run Persona Reviews (0/12 hrs)
- [ ] WP-025: Final Quality Gate (0/4 hrs)

**Week 4 Total:** 0/20 hours

---

**Grand Total:** 0/140 hours (20 work packages)
EOF
```

---

## Success Metrics

Track these metrics weekly:

```bash
# SQL naming errors remaining
grep -r "CREATE TABLE users" docs/ | wc -l
grep -r "CREATE TABLE posts" docs/ | wc -l

# Goal: 0

# Broken links
find docs/ -name "*.md" -exec markdown-link-check {} \; | grep ERROR | wc -l

# Goal: 0

# Code examples tested
# (manual tracking)

# Goal: 100%
```

---

## Next Steps

1. **Review all work packages** in `fraiseql_docs_work_packages/`
2. **Choose execution method:**
   - Local AI for simple edits (WP-001, WP-002, WP-005, WP-006)
   - Claude for complex writing (WP-003, WP-007, WP-012)
   - Manual for testing (WP-020, WP-024)
3. **Start with WP-001** (Fix Core Docs Naming)
4. **Track progress** in PROGRESS.md
5. **Review quality** after each work package (don't batch)

---

**Ready to begin!** All planning is complete. Work packages are detailed and executable.
