# Work Package: Write Vector Operators Reference

**Package ID:** WP-008
**Assignee Role:** Technical Writer - API/Examples (TW-API)
**Priority:** P0 - Critical
**Estimated Hours:** 4 hours
**Dependencies:** None

---

## ⚠️ Execution Requirement

**⚠️ PARTIAL LOCAL MODEL USAGE POSSIBLE** (with careful oversight)

**This work package can use local models for PATTERN APPLICATION only**

**Strategy:**
1. **Claude writes:** First 3-4 operator examples with complete documentation (2 hours)
2. **Local model applies pattern:** Remaining operators following exact template (1 hour)
3. **Claude reviews:** Verify accuracy, fix hallucinations (1 hour)

**Why local models struggle alone:**
- **Technical accuracy critical** (wrong operator syntax breaks code)
- **Performance characteristics** (requires understanding of vector indexes)
- **Example quality** (must be practical, not toy examples)

**What local models CAN do:**
- ✅ Apply documentation pattern to similar operators
- ✅ Fill in operator signature templates
- ✅ Generate simple usage examples (if given template)

**What local models CANNOT do:**
- ❌ Understand performance implications
- ❌ Create nuanced examples
- ❌ Explain when to use which operator

**Recommended:** Claude writes this (4 hours), or use hybrid approach (4 hours total, 25% savings)

---

## Objective

Document all 6 pgvector distance operators with clear use cases.

---

## Deliverable

**New File:** `docs/reference/vector-operators.md`

---

## Content Outline

```markdown
# Vector Search Operators Reference

## 1. Cosine Distance (`<=>`)
**Use when:** Comparing document similarity (most common)

## 2. L2 Distance (`<->`)
**Use when:** Euclidean distance needed (image similarity)

## 3. Inner Product (`<#>`)
**Use when:** Dot product similarity

## 4. L1 Distance (`<+>`)
**Use when:** Manhattan distance

## 5. Hamming Distance (`<~>`)
**Use when:** Binary vectors

## 6. Jaccard Distance (`<%>`)
**Use when:** Set similarity

## Decision Tree
[When to use each operator]

## Performance Considerations
[Index types, query optimization]
```

---

## Acceptance Criteria

- [ ] All 6 operators documented
- [ ] Clear use cases for each
- [ ] Examples tested
- [ ] Decision tree for choosing
- [ ] Links to vector-search.md guide

---

**Deadline:** End of Week 2
