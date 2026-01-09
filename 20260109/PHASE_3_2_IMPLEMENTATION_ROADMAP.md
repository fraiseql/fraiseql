# Phase 3.2: Query Execution & Parameter Binding - Implementation Roadmap

**Updated**: January 9, 2026
**Status**: Tasks 1-5 Complete, Tasks 6-8 Planned
**Focus**: Type-safe parameter binding for PostgreSQL function calls

---

## Phase 3.2 Overview

Phase 3.2 implements type-safe query execution with parameter binding for the FraiseQL Rust pipeline.

### Architecture
```
Python Application
        ↓
FraiseQL Framework (Python)
        ↓
Rust Pipeline (fraiseql_rs)
        ├── Parameter Validation (type-safe QueryParam)
        ├── PostgreSQL Function Calls
        └── Query Execution
        ↓
PostgreSQL (Database)
        ↓
JSON Results (JSONB)
        ↓
Response
```

### Key Principle
**All mutations use PostgreSQL functions**, not direct INSERT/UPDATE/DELETE statements.

---

## Completed Tasks (✅ DONE)

### Task 1: Define QueryParam & Result Types ✅
**Status**: Complete (Previous Session)
**File**: `fraiseql_rs/src/db/types.rs`

Defined:
- `QueryParam` enum (10 variants: Null, Bool, Int, BigInt, Float, Double, Text, Json, Timestamp, Uuid)
- `QueryResult` struct for query results
- `ExecuteResult` struct for mutation results
- Conversion traits (`From<i32>`, `From<String>`, etc.)

---

### Task 2: Extend PoolBackend Trait ✅
**Status**: Complete (Previous Session)
**File**: `fraiseql_rs/src/db/pool/traits.rs`

Extended trait with:
- `execute_query(sql, params)` - parameterized SELECT
- `execute_mutation(sql, params)` - parameterized mutations
- `begin_transaction()` - start transaction
- `commit_transaction(tx_id)` - commit
- `rollback_transaction(tx_id)` - rollback
- `health_check()` - connection health

Enhanced error types:
- `PoolError::InvalidParameter { param_index, reason }`
- `PoolError::QueryExecution(String)`
- `PoolError::TransactionFailed(String)`

---

### Task 3: Implement Parameter Binding Utilities ✅
**Status**: Complete (Previous Session)
**File**: `fraiseql_rs/src/db/parameter_binding.rs` (450+ LOC)

Functions:
- `prepare_parameters(params)` - validate all parameters
- `validate_parameter(index, param)` - validate single parameter
- `format_parameter(param)` - safe debugging output
- `count_placeholders(sql)` - count $1, $2, etc.
- `validate_parameter_count(sql, params)` - match validation

Validation:
- NaN/Infinity detection for floats
- Type-specific checks
- Placeholder count matching
- 14 comprehensive unit tests

---

### Task 4: Implement Parameter Binding in ProductionPool ✅
**Status**: Complete (Previous Session)
**File**: `fraiseql_rs/src/db/pool_production.rs`

Implementation:
- `execute_query_with_params(sql, params)` - main parameterized query method
- Parameter validation before execution
- Deadlock retry logic
- JSONB extraction (FraiseQL CQRS pattern)
- 4 unit tests + parameter validation tests

---

### Task 5: Implement Transaction Support with Parameter Binding ✅
**Status**: Complete (Current Session)
**File**: `fraiseql_rs/src/db/transaction.rs` (+142 LOC)

New methods:
- `query_with_params(sql, params)` - SELECT in transaction
- `execute_with_params(sql, params)` - PostgreSQL function calls in transaction

Features:
- Timeout enforcement
- Active status checks
- Parameter validation
- 4 integration tests

**Key Design**: These methods are for calling PostgreSQL functions, NOT direct INSERT/UPDATE/DELETE.

---

## Pending Tasks (⏳ TODO)

### Task 6: PostgreSQL Function Mutation Integration ⏳
**Estimated**: 2-4 hours
**Status**: Not Started
**Objective**: Document and provide examples for PostgreSQL function-based mutations

What to implement:
1. **Documentation**: Pattern for calling PostgreSQL functions
   ```rust
   // Example: Create user via PostgreSQL function
   let params = vec![
       QueryParam::Text("John".to_string()),
       QueryParam::Text("john@example.com".to_string()),
   ];

   let result = pool.execute_query_with_params(
       "SELECT create_user($1, $2) RETURNING to_jsonb(*)",
       &params
   ).await?;
   ```

2. **Helper patterns**:
   - Single-result functions (create, get)
   - Multi-result functions (list, search)
   - Mutation functions (update, delete via functions)
   - Transaction-wrapped mutations

3. **Error handling**:
   - PostgreSQL constraint violations
   - Function-specific errors
   - Parameter validation errors

4. **Integration examples**:
   - Real PostgreSQL function calls
   - Error scenarios
   - Transaction patterns

### Task 7: Full Rust Test Suite Verification ⏳
**Estimated**: 1-2 hours
**Status**: Not Started
**Objective**: Verify zero regressions and performance baseline

What to test:
1. Run all Rust unit tests
   ```bash
   cargo test --lib
   ```

2. Run all integration tests (if available)
   ```bash
   cargo test --test '*'
   ```

3. Performance baseline measurements:
   - Query execution time
   - Parameter validation overhead
   - Deadlock retry performance

4. Regression detection:
   - Existing functionality still works
   - No new warnings
   - Documentation builds

### Task 8: Phase 3.2 Summary & Documentation ⏳
**Estimated**: 1-2 hours
**Status**: Not Started
**Objective**: Complete Phase 3.2 documentation and plan next phase

What to create:
1. **Implementation Summary**:
   - What was built
   - Why each component matters
   - Design decisions and tradeoffs

2. **Usage Guide**:
   - How to use parameter binding
   - Transaction patterns
   - Error handling
   - Performance considerations

3. **Architecture Diagram**:
   - Data flow (Python → Rust → PostgreSQL)
   - Type conversions
   - Error propagation

4. **Next Phase Planning** (Phase 3d):
   - Hot path optimization opportunities
   - Performance benchmarking strategy
   - Timeline estimation

---

## Implementation Details

### Parameter Binding Flow

```
User Input (Python)
    ↓
GraphQL Query → Rust Binding
    ↓
QueryParam Enum (Type-Safe)
    ↓
prepare_parameters() Validation
    ↓
validate_parameter_count() Check
    ↓
PostgreSQL Prepared Statement
    ↓
Query Execution with Deadlock Retry
    ↓
JSONB Result Extraction
    ↓
Response JSON
```

### Transaction Flow with Parameters

```
Transaction::begin()
    ↓
tx.query_with_params(sql, params)  ← SELECT with parameters
    ↓
tx.savepoint(name)                  ← Optional savepoint
    ↓
tx.execute_with_params(sql, params) ← PostgreSQL function call
    ↓
tx.commit() / tx.rollback()
```

### Mutation Pattern (PostgreSQL Functions)

```
User wants to create user
    ↓
Rust calls: SELECT create_user($1, $2) RETURNING to_jsonb(*)
    ↓
Parameters: [Text("John"), Text("john@example.com")]
    ↓
PostgreSQL Function executes:
    - INSERT into users table
    - Run triggers/constraints
    - Return new row as JSONB
    ↓
Rust receives JSONB result
    ↓
Response to client
```

---

## Architecture Principles

1. **Type Safety First**: `QueryParam` enum prevents invalid parameters
2. **SQL Injection Prevention**: Parameters never interpolated into SQL
3. **Single Source of Truth**: All binding logic in `parameter_binding.rs`
4. **PostgreSQL Functions**: All mutations via functions, not raw SQL
5. **Error Clarity**: Detailed error messages with context
6. **Transaction Safety**: Timeout + status checks in transactions
7. **Performance**: Deadlock retry + efficient pooling

---

## Testing Strategy

### Unit Tests (Completed)
- Parameter validation (NaN, Infinity, type checking)
- Placeholder counting (single, multiple, edge cases)
- Parameter count matching
- Type conversions

### Integration Tests (Pending)
- Actual PostgreSQL connections
- Transaction semantics
- Deadlock scenarios
- PostgreSQL function calls

### Performance Tests (Planned)
- Parameter validation overhead
- Query execution time
- Memory usage
- Deadlock retry efficiency

---

## Known Limitations & Future Work

### Current Phase (3.2)
- Transactions use pool connection internally (not true connection pinning)
- No prepared statement caching
- Placeholder detection is simple (doesn't handle $$ strings)

### Future Phases
- **Phase 3d**: Hot path optimization (query detection, response building in Rust)
- **Phase 3e**: Mutation pipeline unification
- **Phase 3f**: Subscription support
- **Phase 4**: Performance optimization and monitoring

---

## Success Criteria

### Task 6 Complete When:
- [ ] Documentation explains PostgreSQL function pattern
- [ ] Examples show create/read/update/delete via functions
- [ ] Error handling is clear
- [ ] Code compiles with 0 errors

### Task 7 Complete When:
- [ ] All existing tests pass
- [ ] No new warnings introduced
- [ ] Performance baseline measured
- [ ] Zero regressions detected

### Task 8 Complete When:
- [ ] Phase 3.2 summary written
- [ ] Usage guide complete
- [ ] Architecture documented
- [ ] Next phase plan ready

---

## Summary

Phase 3.2 implements **type-safe query execution with parameter binding** for FraiseQL:

**Completed**:
- ✅ Type system (QueryParam enum)
- ✅ Trait extensions (PoolBackend)
- ✅ Parameter validation
- ✅ ProductionPool implementation
- ✅ Transaction support

**In Progress**: Documentation and examples

**Next**: Task 6 (PostgreSQL function integration), Task 7 (testing), Task 8 (documentation)

---

## Related Documents

- `PHASE_3_2_FOUNDATION_COMPLETE.md` - Foundation implementation details
- `PHASE_3_2_ARCHITECTURE_REVIEW.md` - Architectural decisions
- `PHASE_3_COMPLETE_ROADMAP.md` - Overall Phase 3 roadmap
