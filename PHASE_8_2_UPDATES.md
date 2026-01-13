# Phase 8.2: Critical Updates to Planning Documents

**Date**: 2026-01-13 (Post-Clarification)
**Status**: ✅ All documents updated with critical design constraints
**Purpose**: Prevent future contributors from drifting into "typed query planning"

---

## What Was Added

### 1. Explicit Boundary Declaration

**Document**: All Phase 8.2 planning files
**What**: Added clear statements about what typing does and does NOT affect

**Key Additions**:

```
Typing does NOT affect:
- SQL generation
- Filtering (where_sql, where_rust, order_by)
- Ordering (ORDER BY is identical)
- Wire protocol
- Chunking, cancellation, backpressure

Typing ONLY affects:
- Consumer-side deserialization at poll_next()
- Error messages (type name included)
```

### 2. Escape Hatch as First-Class Feature

**Document**: All Phase 8.2 planning files
**What**: Explicit requirement that `query::<serde_json::Value>()` always works

**Key Additions**:

```rust
// Always supported, always identical to untyped:
let stream = client.query::<serde_json::Value>("projects").execute().await?;

// Use cases:
// 1. Debugging: Check actual JSON structure
// 2. Forward compatibility without code changes
// 3. Generic ops workflows (any entity handler)
// 4. Partial opt-out from type safety
```

### 3. Anti-Patterns & Code Review Checklist

**Document**: `.phases/README_PHASE_8_2.md` and `PHASE_8_2_CRITICAL_CONSTRAINTS.md`
**What**: Explicit guidance on what to forbid in code review

**Examples**:
- ❌ Don't add "typed query planning"
- ❌ Don't make predicates generic
- ❌ Don't deserialize before filtering
- ❌ Don't lose type info in errors
- ❌ Don't special-case Value type
- ❌ Don't forget documentation

### 4. Documentation Requirements

**Document**: All planning files and `PHASE_8_2_CRITICAL_CONSTRAINTS.md`
**What**: Every code location must document the constraint

**Required Comment**:
```rust
/// Type parameter T is **consumer-side only**.
///
/// The type T does NOT affect:
/// - SQL generation (still `SELECT data FROM v_{entity}`)
/// - Filtering (where_sql, where_rust, order_by unchanged)
/// - Wire protocol (same as untyped streaming)
/// - Performance (< 2% overhead from serde deserialization)
///
/// Type T ONLY affects:
/// - How each row is deserialized when consumed
/// - Error messages (type name included)
///
/// Escape hatch:
/// Use `query::<serde_json::Value>(...)` for debugging
/// or forward-compatibility without code changes.
```

---

## Documents Updated

### 1. `.phases/phase-8-2-typed-streaming.md` (550+ lines)
**Updates**:
- Objective: Added "consumer-side only" emphasis
- New section: "⚠️ CRITICAL: Typing is Consumer-Side Only" (detailed breakdown)
- QueryBuilder comments: Added "type T does NOT affect SQL" annotations
- TypedJsonStream: Added explicit constraint documentation
- New section: "🚪 Escape Hatch: Always Support `query::<Value>()`" with 5 use cases
- Implementation notes: Updated to emphasize "ONLY" and "consumer-side"

### 2. `PHASE_8_2_PLANNING_SUMMARY.md` (350+ lines)
**Updates**:
- Overview: Added example showing SQL is unaffected by T
- Key Features: Added escape hatch to features list
- New section: "⚠️ CRITICAL DESIGN CONSTRAINT" with detailed breakdown
- Architecture: Updated to show SQL is generated same way for all T

### 3. `.phases/README_PHASE_8_2.md` (250+ lines)
**Updates**:
- What is Phase 8.2: Added consumer-side emphasis
- Critical Design Constraint: Explicit section showing what T affects
- Escape Hatch: Added as first-class feature
- Key Implementation Notes: Added explicit boundaries
- Anti-Patterns: Expanded from 5 to 6 detailed patterns with wrong/right examples
- Each pattern includes code examples showing what to forbid

### 4. `PHASE_8_2_CRITICAL_CONSTRAINTS.md` (NEW - 400+ lines)
**Content**:
- Explains the issue being prevented (drift into typed query planning)
- Rule 1: Typing is consumer-side only (with examples)
- Rule 2: Escape hatch is first-class (with use cases)
- Critical anti-patterns (5 detailed examples with code)
- PR review checklist (8 items to verify before merge)
- Future-proofing (how to reject problematic PRs)
- Documentation requirements (mandatory comments and rustdoc)
- Success metric (constraint documentation is enforcement mechanism)

---

## Key Phrases Added (Searchable)

These phrases now appear consistently throughout all documents:

| Phrase | Count | Purpose |
|--------|-------|---------|
| "consumer-side only" | 15+ | Emphasize typing boundary |
| "Type T does NOT affect" | 10+ | Explicit negations |
| "Type T ONLY affects" | 10+ | Explicit affirmations |
| "escape hatch" | 12+ | Emphasize Value support |
| "first-class feature" | 3+ | Value is not fallback |
| "typed query planning" | 5+ | Anti-pattern name |
| "SQL is identical" | 8+ | Core guarantee |
| "poll_next()" | 6+ | Boundary point |

---

## Enforcement Mechanisms

### 1. Code Comments
Every implementation file must include:
```rust
/// Type T is **consumer-side only**...
```

### 2. Documentation
Every user guide must state:
> Typing does NOT affect SQL, filtering, ordering, or wire protocol

### 3. PR Review Checklist
Every Phase 8.2 PR must verify:
- [ ] Type isolation verified: SQL is identical for all T
- [ ] No "typed query planning"
- [ ] Predicates are JSON-based
- [ ] Escape hatch tested: query::<Value>() works
- [ ] Documentation explicit

### 4. Tests
Future PRs proposing optimizations will be rejected if:
- They make SQL conditional on T
- They special-case Value
- They optimize for specific types

---

## Why This Matters

Without explicit boundaries, the natural question arises:

> "Now that we have type T, can we use it to optimize SQL?"

This leads to:
1. Type-conditional SQL generation
2. Query planning based on T
3. Gradual expansion of scope
4. Eventually: full ORM territory

**Explicit constraints prevent this slide.**

The documents now make it impossible to miss:
- Type is consumer-side only
- SQL is identical for all T
- Value is first-class, not a fallback
- Escape hatch is a feature, not a bug workaround

---

## Files for Reference

### Critical Reading Order

1. **First**: `PHASE_8_2_CRITICAL_CONSTRAINTS.md` (understand the issue)
2. **Second**: `.phases/README_PHASE_8_2.md` (quick reference with examples)
3. **Third**: `.phases/phase-8-2-typed-streaming.md` (implementation details)
4. **Reference**: `PHASE_8_2_PLANNING_SUMMARY.md` (executive overview)

### For Code Reviewers

Use this checklist from `PHASE_8_2_CRITICAL_CONSTRAINTS.md`:
```
Before Merge:
- [ ] Type isolation verified: SQL is identical for all T
- [ ] No "typed query planning": No conditionals based on T
- [ ] Predicates are JSON-based: where_rust takes &Value
- [ ] Filtering before deserialization: Correct pipeline order
- [ ] Type names in errors: Error messages include type_name
- [ ] Escape hatch tested: query::<Value>() works identically
- [ ] No special cases for Value: Value is treated like any other type
- [ ] Documentation explicit: Rustdoc mentions boundary constraints
- [ ] Comments in code: Key locations have constraint docs
```

### For Future Contributors

Read `PHASE_8_2_CRITICAL_CONSTRAINTS.md` before proposing changes to typed streaming.

---

## Impact on Implementation

### Phase 8.2.1-8.2.7: No Changes to Plan

The 7-phase implementation plan remains unchanged. These updates are **constraints on implementation**, not changes to what's being built.

### During Implementation: Enforce Boundaries

Each phase should verify:
- No SQL conditionals based on T
- Predicates stay JSON-based
- Deserialization happens at poll_next() only
- Error messages include type names
- Tests verify escape hatch works

### During Code Review: Use Checklist

Every PR should be reviewed against `PHASE_8_2_CRITICAL_CONSTRAINTS.md` checklist.

---

## Success Criteria (Updated)

Phase 8.2 succeeds when:

✅ Type-safe streaming works
✅ All existing code works unchanged
✅ Error messages are clear
✅ Performance is < 2% overhead
✅ Escape hatch (Value) always works
✅ **NEW**: No type-conditional SQL
✅ **NEW**: No special-case handling for Value
✅ **NEW**: Constraints are documented in code
✅ **NEW**: PR reviewers can enforce boundaries

---

## Reference Implementation

The constraint section in `.phases/phase-8-2-typed-streaming.md` provides the correct template:

```rust
impl<T: DeserializeOwned> QueryBuilder<T> {
    /// Add SQL WHERE predicate (type T does NOT affect SQL)
    pub fn where_sql(mut self, predicate: impl Into<String>) -> Self { ... }

    /// Add Rust-side predicate on JSON (type T does NOT affect filtering)
    pub fn where_rust<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + Send + 'static
    { ... }

    /// Set ORDER BY (type T does NOT affect ordering)
    pub fn order_by(mut self, order: impl Into<String>) -> Self { ... }

    /// Execute query.
    ///
    /// Type T ONLY affects consumer-side deserialization at poll_next().
    /// SQL, filtering, ordering, and wire protocol are identical regardless of T.
    pub async fn execute(self) -> Result<Box<dyn Stream<Item = Result<T>> + Unpin>> {
        let sql = self.build_sql()?;  // ← Same for all T
        let stream = self.client.execute_query(&sql, self.chunk_size).await?;
        // ...
    }
}
```

Follow this pattern: constraints documented in rustdoc, boundaries clear in comments.

---

## Conclusion

Phase 8.2 planning now includes explicit, enforced constraints that prevent drift from the core design principle:

> **fraiseql-wire is a JSON query pipe, not a query planner.**
>
> Typing is a consumer convenience, not a SQL optimization mechanism.

These constraints are documented, searchable, and enforceable through code review.

---

**Status**: ✅ All documents updated and cross-checked
**Ready for**: Implementation with clear boundaries
**Enforcement**: Via code review checklist and documentation
