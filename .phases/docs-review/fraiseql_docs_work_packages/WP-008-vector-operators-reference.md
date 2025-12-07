# Work Package: Write Vector Operators Reference

**Package ID:** WP-008
**Assignee Role:** Technical Writer - API/Examples (TW-API)
**Priority:** P0 - Critical
**Estimated Hours:** 4 hours
**Dependencies:** None

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
