# Next Steps for Junior Developer - Post Journey Docs Verification

**Date:** 2025-12-08
**Context:** WP-004 and WP-005 completed, 4 new work packages created from verification
**Status:** Journey docs have hallucinations that need fixing

---

## Current Situation

✅ **Completed:**
- WP-004: Journey Pages (Set 1) - Created 3 journey guides
- WP-005: Fix Advanced Patterns Naming - SQL naming corrections

⚠️ **Problem Discovered:**
Journey documentation contains 3 significant hallucinations (missing features referenced as if they exist)

🎯 **Decision Point:**
Should junior developer:
1. Fix the hallucinations in journey docs (remove bad references)?
2. Move to next documentation work package?
3. Implement one of the missing features?

---

## Option 1: Fix Journey Doc Hallucinations ⭐ RECOMMENDED

**What:** Update the 3 journey files to remove or correct hallucinated references

**Effort:** 2-3 hours

**Tasks:**

### 1. Fix Backend Engineer Journey (High Priority)
**File:** `docs/journeys/backend-engineer.md`

**Changes Needed:**

#### A. Remove Benchmark Script Reference (lines 42-44)
**Current (WRONG):**
```bash
cd fraiseql/benchmarks
python run_performance_comparison.py
```

**Fix Option 1 (Remove entirely):**
```markdown
### Step 2: Performance Deep-Dive (30 minutes)

**Goal:** Understand FraiseQL's performance architecture

**Read:** [Rust Pipeline Integration](../core/rust-pipeline-integration.md)

**Key Concepts:**
- Zero-copy JSONB processing
- Rust JSON serialization (7-10x faster than Python)
- How the Rust pipeline integrates with Python GraphQL layer

**Success Check:** You understand why Rust improves performance
```

**Fix Option 2 (Note it's in development):**
```markdown
**Hands-on Benchmark:**
> **Note:** Comprehensive benchmark script is in development (WP-026).
> For now, review existing benchmarks in `benchmarks/` directory.

```bash
# Run existing Rust vs Python benchmark
cd fraiseql/benchmarks
python rust_vs_python_benchmark.py
```

**Expected Results:**
- Rust pipeline shows significant performance improvement
- See `benchmarks/README.md` for interpretation
```

#### B. Remove Connection Pooling Example (lines 103-109)
**Current (WRONG):**
```python
app = create_fraiseql_app(
    database_url="postgresql://...",
    connection_pool_size=20  # ❌ DOES NOT EXIST
)
```

**Fix (Remove this example):**
```markdown
3. **Connection Pooling:**
   > **Note:** Explicit connection pooling configuration is planned (WP-027).
   > FraiseQL currently uses default asyncpg connection pooling.
   > For production tuning, see [Database Configuration](../database/configuration.md).
```

#### C. Remove Migration Guide Links (lines 60-64)
**Current (WRONG):**
```markdown
**Framework-Specific Guides:**
- **From Strawberry:** [Migration Guide](../migration/from-strawberry.md)
- **From Graphene:** [Migration Guide](../migration/from-graphene.md)
- **From PostGraphile:** [Migration Guide](../migration/from-postgraphile.md)
```

**Fix:**
```markdown
**Migration Assessment:**

> **Note:** Detailed framework-specific migration guides are in development (WP-028).
> Contact the team on Discord for migration assistance.

**General Migration Effort:**
- **Strawberry migration:** 2-3 weeks for 2 engineers
- **Graphene migration:** 1-2 weeks for 2 engineers
- **PostGraphile migration:** 3-4 days for 1 engineer

**Key Migration Steps:**
1. Audit your current schema (types, resolvers, mutations)
2. Create PostgreSQL views using trinity pattern (tb_/v_/tv_)
3. Convert resolvers to FraiseQL decorators
4. Test thoroughly with side-by-side comparison
5. Deploy using blue-green strategy
```

#### D. Remove `/ready` Endpoint (line 133)
**Current (WRONG):**
```bash
# Readiness probe
curl http://localhost:8000/ready
```

**Fix:**
```markdown
**Deployment Commands:**
```bash
# Health check (liveness probe)
curl http://localhost:8000/health

# Metrics endpoint
curl http://localhost:8000/metrics

# Readiness probe (in development - WP-029)
# For now, use /health for both liveness and readiness
```

---

### 2. Fix Architect/CTO Journey (Low Priority)
**File:** `docs/journeys/architect-cto.md`

**Changes Needed:**

#### A. Fix Compliance Matrix Link (line 230)
**Current (WRONG):**
```markdown
- [Compliance Matrix](../security-compliance/compliance-matrix.md)
```

**Fix:**
```markdown
- [Compliance Matrix](../security/controls-matrix.md)
```

#### B. Clarify Docker Images Status (line 90)
**Current (VAGUE):**
```markdown
- **Docker:** Official images with Rust binaries
```

**Fix:**
```markdown
- **Docker:** Production-ready Dockerfiles provided (see `deploy/docker/`)
```

---

### 3. Junior Developer Journey
**File:** `docs/journeys/junior-developer.md`

**Status:** ✅ No changes needed - all claims verified as accurate!

---

## Option 2: Move to Next Documentation WP 🔄

**What:** Start WP-006 (Fix Example READMEs)

**Pros:**
- Continue with planned documentation work
- Fix contradictions between READMEs and SQL files

**Cons:**
- Journey docs remain broken (bad user experience)
- Backend engineers hitting broken links during evaluation

**Verdict:** Not recommended until journey docs are fixed

---

## Option 3: Implement Missing Features 🛠️

**What:** Implement WP-028 (Migration Guides) - highest priority missing feature

**Pros:**
- Addresses biggest adoption blocker
- Makes journey docs accurate (no "coming soon" notes)

**Cons:**
- 12 hours of work (beyond junior dev scope for now)
- Requires deep knowledge of Strawberry/Graphene/PostGraphile

**Verdict:** Better suited for senior technical writer or engineer

---

## Recommended Next Steps

### Immediate (Today): Fix Journey Doc Hallucinations ⭐

**Priority: HIGH**

1. **Backend Engineer Journey** (1.5 hours)
   - Remove benchmark script reference (Option 1: clean removal)
   - Remove connection pooling example
   - Add note about migration guides in development
   - Fix `/ready` endpoint reference

2. **Architect/CTO Journey** (0.5 hours)
   - Fix compliance matrix link
   - Clarify Docker images status

3. **Test the Changes** (0.5 hours)
   - Verify all internal links work
   - Check markdown rendering
   - Validate no broken references remain

**Total Time:** 2-3 hours

**Deliverable:** Journey docs are accurate (with clear notes about features in development)

---

### Then (Tomorrow): Continue Documentation Work

**Option A: WP-006 - Fix Example READMEs** (4 hours)
- Update `examples/blog_simple/README.md` to use trinity naming
- Update `examples/mutations_demo/README.md` to use trinity naming
- Ensure READMEs match actual SQL files

**Option B: Wait for Senior Dev to Complete WP-026, WP-027, WP-028**
- Then update journey docs to reference real features (no "coming soon")

---

## Why Fix Journey Docs First?

1. **User Trust:** Broken links damage credibility during evaluation
2. **Adoption Impact:** Backend engineers are key decision-makers
3. **Quick Win:** 2-3 hours to fix vs 12+ hours to implement features
4. **Professional Polish:** Shows attention to detail and quality

---

## Decision Tree

```
Are journey docs being used for evaluation/demos?
├─ YES → Fix hallucinations immediately (Option 1) ⭐
└─ NO → Proceed with WP-006 (Option 2)

Are senior devs available to implement WP-026/027/028?
├─ YES → Fix journey docs now, they'll add features later
└─ NO → Fix journey docs with "coming soon" notes
```

---

## Commit Message Template

After fixing journey docs:

```
docs(journeys): Fix hallucinated references in journey documentation

Backend Engineer Journey:
- Remove reference to non-existent run_performance_comparison.py
- Remove connection_pool_size parameter example (not yet implemented)
- Update migration guides section with development status note
- Fix /ready endpoint reference (use /health for now)

Architect/CTO Journey:
- Fix compliance matrix link: security-compliance/ → security/
- Clarify Docker images status (Dockerfiles provided, not published images)

Junior Developer Journey:
- No changes needed (all claims verified accurate)

Context: Journey docs created with references to planned features
that don't yet exist (WP-026, WP-027, WP-028, WP-029). This commit
removes hallucinations and adds clear notes about development status.

Fixes: Documentation verification report findings (Dec 8, 2025)
```

---

## Summary

**Recommended Action:** Fix journey doc hallucinations first (Option 1)

**Rationale:**
- Quick fix (2-3 hours)
- High impact (prevents broken user experience)
- Professional polish
- Unblocks use of journey docs for demos/evaluation

**After That:**
- Continue with WP-006 (Fix Example READMEs)
- OR wait for WP-026/027/028/029 implementation by senior devs

**Long-Term:**
Once WP-026, WP-027, WP-028, WP-029 are complete, update journey docs again to reference real features (remove "coming soon" notes).

---

**Next Command:**
```bash
# Start fixing backend engineer journey
code docs/journeys/backend-engineer.md
```

**End of Next Steps Assessment**
