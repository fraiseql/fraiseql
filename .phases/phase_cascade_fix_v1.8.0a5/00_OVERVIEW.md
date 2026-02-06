# Phase: CASCADE Fix for v1.8.0-alpha.5

**Phase ID:** `phase_cascade_fix_v1.8.0a5`
**Target Release:** FraiseQL v1.8.0-alpha.5
**Estimated Duration:** 4-6 hours
**Complexity:** Low
**Type:** Bug Fix (Parser Update)

---

## 🎯 Goal

Fix the CASCADE nesting bug where CASCADE data appears inside entity objects instead of at the success wrapper level.

**Current Behavior (Bug):**

```json
{
  "createAllocation": {
    "allocation": {
      "cascade": { /* CASCADE HERE - WRONG! */ }
    },
    "cascade": {}  // Empty
  }
}
```

**Expected Behavior (After Fix):**

```json
{
  "createAllocation": {
    "allocation": { /* No cascade field */ },
    "cascade": { /* CASCADE HERE - CORRECT! */ }
  }
}
```

---

## 📊 Problem Statement

### Root Cause

FraiseQL Rust doesn't parse PrintOptim's new 8-field `mutation_response` composite type. It treats the composite as a JSON blob, causing incorrect field mapping.

**PrintOptim Structure (Already Migrated):**

```sql
CREATE TYPE app.mutation_response AS (
    status          TEXT,     -- Position 1
    message         TEXT,     -- Position 2
    entity_id       TEXT,     -- Position 3
    entity_type     TEXT,     -- Position 4
    entity          JSONB,    -- Position 5
    updated_fields  TEXT[],   -- Position 6
    cascade         JSONB,    -- Position 7 ← EXPLICIT CASCADE FIELD!
    metadata        JSONB     -- Position 8
);
```

**FraiseQL Rust (Current):**

- Expects old 6-field format OR simple JSONB
- Doesn't know about Position 7 (cascade field)
- Doesn't extract CASCADE from correct position

### Impact

- ❌ GraphQL schema violation (CASCADE in wrong location)
- ❌ Frontend cache updates broken (CASCADE not accessible)
- ❌ Spec compliance failure (graphql-cascade spec violated)

---

## ✅ Success Criteria

### Functional Requirements

- [ ] CASCADE appears at success wrapper level (e.g., `CreateAllocationSuccess.cascade`)
- [ ] CASCADE does NOT appear in entity object (e.g., `allocation.cascade` should not exist)
- [ ] All 8 fields from `mutation_response` composite type are parsed correctly
- [ ] Backward compatibility maintained (simple format still works)

### Non-Functional Requirements

- [ ] Zero PrintOptim backend changes required
- [ ] All existing tests pass
- [ ] New tests added for 8-field composite parsing
- [ ] Performance: < 10μs overhead for parsing (negligible)

### Testing Requirements

- [ ] Unit tests for composite type parsing
- [ ] Integration tests with PrintOptim mutations
- [ ] CASCADE location verification tests
- [ ] Backward compatibility tests (simple format)

---

## 🏗️ Implementation Strategy

### Approach: Add 8-Field Composite Type Parser

**Why This Works:**

1. PrintOptim already migrated to 8-field format (~70 functions)
2. CASCADE already at Position 7 (explicit field)
3. entity_type already at Position 4
4. Zero database changes needed

**What We're Adding:**

- New module: `postgres_composite.rs` (~80 lines)
- Parser for 8-field `mutation_response` composite type
- Fallback to simple format (backward compatibility)

---

## 📁 Files to Modify

### New Files (Create)

1. **`fraiseql_rs/src/mutation/postgres_composite.rs`**
   - Parser for 8-field composite type
   - Struct: `PostgresMutationResponse`
   - ~80 lines of code

### Modified Files

2. **`fraiseql_rs/src/mutation/mod.rs`**
   - Add import for `postgres_composite`
   - Update `build_mutation_response()` to try new parser first
   - ~5 lines changed

3. **`fraiseql_rs/src/mutation/tests.rs`**
   - Add tests for 8-field parsing
   - Add CASCADE extraction tests
   - ~100 lines added

### Documentation

4. **`CHANGELOG.md`**
   - Document bug fix
   - Breaking changes: None
   - New features: Support for 8-field mutation_response

---

## 🚀 Implementation Phases

### Phase 1: Create Parser Module (1-2 hours)

**Task:** Create `postgres_composite.rs`

**Steps:**

1. Create new file: `fraiseql_rs/src/mutation/postgres_composite.rs`
2. Define `PostgresMutationResponse` struct with 8 fields
3. Implement `from_json()` parser
4. Implement `to_mutation_result()` converter
5. Add error handling and documentation

**Deliverable:** Working parser module

### Phase 2: Integrate Parser (30 min)

**Task:** Update entry point to use new parser

**Steps:**

1. Add `mod postgres_composite;` to `mod.rs`
2. Update `build_mutation_response()`:
   - Try 8-field parser first
   - Fallback to simple format on parse error
3. Test compilation

**Deliverable:** Integration complete, compiles successfully

### Phase 3: Add Tests (1-2 hours)

**Task:** Comprehensive test coverage

**Steps:**

1. Unit tests for composite type parsing
2. CASCADE extraction tests
3. entity_type resolution tests
4. Backward compatibility tests
5. Integration test with real mutation

**Deliverable:** All tests passing

### Phase 4: Validation (1 hour)

**Task:** Test with PrintOptim

**Steps:**

1. Build FraiseQL locally
2. Run PrintOptim test suite
3. Verify CASCADE location in responses
4. Check performance (no regression)

**Deliverable:** PrintOptim tests passing, CASCADE in correct location

---

## 🧪 Testing Strategy

### Unit Tests (Rust)

```rust
// fraiseql_rs/src/mutation/tests.rs

#[test]
fn test_parse_8field_composite_basic() {
    let json = r#"{
        "status": "created",
        "message": "Success",
        "entity_id": "uuid",
        "entity_type": "Allocation",
        "entity": {"id": "uuid"},
        "updated_fields": ["location_id"],
        "cascade": {"updated": []},
        "metadata": {}
    }"#;

    let result = PostgresMutationResponse::from_json(json).unwrap();
    assert_eq!(result.status, "created");
    assert!(result.cascade.is_some());
}

#[test]
fn test_cascade_at_position_7() {
    // Verify CASCADE extracted from Position 7, not metadata
}

#[test]
fn test_backward_compat_simple_format() {
    // Ensure simple format still works
}
```

### Integration Tests (Python)

```python
# tests/test_cascade_fix.py

async def test_cascade_at_success_level():
    """Verify CASCADE at wrapper level, not in entity"""
    result = await execute_mutation(...)

    # CASCADE at success level
    assert "cascade" in result["createAllocation"]

    # CASCADE NOT in entity
    assert "cascade" not in result["createAllocation"]["allocation"]
```

---

## 📋 Checklist

### Pre-Implementation

- [ ] Review design document
- [ ] Understand 8-field composite type structure
- [ ] Set up development environment
- [ ] Create feature branch: `fix/cascade-nesting-v1.8.0a5`

### Implementation

- [ ] Create `postgres_composite.rs` module
- [ ] Add 8-field struct definition
- [ ] Implement JSON parser
- [ ] Implement converter to `MutationResult`
- [ ] Update `mod.rs` entry point
- [ ] Add comprehensive tests
- [ ] Update CHANGELOG.md

### Testing

- [ ] Run Rust unit tests: `cargo test`
- [ ] Run Python integration tests: `pytest tests/`
- [ ] Test with PrintOptim mutations
- [ ] Verify CASCADE location
- [ ] Check backward compatibility

### Release

- [ ] Bump version to v1.8.0-alpha.5
- [ ] Create PR with clear description
- [ ] Get code review
- [ ] Merge to main
- [ ] Publish to PyPI
- [ ] Update PrintOptim dependency

---

## 🎯 Acceptance Criteria

### Must Have (P0)

- [x] CASCADE appears at success wrapper level
- [x] CASCADE does NOT appear in entity
- [x] All existing tests pass
- [x] Zero PrintOptim backend changes
- [x] Backward compatible

### Should Have (P1)

- [ ] Performance: < 10μs parsing overhead
- [ ] Comprehensive test coverage
- [ ] Clear error messages

### Nice to Have (P2)

- [ ] Benchmarks for parsing performance
- [ ] Documentation with examples

---

## 🚨 Risks & Mitigations

### Risk 1: Backward Compatibility Break

**Risk:** Simple format responses break after changes
**Likelihood:** Low
**Impact:** Medium

**Mitigation:**

- Try 8-field parser first, fallback to simple format
- Add tests for both formats
- Test with existing FraiseQL users

### Risk 2: Performance Regression

**Risk:** New parsing logic is slower
**Likelihood:** Very Low
**Impact:** Low

**Mitigation:**

- Simple struct deserialization (very fast)
- Benchmark before/after
- Monitor in production

---

## 📊 Timeline

```
Day 1 (4-6 hours):
├─ 09:00-10:30  Phase 1: Create parser module
├─ 10:30-11:00  Phase 2: Integrate parser
├─ 11:00-12:30  Phase 3: Add tests
└─ 12:30-13:30  Phase 4: Validation

Release: Same day or next morning
```

---

## 📚 References

- **Design Document:** `/tmp/fraiseql_mutation_pipeline_design.md`
- **GraphQL CASCADE Spec:** `~/code/graphql-cascade/`
- **PrintOptim Migration:** `/home/lionel/code/printoptim_backend_manual_migration/.phases/phase_fraiseql_mutation_response/`
- **Bug Report:** `/tmp/fraiseql_v1.8.0a4_test_report.md`

---

## 🎉 Expected Outcome

After this phase:

✅ CASCADE bug fixed
✅ FraiseQL v1.8.0-alpha.5 released
✅ PrintOptim can upgrade and get correct CASCADE behavior
✅ GraphQL schema compliance restored
✅ Frontend cache updates working

**Next Phase:** Performance optimization (if needed) in v1.8.1
