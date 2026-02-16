# Documentation Quality Audit Plan

## 🎯 Objective

Systematically review all documentation files for clarity, consistency, appropriate tone, and adherence to FraiseQL documentation standards before v1.1.1 release.

## 📊 Scope

- **Total files**: 123 markdown files
- **Directories**: docs/, README.md, CONTRIBUTING.md, INSTALLATION.md, etc.
- **Exclusions**: dev/, archive/, .github/docs/ (internal developer docs)

## 🔍 Quality Patterns to Check

### 1. Tone & Audience Issues

- ❌ **Interview language**: "for your interview", "during the interview"
- ❌ **Internal notes**: "TODO", "FIXME", "WIP", "XXX"
- ❌ **Overly casual**: "super cool", "awesome sauce", excessive exclamation marks
- ❌ **Inconsistent voice**: switching between "we", "you", "I"
- ❌ **Developer-only jargon**: unexplained technical terms in user guides

### 1.5 FraiseQL Pattern Violations

Based on production PrintOptim codebase analysis:

**Type Definition Violations**:

- ❌ Optional fields before required fields
- ❌ Missing `@fraiseql.type(sql_source=...)`
- ❌ Missing `BaseGQLType` inheritance
- ❌ Using `= []` for list defaults (should be `| None = None`)
- ❌ Missing comprehensive docstrings
- ❌ Fields not documented in docstring

**Mutation Pattern Violations**:

- ❌ Using `None` instead of `UNSET` for optional mutation inputs
- ❌ Not separating Input, Success, Error types
- ❌ Missing `@fraiseql.mutation` with `function` parameter
- ❌ Missing `context_params` for audit fields
- ❌ Not using class-based mutation pattern

**Query Pattern Violations**:

- ❌ Missing default ordering in list queries
- ❌ Not extracting `db` and `tenant_id` from context
- ❌ Missing standard parameters (`where`, `limit`, `offset`, `order_by`)
- ❌ Incorrect return type annotations

**Naming Convention Violations**:

- ❌ Plural type names (should be singular: `Machine` not `Machines`)
- ❌ camelCase in Python (should be snake_case)
- ❌ Missing suffixes on input types (`Input`, `Success`, `Error`)
- ❌ List queries not plural (`machine` instead of `machines`)
- ❌ Count queries not suffixed with `_count`

**Code Organization Violations**:

- ❌ Multiple mutations in one file
- ❌ Mixing types and queries in same file
- ❌ Not using domain modules pattern
- ❌ Filters not centralized

**GraphQL Client Payload Violations**:

- ❌ Inline values instead of variables
- ❌ Not checking `__typename` for union types
- ❌ Not handling `errors` in response
- ❌ Using empty strings instead of `null` to clear fields
- ❌ Including all fields when only subset changed (updates)
- ❌ Not requesting `errors` array in error fragments
- ❌ Not requesting `conflictEntity` for duplicate detection

### 2. Structural Issues

- ❌ **Missing headers**: files without proper title (# Title)
- ❌ **Inconsistent heading hierarchy**: skipping levels (# to ###)
- ❌ **Empty sections**: headers with no content
- ❌ **Broken table of contents**: TOC that doesn't match actual sections

### 3. Content Issues

- ❌ **Placeholder text**: "Lorem ipsum", "[TODO]", "[DESCRIPTION]"
- ❌ **Outdated version references**: referencing old versions, deprecated APIs
- ❌ **Dead code examples**: code that won't run or uses removed features
- ❌ **Inconsistent code style**: mixing tabs/spaces, inconsistent formatting
- ❌ **Missing code language tags**: \`\`\` without language specification

### 4. Link Issues

- ❌ **Broken internal links**: links to moved/deleted files (already validated)
- ❌ **Absolute GitHub links**: should be relative links instead
- ❌ **Ambiguous link text**: "click here", "this link"
- ❌ **Missing link context**: links without explanation of destination

### 5. Formatting Issues

- ❌ **Trailing whitespace**: spaces at end of lines
- ❌ **Inconsistent list formatting**: mixing `-` and `*`
- ❌ **Missing blank lines**: around code blocks, headers
- ❌ **Inconsistent emoji usage**: some docs use emojis, others don't
- ❌ **Excessive formatting**: too many bold/italic/code tags

### 6. Technical Accuracy Issues

- ❌ **Wrong Python version**: stating 3.11+ instead of 3.13+
- ❌ **Incorrect installation commands**: outdated package names
- ❌ **Mismatched examples**: code examples that don't match explanations
- ❌ **Missing error handling**: examples without error cases

---

## 📋 Phased Execution Plan

### Phase 1: Automated Pattern Detection (Quick Scan)

**Duration**: ~30 minutes
**Tool**: Bash scripts + grep patterns
**Output**: `dev/audits/docs-quality-issues-automated.md`

**Process**:

```bash
# 1. Scan for tone issues
grep -rn "interview\|TODO\|FIXME\|WIP\|XXX" docs/ --include="*.md"

# 2. Scan for placeholder text
grep -rn "\[TODO\]\|\[DESCRIPTION\]\|Lorem ipsum" docs/ --include="*.md"

# 3. Scan for code blocks without language tags
grep -rn "^\`\`\`$" docs/ --include="*.md"

# 4. Scan for excessive exclamation marks
grep -rn "!!!\|!!!!!" docs/ --include="*.md"

# 5. Scan for absolute GitHub URLs
grep -rn "https://github.com/fraiseql/fraiseql" docs/ --include="*.md"
```

**Deliverable**: Automated scan report with line numbers and file paths

---

### Phase 2: Category-Based Manual Review

**Duration**: ~3-4 hours
**Reviewer**: Documentation specialist (AI agent or human)
**Output**: `dev/audits/docs-quality-issues-manual.md`

#### Phase 2.1: User-Facing Documentation (Priority: HIGH)

Review these critical paths first:

1. **Getting Started** (30 files)
   - `README.md` ⭐ (most important)
   - `INSTALLATION.md` ⭐
   - `CONTRIBUTING.md` ⭐
   - `docs/getting-started/*.md`
   - `docs/quickstart.md`

2. **Guides** (25 files)
   - `docs/guides/*.md`
   - Check: clarity, step-by-step accuracy, complete examples

3. **Tutorials** (15 files)
   - `docs/tutorials/*.md`
   - Check: follows beginner → advanced progression

#### Phase 2.2: Advanced Documentation (Priority: MEDIUM)

1. **Advanced Topics** (20 files)
   - `docs/advanced/*.md`
   - Check: appropriate technical depth, accurate code examples

2. **Reference** (15 files)
   - `docs/reference/*.md`
   - Check: completeness, accuracy, up-to-date API info

#### Phase 2.3: Supporting Documentation (Priority: LOW)

1. **Features** (10 files)
   - `docs/features/*.md`

2. **Strategic/Planning** (8 files)
   - `docs/strategic/*.md`
   - Note: May contain interview references intentionally (planning docs)

---

### Phase 3: Pattern-Specific Deep Dive

**Duration**: ~2 hours
**Output**: Category-specific reports

#### 3.1: Code Example Validation

**Process**:

1. Extract all Python code blocks from docs
2. Run syntax validation: `python -m py_compile`
3. Check against Python 3.13+ features
4. Verify imports are valid
5. Check that examples are self-contained or clearly reference setup

**Script**:

```bash
# Extract and validate Python code blocks
./scripts/validate-docs.sh syntax
```

#### 3.2: Tone Consistency Check

**Process**:

1. Identify all second-person references ("you", "your")
2. Identify all first-person plural ("we", "our")
3. Flag inconsistencies within same document
4. Flag overly casual language

**Criteria**:

- User guides: "you/your" (instructional tone)
- Technical reference: neutral/passive voice
- Getting started: friendly "you/your"
- Strategic docs: "we/our" (planning perspective)

#### 3.3: Version Reference Audit

**Process**:

1. Scan for all Python version mentions
2. Scan for all FraiseQL version mentions
3. Verify accuracy against current state
4. Check for deprecated API references

**Current truth**:

- Python: 3.13+ required
- FraiseQL: v1.1.1 (about to release)
- Rust extension: bundled in wheel

---

### Phase 4: Issue Categorization & Prioritization

**Duration**: ~1 hour
**Output**: `dev/audits/docs-quality-action-items.md`

**Categories**:

1. **CRITICAL** - Blocks release
   - Incorrect installation instructions
   - Wrong Python version requirements
   - Dead code examples in getting-started
   - Broken critical links in README

2. **HIGH** - Should fix before release
   - Interview language in user-facing docs
   - Placeholder text in tutorials
   - Inconsistent tone in guides
   - Missing language tags in code blocks

3. **MEDIUM** - Should fix soon after release
   - Minor formatting inconsistencies
   - Ambiguous link text
   - Excessive casual language
   - Inconsistent emoji usage

4. **LOW** - Nice to have
   - Trailing whitespace
   - Inconsistent list bullet styles
   - Minor wording improvements

---

### Phase 5: Remediation Execution

**Duration**: ~4-6 hours (depending on issues found)
**Process**: Address issues in priority order

#### 5.1: CRITICAL fixes (must complete)

- Review and test all changes
- Run full validation suite after fixes

#### 5.2: HIGH priority fixes (strongly recommended)

- Batch similar changes
- Review for consistency

#### 5.3: MEDIUM/LOW (time permitting)

- Can be deferred to v1.1.2 if needed

---

## 📤 Deliverables

### Automated Scan Report

**File**: `dev/audits/docs-quality-issues-automated.md`
**Format**:

```markdown
# Documentation Quality Issues - Automated Scan

## Tone Issues (23 found)
- docs/advanced/advanced-patterns.md:45: "for your interview"
- docs/guides/performance.md:12: "TODO: Add benchmarks"
...

## Code Blocks Without Language Tags (15 found)
- docs/getting-started/quickstart.md:67
...

## Summary
- Total issues found: 156
- CRITICAL: 3
- HIGH: 47
- MEDIUM: 89
- LOW: 17
```

### Manual Review Report

**File**: `dev/audits/docs-quality-issues-manual.md`
**Format**:

```markdown
# Documentation Quality Issues - Manual Review

## docs/getting-started/README.md

### Issues Found
1. **[HIGH] Tone inconsistency** (line 23-45)
   - Switches from "you" to "we" mid-section
   - Recommendation: Use "you" throughout getting-started docs

2. **[CRITICAL] Wrong Python version** (line 12)
   - States "Python 3.11+"
   - Should be "Python 3.13+"
...
```

### Action Items Report

**File**: `dev/audits/docs-quality-action-items.md`
**Format**:

```markdown
# Documentation Quality - Action Items

## CRITICAL (Must Fix Before Release)
- [ ] Fix Python version requirement in README.md (3.11+ → 3.13+)
- [ ] Update installation command in INSTALLATION.md
- [ ] Fix broken code example in docs/getting-started/quickstart.md:67

## HIGH Priority
- [ ] Remove "interview" language from 4 files
- [ ] Add language tags to 15 code blocks
- [ ] Fix tone consistency in docs/guides/
...
```

---

## 🤖 Implementation Options

### Option 1: AI Agent (Explore subagent)

Use Claude Code's Explore agent to systematically review each file:

```bash
# Launch comprehensive doc review
Task(
  subagent_type="Explore",
  description="Documentation quality audit",
  prompt="Review all docs/*.md files against the quality patterns..."
)
```

**Pros**:

- Fast (completes in ~1 hour)
- Comprehensive pattern matching
- Consistent evaluation

**Cons**:

- May miss nuanced context issues
- Requires human review of findings

### Option 2: Manual Review (Human)

Systematically review each category with human reviewer

**Pros**:

- Catches nuanced issues
- Better context understanding
- Higher quality assessment

**Cons**:

- Time-consuming (6-8 hours)
- Potential for inconsistent criteria
- Fatigue-based errors

### Option 3: Hybrid (RECOMMENDED)

1. Run automated scan (Phase 1) - 30 min
2. Use AI agent for initial manual review (Phase 2) - 2 hours
3. Human review of CRITICAL/HIGH findings - 2 hours
4. Execute fixes in priority order - 4 hours

**Total time**: ~8-9 hours

---

## ✅ Success Criteria

### Quality Gates

- ✅ Zero CRITICAL issues remaining
- ✅ <5 HIGH priority issues remaining
- ✅ All user-facing docs reviewed
- ✅ All code examples validated
- ✅ Consistent tone across documentation categories
- ✅ No placeholder text in published docs
- ✅ All version references accurate

### Release Readiness Checklist

- [ ] Automated scan complete
- [ ] Manual review complete for user-facing docs
- [ ] All CRITICAL issues resolved
- [ ] All HIGH issues resolved or documented as deferred
- [ ] Code examples validated
- [ ] Version references updated
- [ ] Final validation suite passes

---

## 📅 Recommended Timeline

### Before v1.1.1 Release

**Day 1** (Today):

- Morning: Phase 1 - Automated scan (30 min)
- Morning: Phase 2.1 - User-facing review (2 hours)
- Afternoon: Phase 3.2 - Tone check (1 hour)
- Afternoon: Phase 4 - Categorize issues (1 hour)

**Day 2**:

- Morning: Phase 5.1 - Fix CRITICAL issues (2 hours)
- Afternoon: Phase 5.2 - Fix HIGH issues (3 hours)
- Evening: Final validation (1 hour)

**Day 3**:

- Release v1.1.1 with quality documentation ✅

### After Release (if needed)

- Week 1: Address remaining MEDIUM issues
- Week 2: Address LOW issues as time permits

---

## 🎯 Next Steps

To execute this plan:

1. **Run automated scan first**:

   ```bash
   ./scripts/audit-docs-quality.sh
   ```

2. **Launch AI review agent**:

   ```bash
   # Use Task tool with Explore agent
   # Review dev/audits/docs-quality-issues-automated.md findings
   ```

3. **Human review of critical files**:
   - README.md
   - INSTALLATION.md
   - docs/getting-started/*.md

4. **Execute remediation**

Would you like me to:

- **Option A**: Start with the automated scan right now?
- **Option B**: Create the audit script first?
- **Option C**: Launch the AI Explore agent to begin systematic review?
