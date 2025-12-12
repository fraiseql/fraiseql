# FraiseQL Documentation Quality Assurance Framework

**Purpose:** Ensure 10x improvement in documentation quality through systematic review and validation
**Owner:** Documentation Architect
**Enforced by:** ENG-QA, Team Leads, Architect

---

## Quality Standards

### 1. SQL Naming Convention Standard

**Rule:** ALL SQL examples MUST use trinity pattern naming

✅ **CORRECT:**
```sql
CREATE TABLE tb_user (id UUID PRIMARY KEY, name TEXT);
CREATE VIEW v_user AS SELECT * FROM tb_user;
CREATE VIEW tv_user_with_posts AS SELECT u.*, COUNT(p.id) FROM tb_user u LEFT JOIN tb_post p ...;
```

❌ **INCORRECT:**
```sql
CREATE TABLE users (id UUID PRIMARY KEY, name TEXT);  -- No tb_ prefix
CREATE VIEW users_view AS SELECT * FROM users;  -- Not v_ pattern
```

**Exception:** Migration guides teaching transition from simple → trinity (must be clearly labeled)

**Validation:**
- Automated: Grep for `CREATE TABLE [a-z_]+` without `tb_` prefix
- Manual: ENG-QA reviews all SQL blocks

---

### 2. Code Example Standard

**All code examples MUST:**

1. **Run successfully** on v1.8.0-beta.1
2. **Include expected output:**
   ```python
   result = await schema.execute(query)
   # Expected output:
   # {
   #   "data": {
   #     "users": [{"id": "123", "name": "Alice"}]
   #   }
   # }
   ```

3. **Specify language** in code blocks:
   ```markdown
   ```sql
   SELECT * FROM tb_user;
   ​```
   ```

4. **Handle errors** (show common failure modes):
   ```python
   try:
       result = await db.execute(query)
   except DatabaseError as e:
       # Common error: Connection pool exhausted
       # Solution: Increase pool size in config
   ```

5. **Link to source** (for full examples):
   ```markdown
   See [blog-simple example](../../examples/blog_simple/) for complete implementation.
   ```

**Validation:**
- Automated: Extract code blocks, run through test harness
- Manual: ENG-QA spot-checks 20% of examples

---

### 3. Page Structure Standard

**Every documentation page MUST have:**

1. **Time estimate** (at top):
   ```markdown
   **Time to read:** 10 minutes
   **Time to complete:** 30 minutes (hands-on tutorial)
   ```

2. **Prerequisites** (clearly listed):
   ```markdown
   ## Prerequisites
   - PostgreSQL 14+ installed
   - FraiseQL v1.8.0+ installed
   - Basic GraphQL knowledge
   ```

3. **"Next Steps" section** (at bottom):
   ```markdown
   ## Next Steps
   - [Advanced vector search patterns](../ai-ml/embedding-strategies.md)
   - [Production deployment checklist](../production/deployment-checklist.md)
   - [RAG system example](../../examples/rag-system/)
   ```

4. **Clear section headings** (Markdown ## H2, ### H3):
   ```markdown
   ## Overview
   ### What is X?
   ### Why use X?

   ## How It Works
   ### Step 1: ...
   ### Step 2: ...
   ```

**Validation:**
- Automated: Check for required sections (time estimate, prerequisites, next steps)
- Manual: Team Lead reviews structure

---

### 4. Writing Style Standard

**Active voice** (not passive):
- ✅ "Configure the database" (active)
- ❌ "The database should be configured" (passive)

**Actual commands** (not vague instructions):
- ✅ `uv pip install fraiseql[ai]`
- ❌ "Install the AI dependencies"

**Emoji sparingly** (only for status):
- ✅ Success indicator
- ❌ Error indicator
- ⚠️ Warning indicator
- (No decorative emoji)

**Clear, concise sentences:**
- ✅ "FraiseQL uses trinity pattern for database naming."
- ❌ "FraiseQL, being a modern framework that emphasizes best practices, utilizes what is known as the trinity pattern, which is a naming convention for database objects that provides several benefits including..."

**Validation:**
- Manual: Team Lead reviews for style compliance
- Tool-assisted: Grammarly/LanguageTool for passive voice detection

---

## Review Process

### Level 1: Self-Review (Writer/Engineer)

**Before submitting work package, check:**

- [ ] Follows style guide (active voice, actual commands, clear headings)
- [ ] SQL examples use tb_/v_/tv_ naming
- [ ] Code blocks specify language
- [ ] Time estimates included
- [ ] Prerequisites listed
- [ ] "Next Steps" section present
- [ ] All links work (manually clicked)
- [ ] Spell-checked (no typos)

**Tool:** Use checklist in each work package

---

### Level 2: Team Lead Review (TW-LEAD or ENG-LEAD)

**Team Lead checks:**

- [ ] **Quality score ≥ 4/5** (overall assessment)
- [ ] **Style guide compliance** (active voice, formatting)
- [ ] **No grammar errors** (readable, professional)
- [ ] **Consistent terminology** (same terms as other docs)
- [ ] **Appropriate depth** (not too shallow, not too deep for audience)

**Timeline:** <24 hours from submission

**Outcome:**
- ✅ **Approved** → Pass to Architect review
- ⚠️ **Minor changes needed** → Return to writer with notes
- ❌ **Major rework needed** → Return to writer, may reassign

---

### Level 3: Technical Accuracy Review (ENG-QA)

**ENG-QA validates:**

- [ ] **Code examples run** (tested on v1.8.0-beta.1)
- [ ] **SQL is valid** (no syntax errors)
- [ ] **Technical claims verified** (links to source code/tests/benchmarks)
- [ ] **No contradictions** (consistent with other documentation)
- [ ] **API references accurate** (matches actual API)

**Method:**
1. Extract all code blocks from markdown
2. Run SQL through PostgreSQL validator
3. Run Python through `ruff` linter
4. Execute examples (where feasible)
5. Cross-check claims against codebase

**Timeline:** Ongoing (starts Week 2, validates as work packages complete)

**Outcome:**
- ✅ **Technically accurate** → Pass
- ⚠️ **Minor technical issues** → Flag for writer to fix
- ❌ **Critical technical errors** → Block merge, escalate to Architect

---

### Level 4: Persona Review (ENG-QA)

**ENG-QA simulates personas:**

For each persona journey:
1. Start with persona's goal (e.g., "Build RAG system in <2 hours")
2. Follow documentation journey (links, tutorials, examples)
3. Note where stuck or confused
4. Verify success criteria met

**Personas tested:**
- Junior Developer
- Senior Backend Engineer
- AI/ML Engineer
- DevOps Engineer
- Security Officer
- CTO/Architect
- Procurement Officer

**Timeline:** Week 4 (WP-024)

**Outcome:**
- ✅ **Persona succeeds** → Journey validated
- ⚠️ **Persona struggles but completes** → Note improvements, non-blocking
- ❌ **Persona fails** → Block release, escalate to Architect

---

### Level 5: Architect Final Approval (Documentation Architect)

**Architect checks:**

- [ ] **Meets acceptance criteria** (from work package)
- [ ] **Aligns with architecture blueprint** (follows structure)
- [ ] **No contradictions** (single source of truth)
- [ ] **Quality score ≥ 4/5** (meets standards)
- [ ] **Strategic value** (contributes to 10x improvement)

**Timeline:** <24 hours from Team Lead + ENG-QA approval

**Outcome:**
- ✅ **Approved** → Merge to main branch
- ⚠️ **Minor tweaks** → Quick fixes, then merge
- ❌ **Reject** → Return with detailed feedback, may reassign

---

## Conflict Detection Methodology

### Automated Conflict Detection

**Tool:** Custom script to detect contradictions

**Method:**
1. Extract all documentation into database
2. Search for same concepts across files (e.g., "trinity pattern", "tb_user", "SLSA provenance")
3. Compare explanations using semantic similarity
4. Flag high-similarity sections that differ in recommendations

**Example:**
```
CONFLICT DETECTED:
File 1: docs/core/trinity-pattern.md says "Always use tb_ prefix"
File 2: docs/database/naming-conventions.md says "Use tb_ for production, users for prototypes"

Severity: MEDIUM
Action: Ensure both docs clarify context (production vs prototype)
```

**Run:** Weekly during development, final check in Week 4 (WP-022)

---

### Manual Conflict Detection

**Method:**
1. Cross-check authoritative documents:
   - `docs/database/naming-conventions.md`
   - `docs/core/trinity-pattern.md`
   - `docs/database/trinity-identifiers.md`

2. Verify examples match reference docs:
   - Example apps use same patterns as documented
   - READMEs match SQL files

3. Check versioning:
   - No references to deprecated features without clear warnings
   - Version-specific guidance is labeled

**Owner:** ENG-QA (WP-022)

**Acceptance:** ZERO contradictions before release

---

## Freshness Verification

### Ensuring Examples Match Current Version

**Problem:** Documentation lags behind code changes

**Solution:**

1. **Version pinning in examples:**
   ```python
   # This example tested with FraiseQL v1.8.0-beta.1
   # Last updated: 2025-12-07
   ```

2. **Automated testing:**
   - CI job runs all code examples on each release
   - If example fails, CI fails → docs must be updated

3. **Quarterly freshness audit:**
   - Every 3 months, review all docs for outdated references
   - Update version numbers, deprecation warnings
   - Re-test all examples

**Validation:**
- ENG-QA (WP-021) validates all examples run on v1.8.0-beta.1
- Architect reviews version references in final approval

---

## Quality Gates (Go/No-Go Criteria)

### Gate 1: Work Package Completion

**Before moving to next phase:**

- [ ] All P0 work packages complete
- [ ] P1 work packages: >80% complete (can defer some)
- [ ] No blockers (dependencies resolved)

**Owner:** Team Leads report to Architect daily

---

### Gate 2: Technical Accuracy

**Before release:**

- [ ] **100% of code examples run** (no failures)
- [ ] **Zero SQL syntax errors**
- [ ] **All technical claims verified** (links to evidence)
- [ ] **Zero contradictions** (single source of truth)

**Owner:** ENG-QA (WP-021, WP-022)

**Blocker:** If any criteria fails, BLOCK release

---

### Gate 3: Persona Validation

**Before release:**

- [ ] **7/7 personas pass review** (all can accomplish goals)
- [ ] **Time estimates accurate** (±20% variance acceptable)
- [ ] **No critical confusions** (personas don't get stuck)

**Owner:** ENG-QA (WP-024)

**Blocker:** If >1 persona fails, BLOCK release

---

### Gate 4: Link Validation

**Before release:**

- [ ] **Zero broken internal links** (relative paths work)
- [ ] **Zero broken external links** (GitHub, docs sites reachable)
- [ ] **All "Next Steps" links valid**

**Owner:** ENG-QA (WP-023)

**Blocker:** If any critical links broken, BLOCK release

---

### Gate 5: Final Quality Assessment

**Before release:**

- [ ] **Average quality score ≥ 4.0/5** (across all deliverables)
- [ ] **Zero P0 work packages incomplete**
- [ ] **All acceptance criteria met**
- [ ] **Architect approval** (final sign-off)

**Owner:** Documentation Architect (WP-025)

**Outcome:**
- ✅ **GO** → Release documentation
- ❌ **NO-GO** → Fix critical issues, re-review

---

## Quality Scoring Rubric

### Quality Score: 1-5 Scale

**5/5 - Excellent:**
- Authoritative reference quality
- Zero errors (technical, grammar, style)
- Clear, concise, engaging writing
- Perfect code examples (run flawlessly)
- Exceeds acceptance criteria

**4/5 - Good:**
- High quality, professional
- Minor issues (1-2 small fixes needed)
- Meets all acceptance criteria
- Code examples work correctly
- Minor style improvements possible

**3/5 - Fair:**
- Acceptable but needs improvement
- Some errors or unclear sections
- Meets most acceptance criteria
- Code examples work with minor issues
- Requires revision before approval

**2/5 - Poor:**
- Significant issues
- Multiple errors (technical or style)
- Does not meet acceptance criteria
- Code examples have problems
- Requires major rework

**1/5 - Unacceptable:**
- Critical errors
- Misleading or incorrect information
- Does not meet acceptance criteria
- Code examples broken
- Must be rewritten

**Minimum for Approval:** 4/5 (Good)

---

## Tools and Automation

### Link Checker
```bash
# Run link checker on all markdown files
find docs/ -name "*.md" -exec markdown-link-check {} \;
```

### Code Extractor & Validator
```python
# Extract SQL blocks, validate syntax
import re
import subprocess

def validate_sql_blocks(markdown_file):
    with open(markdown_file) as f:
        content = f.read()

    sql_blocks = re.findall(r'```sql\n(.*?)\n```', content, re.DOTALL)

    for sql in sql_blocks:
        # Validate SQL syntax using PostgreSQL
        result = subprocess.run(
            ['psql', '-c', sql, '--dry-run'],
            capture_output=True
        )
        if result.returncode != 0:
            print(f"INVALID SQL in {markdown_file}: {sql}")
```

### Contradiction Detector
```python
# Semantic search for contradictions
from sentence_transformers import SentenceTransformer, util

def detect_contradictions(docs_dir):
    model = SentenceTransformer('all-MiniLM-L6-v2')

    # Extract all paragraphs about "trinity pattern"
    paragraphs = extract_paragraphs(docs_dir, keyword="trinity pattern")

    # Compute embeddings
    embeddings = model.encode(paragraphs)

    # Find high-similarity pairs that differ in recommendations
    for i, para1 in enumerate(paragraphs):
        for j, para2 in enumerate(paragraphs[i+1:]):
            similarity = util.cos_sim(embeddings[i], embeddings[j+i+1])

            if similarity > 0.8 and differs_in_recommendation(para1, para2):
                print(f"POTENTIAL CONFLICT:\n{para1}\nvs\n{para2}")
```

---

## Continuous Quality Monitoring (Post-Release)

### Weekly Checks (Automated)
- Link validation (broken links)
- Code example testing (still run on latest version)
- Security scan (no hardcoded secrets in examples)

### Quarterly Audits (Manual)
- Persona reviews (re-test all 7 journeys)
- Freshness audit (version references, deprecated features)
- Quality scoring (random sample of 20% of docs)

### Community Feedback Loop
- GitHub issues for documentation problems
- "Was this helpful?" widget on docs pages
- User surveys (quarterly)

**Owner:** Ongoing maintenance team (1 writer + 1 engineer, part-time)

---

## Escalation Path

### Issue Severity Levels

**P0 - Critical:**
- Blocks release (broken code examples, contradictions)
- Security issue (hardcoded credentials)
- Misleading information (causes user harm)

**P1 - High:**
- Degrades quality (broken links, style issues)
- Missing critical information
- Time estimates significantly off

**P2 - Medium:**
- Minor improvements needed
- Cosmetic issues
- Nice-to-have enhancements

### Escalation Process

**Writer/Engineer** encounters issue:
1. Try to resolve independently (consult style guide, ask team)
2. If unresolved, escalate to **Team Lead**

**Team Lead** cannot resolve:
1. Escalate to **Documentation Architect**
2. Provide context: issue, attempts to resolve, recommendation

**Documentation Architect** makes final decision:
1. Resolve issue (may consult codebase owner)
2. Document decision (update work package or create ADR)
3. Communicate resolution to team

**Timeline:**
- P0: <4 hours
- P1: <24 hours
- P2: <1 week

---

## Success Metrics

### Quantitative Quality Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| SQL naming errors | 0 | Automated grep + manual review |
| Broken code examples | 0 | Automated test harness |
| Broken links | 0 | Link checker |
| Contradictions | 0 | Automated + manual detection |
| Average quality score | ≥ 4.0/5 | Team Lead + Architect ratings |

### Qualitative Quality Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Persona success rate | 100% (7/7) | ENG-QA persona simulations |
| Time estimate accuracy | ±20% | Persona review feedback |
| Reader satisfaction | ≥ 4.5/5 | Community surveys (post-release) |
| Support ticket reduction | -50% | Compare pre/post-release |

---

## Appendix: Review Checklists

### Code Example Checklist

For each code example:

- [ ] Language specified (```sql, ```python)
- [ ] Code runs successfully on v1.8.0-beta.1
- [ ] Expected output shown
- [ ] Common errors documented
- [ ] Links to full example (if snippet)
- [ ] SQL uses tb_/v_/tv_ naming

### Documentation Page Checklist

For each markdown file:

- [ ] Time estimate at top
- [ ] Prerequisites listed
- [ ] Clear headings (## H2, ### H3)
- [ ] Active voice (not passive)
- [ ] Actual commands (not vague)
- [ ] "Next Steps" section at bottom
- [ ] All links work
- [ ] Spell-checked

### Work Package Checklist

Before marking complete:

- [ ] All deliverables created
- [ ] Self-review checklist passed
- [ ] Team Lead review passed
- [ ] ENG-QA validation passed (if applicable)
- [ ] Meets all acceptance criteria
- [ ] Quality score ≥ 4/5

---

**End of QA Framework**
