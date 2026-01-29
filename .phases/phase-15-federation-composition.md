# Phase 15: Federation Composition Validation

## Objective

Implement comprehensive federation composition validation and multi-subgraph coordination testing. Validate the complete GraphQL federation flow: schema composition → directive validation (@requires/@provides) → distributed query execution across multiple subgraphs using the saga system.

## Context

**Completed (Cycles 12-14):**
- ✅ Saga E2E Testing (25 tests)
- ✅ Saga Performance & Stress Testing (stress + benchmarks)
- ✅ Saga Chaos Testing (18 non-deterministic failure tests)
- **Total**: 68+ saga system tests validating distributed transactions

**Ready For:**
- Federation composition (schema merging, directive validation)
- Multi-subgraph query coordination using saga system
- Cross-subgraph dependency resolution
- Apollo Router federation certification

---

## Success Criteria

- [ ] 25-30 federation composition validation tests created
- [ ] Schema composition tests (@requires/@provides directives)
- [ ] Multi-subgraph query coordination tests (3-5 subgraphs)
- [ ] Cross-subgraph mutation tests using saga system
- [ ] Circular dependency detection
- [ ] Type consistency validation across subgraphs
- [ ] Query plan optimization tests
- [ ] All tests passing: `cargo test federation_composition`
- [ ] Zero clippy warnings
- [ ] Code properly formatted

---

## Gap Analysis

### Already Tested (Cycles 12-14)
✅ Saga forward execution (multi-step, concurrent)
✅ Saga failure scenarios (compensation, recovery)
✅ Saga performance (1000+ concurrent sagas)
✅ Saga chaos (non-deterministic failures)
✅ State machine transitions
✅ Crash recovery

### Missing (Cycle 15 Will Cover)
❌ Federation schema composition
❌ @requires/@provides directive validation
❌ Multi-subgraph query execution
❌ Cross-subgraph mutation coordination
❌ Type consistency checking
❌ Query plan generation and optimization
❌ Circular dependency detection
❌ Subgraph integration scenarios

---

## Test File

**Path:** `crates/fraiseql-core/tests/federation_composition_validation.rs`

**Purpose:** Comprehensive federation composition and multi-subgraph coordination tests

**Structure:**

```rust
//! Cycle 15: Federation Composition Validation
//!
//! Tests schema composition, directive validation, and multi-subgraph
//! query/mutation coordination using the saga system for transactions.
//!
//! ## Test Categories (25-30 tests)
//!
//! - Schema Composition (4 tests)
//! - Directive Validation (4 tests)
//! - Query Planning (3 tests)
//! - Multi-Subgraph Queries (4 tests)
//! - Cross-Subgraph Mutations (4 tests)
//! - Dependency Resolution (3 tests)
//! - Type Consistency (2 tests)
//! - Error Scenarios (2 tests)

mod harness {
    // ComposedSchema - Result of merging subgraph schemas
    // SubgraphSchema - Individual subgraph schema definition
    // DirectiveValidator - @requires/@provides validation
    // QueryPlanner - Query plan generation
    // SagaCoordinator - Mutation coordination across subgraphs
    // TypeRegistry - Type consistency tracking
}

// ============================================================================
// Category 1: Schema Composition (4 tests)
// ============================================================================

#[test]
fn test_compose_3_subgraphs_single_type()

#[test]
fn test_compose_5_subgraphs_overlapping_types()

#[test]
fn test_compose_validates_type_definitions()

#[test]
fn test_compose_merges_fields_from_multiple_subgraphs()

// ============================================================================
// Category 2: Directive Validation (4 tests)
// ============================================================================

#[test]
fn test_requires_directive_validates_dependencies()

#[test]
fn test_provides_directive_validates_capabilities()

#[test]
fn test_invalid_requires_reference_rejected()

#[test]
fn test_circular_requires_detected()

// ============================================================================
// Category 3: Query Planning (3 tests)
// ============================================================================

#[test]
fn test_query_plan_single_subgraph()

#[test]
fn test_query_plan_multi_subgraph_joins()

#[test]
fn test_query_plan_optimizes_subgraph_order()

// ============================================================================
// Category 4: Multi-Subgraph Queries (4 tests)
// ============================================================================

#[test]
fn test_query_users_from_users_subgraph()

#[test]
fn test_query_user_with_orders_cross_subgraph()

#[test]
fn test_query_user_orders_products_3_subgraphs()

#[test]
fn test_query_with_filters_across_subgraphs()

// ============================================================================
// Category 5: Cross-Subgraph Mutations (4 tests)
// ============================================================================

#[test]
fn test_create_user_and_order_coordinated_saga()

#[test]
fn test_create_user_order_payment_3_subgraph_saga()

#[test]
fn test_mutation_rollback_on_second_subgraph_failure()

#[test]
fn test_concurrent_mutations_different_users()

// ============================================================================
// Category 6: Dependency Resolution (3 tests)
// ============================================================================

#[test]
fn test_resolve_entity_references()

#[test]
fn test_resolve_nested_entity_references()

#[test]
fn test_resolve_with_type_extensions()

// ============================================================================
// Category 7: Type Consistency (2 tests)
// ============================================================================

#[test]
fn test_type_mismatch_detected_across_subgraphs()

#[test]
fn test_conflicting_field_definitions_rejected()

// ============================================================================
// Category 8: Error Scenarios (2 tests)
// ============================================================================

#[test]
fn test_subgraph_unreachable_during_query()

#[test]
fn test_malformed_subgraph_schema_rejected()
```

---

## TDD Implementation Phases

### RED Phase (4-5 hours)

**Step 1: Create test file skeleton**
```bash
touch crates/fraiseql-core/tests/federation_composition_validation.rs
```

**Step 2: Add harness module**
- Create `SubgraphSchema` struct
- Create `ComposedSchema` struct with composition logic
- Create `DirectiveValidator` for @requires/@provides validation
- Create `QueryPlanner` for query plan generation
- Create `TypeRegistry` for type consistency checking
- Create `FederationTestHarness` to tie it all together

**Step 3: Write 25-30 failing tests**

**Composition Tests (4)**
- Schema merge with 3 subgraphs
- Schema merge with 5 subgraphs (overlapping types)
- Type validation across subgraphs
- Field merging from multiple subgraphs

**Directive Tests (4)**
- @requires directive validation
- @provides directive validation
- Invalid @requires reference rejection
- Circular @requires detection

**Query Planning Tests (3)**
- Single subgraph query plan
- Multi-subgraph query plan with joins
- Query plan optimization

**Query Tests (4)**
- Single subgraph query execution
- Cross-subgraph query (user → orders)
- 3-subgraph query (user → orders → products)
- Query with filters across subgraphs

**Mutation Tests (4)**
- Create user and order via saga
- Create user, order, payment (3 subgraphs) via saga
- Rollback on second subgraph failure
- Concurrent mutations

**Dependency Tests (3)**
- Entity reference resolution
- Nested entity reference resolution
- Type extension resolution

**Consistency Tests (2)**
- Type mismatch detection
- Conflicting field definitions

**Error Tests (2)**
- Subgraph unreachable
- Malformed schema rejection

**Expected:** All tests fail (harness doesn't exist yet)

**Verification:**
```bash
cargo test --test federation_composition_validation 2>&1 | grep "test result"
```

---

### GREEN Phase (3-4 hours)

**Step 1: Implement harness structures**

```rust
pub struct SubgraphSchema {
    pub name: String,
    pub types: HashMap<String, TypeDef>,
    pub directives: Vec<DirectiveUsage>,
}

pub struct ComposedSchema {
    pub subgraphs: Vec<SubgraphSchema>,
    pub merged_types: HashMap<String, MergedTypeDef>,
    pub validation_errors: Vec<ValidationError>,
}

impl ComposedSchema {
    pub fn compose(subgraphs: Vec<SubgraphSchema>) -> Result<Self> {
        // Merge types from all subgraphs
        // Validate directives
        // Check type consistency
        // Return composed schema or errors
    }
}

pub struct DirectiveValidator {
    pub schema: ComposedSchema,
}

impl DirectiveValidator {
    pub fn validate_requires(&self) -> Vec<ValidationError> { ... }
    pub fn validate_provides(&self) -> Vec<ValidationError> { ... }
}

pub struct QueryPlanner {
    pub schema: ComposedSchema,
}

impl QueryPlanner {
    pub fn plan_query(&self, query: &str) -> Result<QueryPlan> { ... }
}
```

**Step 2: Implement minimal test logic**

For each test category:
1. Create sample subgraph schemas
2. Call compose/validate/plan operations
3. Verify expected results or errors
4. Assert no panics

**Step 3: Integrate with saga system**

For mutation tests:
1. Create saga with steps for each subgraph
2. Execute saga across subgraphs
3. Verify all subgraphs updated or all rolled back

**Expected:** All 25-30 tests pass

**Verification:**
```bash
cargo test --test federation_composition_validation
cargo test --test federation_saga_e2e  # Verify no regressions
```

---

### REFACTOR Phase (2-3 hours)

**Code Organization:**
- Extract schema builders into helper functions
- Create scenario builders for common compositions
- Extract directive validation logic
- Extract query planning logic
- Create assertion helpers

**Documentation:**
- Add module-level docs
- Document each test category
- Add examples of composition patterns
- Document supported/unsupported directive combinations

**Example Refactoring:**

```rust
// Helper for creating test subgraph schemas
fn build_users_subgraph() -> SubgraphSchema {
    SubgraphSchema {
        name: "users".to_string(),
        types: vec![
            ("User", vec!["id: ID!", "name: String!"])
        ].into_iter().collect(),
        directives: vec![],
    }
}

fn build_orders_subgraph() -> SubgraphSchema {
    SubgraphSchema {
        name: "orders".to_string(),
        types: vec![
            ("Order", vec!["id: ID!", "userId: ID!", "total: Float!"])
        ].into_iter().collect(),
        directives: vec![
            DirectiveUsage::Requires("Order.userId", "User.id")
        ],
    }
}

// Helper for composition assertions
fn assert_type_exists(schema: &ComposedSchema, type_name: &str) {
    assert!(
        schema.merged_types.contains_key(type_name),
        "Type {} should exist in composed schema",
        type_name
    );
}

fn assert_no_validation_errors(schema: &ComposedSchema) {
    assert!(
        schema.validation_errors.is_empty(),
        "Should have no validation errors, got: {:?}",
        schema.validation_errors
    );
}
```

---

### CLEANUP Phase (1 hour)

**Linting:**
```bash
cargo fmt
cargo clippy --test federation_composition_validation -- -D warnings
```

**Documentation:**
- Polish all test documentation
- Add federation composition guide
- Document composition patterns
- Add troubleshooting section

**Final Verification:**
```bash
# Composition tests
cargo test --test federation_composition_validation

# Verify no regressions in saga tests
cargo test --test federation_saga_e2e
cargo test --test federation_saga_chaos_test
cargo test --test federation_saga_stress_test

# Full federation test suite
cargo test federation

# Clippy check
cargo clippy --all-targets -- -D warnings
```

---

## Key Design Decisions

### 1. In-Memory Composition
**Decision:** Implement composition as in-memory operations, no persistence

**Rationale:**
- Tests composition logic, not persistence
- Fast execution
- Deterministic results

### 2. Schema Validation Points
**Decision:** Validate at composition time, not query time

**Rationale:**
- Catch errors early
- Match GraphQL federation spec
- Match Apollo Router behavior

### 3. Saga Integration for Mutations
**Decision:** Use existing saga system for cross-subgraph mutations

**Rationale:**
- Tests integration with saga system
- Validates distributed transaction semantics
- Reuses battle-tested saga code

### 4. Query Planning Simplicity
**Decision:** Start with simple join ordering, not full optimization

**Rationale:**
- Establish baseline
- Can optimize in later cycle
- Focus on correctness first

---

## Critical Files to Reference

1. **`crates/fraiseql-core/tests/federation_saga_e2e.rs`**
   - Copy harness patterns
   - Reuse SagaOrchestrator for mutation tests

2. **`crates/fraiseql-core/src/federation/`**
   - Review existing composition logic if any
   - Reference directive handling

3. **GraphQL Federation Spec**
   - Reference for composition semantics
   - Reference for @requires/@provides

---

## Success Criteria

### Functional
- [ ] 25-30 composition tests pass (<5s total)
- [ ] All schema composition scenarios covered
- [ ] Directive validation comprehensive
- [ ] Query planning working
- [ ] Multi-subgraph mutations via saga
- [ ] All error cases tested

### Quality
- [ ] Zero clippy warnings
- [ ] Code properly formatted
- [ ] Comprehensive documentation
- [ ] Clear test names and assertions

### Coverage
- [ ] Schema composition: 4 tests
- [ ] Directive validation: 4 tests
- [ ] Query planning: 3 tests
- [ ] Multi-subgraph queries: 4 tests
- [ ] Cross-subgraph mutations: 4 tests
- [ ] Dependency resolution: 3 tests
- [ ] Type consistency: 2 tests
- [ ] Error handling: 2 tests

---

## Expected Outcomes

After Cycle 15 completion:
- **25-30 new composition validation tests**
- **Federation composition validation harness**
- **Multi-subgraph query coordination tests**
- **Cross-subgraph mutation tests using saga system**
- **Confidence in federation composition logic**

**Test Summary:**
- Composition tests: 25-30 (this cycle)
- Saga tests: 68+ (cycles 12-14)
- **Total federation tests: 93-98**

---

## Verification Checklist

### Pre-Commit
- [ ] All 25-30 composition tests pass
- [ ] No clippy warnings
- [ ] Code formatted
- [ ] Saga tests still pass (no regressions)
- [ ] Documentation complete

### Post-Commit
- [ ] Tests run successfully in CI
- [ ] No flaky test failures
- [ ] Composition guide added to docs
- [ ] Team understands composition validation

---

## Timeline Estimate

- **RED Phase**: 4-5 hours (test creation)
- **GREEN Phase**: 3-4 hours (implementation)
- **REFACTOR Phase**: 2-3 hours (organization)
- **CLEANUP Phase**: 1 hour (final verification)

**Total**: 10-13 hours (1-2 development days)

---

## Related Cycles

**Previous (Completed):**
- Cycle 12: Saga E2E Testing (foundation)
- Cycle 13: Saga Performance & Stress Testing
- Cycle 14: Saga Chaos Testing

**Upcoming (Potential):**
- Cycle 16: Apollo Router Integration
- Cycle 17: Database Backend Support
- Cycle 18: Production Observability

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Composition complexity | High | Start simple, iterate |
| Directive validation gaps | Medium | Reference GraphQL spec |
| Query plan bugs | Medium | Comprehensive test cases |
| Mutation saga integration | Medium | Reuse proven saga patterns |
| Schema mocking complexity | Low | Keep schemas minimal for tests |

---

## Commit Strategy

```
feat(federation): Cycle 15 - Federation Composition Validation

## Summary

Implement federation composition validation and multi-subgraph query
coordination. Validate schema merging, directive validation (@requires/@provides),
and cross-subgraph mutation coordination using saga system.

## Changes

- Add federation_composition_validation.rs with 25-30 tests
- Implement ComposedSchema for schema merging
- Implement DirectiveValidator for @requires/@provides
- Implement QueryPlanner for query plan generation
- Integrate with SagaOrchestrator for mutation coordination
- Comprehensive composition and directive validation

## Test Coverage

- Schema composition: 4 tests
- Directive validation: 4 tests
- Query planning: 3 tests
- Multi-subgraph queries: 4 tests
- Cross-subgraph mutations: 4 tests
- Dependency resolution: 3 tests
- Type consistency: 2 tests
- Error scenarios: 2 tests

## Verification

✅ 25-30 composition tests pass
✅ No regressions in saga tests (68+ tests pass)
✅ Zero clippy warnings
✅ Code properly formatted

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>
```

---

**Status**: Ready for implementation
**Created**: 2026-01-29
**Branch**: feature/phase-1-foundation
**Estimated Duration**: 1-2 development days

---

## Decision Point

**Should we proceed with Cycle 15: Federation Composition Validation?**

This cycle:
- ✅ Builds directly on saga testing (cycles 12-14)
- ✅ Moves toward full GraphQL federation support
- ✅ Prepares for Apollo Router integration
- ✅ Validates federation-specific scenarios
- ✅ Stays focused on testing infrastructure

**Alternative directions** (for future cycles):
- Cycle 15: Database Backend Support (PostgreSQL, MySQL, SQLite, SQL Server)
- Cycle 15: Production Observability (metrics, tracing, structured logging)
- Cycle 15: Performance Optimization (query planning, caching optimization)
