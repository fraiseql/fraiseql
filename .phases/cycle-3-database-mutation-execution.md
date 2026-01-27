# Cycle 3: Database Execution and Mutation Execution

## Overview

Implement actual database query execution and mutation handling. This cycle transforms placeholder modules into production-ready components that execute real SQL against databases.

**Duration**: 4 weeks (2 cycles: Database execution + Mutation execution)
**Tests to Pass**: 60 remaining tests (26 database + 34 mutations)
**Focus**: Making real database calls, transaction handling, error recovery

---

## Cycle 3 Structure

### Cycle 3.1: Database Query Execution (Weeks 1-2)
Focus: `database_resolver.rs` implementation

**Tests to Pass (26 total)**
- Entity resolution from PostgreSQL (5)
- Cross-database federation (5)
- Connection pooling and retry (4)
- Query execution and transactions (6)
- Performance and error handling (6)

**Implementation Tasks**
1. Replace mock data in DatabaseEntityResolver with real database queries
2. Implement connection pooling strategy
3. Add retry logic for transient failures
4. Handle different database types (PostgreSQL, MySQL, SQL Server)
5. Implement transaction support
6. Add result projection and ordering

**Key Changes to database_resolver.rs**
```rust
impl<A: DatabaseAdapter> DatabaseEntityResolver<A> {
    pub async fn resolve_entities_from_db(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>> {
        // BEFORE: Returns mock data
        // AFTER: 
        // 1. Build WHERE IN clause from representations
        // 2. Execute actual query: SELECT fields FROM table WHERE ...
        // 3. Map results to entity order
        // 4. Return real database rows
    }
}
```

### Cycle 3.2: Mutation Execution (Weeks 3-4)
Focus: `mutation_executor.rs` implementation

**Tests to Pass (34 total)**
- Local entity mutations (11)
- Response formatting (1)
- Cross-subgraph coordination (7)
- Error scenarios (6)
- Performance and latency (2)
- Extended entity mutations (7)

**Implementation Tasks**
1. Implement `execute_local_mutation()` with actual SQL execution
2. Implement `execute_extended_mutation()` with HTTP propagation
3. Add transaction support with rollback on failure
4. Handle composite key mutations
5. Implement batch mutation processing
6. Add multi-subgraph coordination logic

**Key Changes to mutation_executor.rs**
```rust
impl<A: DatabaseAdapter> FederationMutationExecutor<A> {
    pub async fn execute_local_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
    ) -> Result<Value> {
        // BEFORE: Returns mock entity
        // AFTER:
        // 1. Determine mutation type (CREATE/UPDATE/DELETE)
        // 2. Build SQL query from variables
        // 3. Execute against database
        // 4. Return updated entity
    }

    pub async fn execute_extended_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
    ) -> Result<Value> {
        // BEFORE: Returns mock entity
        // AFTER:
        // 1. Find authoritative subgraph from metadata
        // 2. Send mutation to authoritative subgraph (HTTP or direct DB)
        // 3. Return federation response
    }
}
```

---

## RED Phase: Test Analysis

We already have 60 failing tests from Cycle 2's RED phase:

### Database Integration Tests (26 failing)

**Group 1: Entity Resolution (5 tests)**
- test_resolve_entity_from_postgres_table
- test_resolve_entities_batch_from_postgres
- test_resolve_entity_composite_key_from_postgres
- test_resolve_entity_with_null_values_from_postgres
- test_resolve_entity_large_result_set_from_postgres

**Group 2: Cross-Database Federation (5 tests)**
- test_cross_database_postgres_to_mysql
- test_cross_database_postgres_to_sqlserver
- test_cross_database_type_coercion_numeric
- test_cross_database_type_coercion_string
- test_cross_database_type_coercion_datetime

**Group 3: Connection Management (4 tests)**
- test_database_connection_pooling
- test_database_connection_reuse
- test_database_connection_timeout
- test_database_connection_retry

**Group 4: Query Execution (6 tests)**
- test_database_query_execution_basic
- test_database_prepared_statements
- test_database_parameterized_queries
- test_database_transaction_handling
- test_database_transaction_rollback
- (5 more error handling tests)

**Group 5: Performance (6 tests)**
- test_single_entity_resolution_latency
- test_batch_100_entities_resolution_latency
- test_concurrent_entity_resolution
- (3 more error scenarios)

### Mutation Integration Tests (34 failing)

**Group 1: Local Entity Mutations (11 tests)**
- test_mutation_create_owned_entity
- test_mutation_update_owned_entity
- test_mutation_delete_owned_entity
- test_mutation_owned_entity_returns_updated_representation
- test_mutation_owned_entity_batch_updates
- test_mutation_composite_key_update
- test_mutation_with_validation_errors
- test_mutation_constraint_violation
- test_mutation_concurrent_updates
- test_mutation_transaction_rollback
- (1 more)

**Group 2: Extended Entity Mutations (7 tests)**
- test_mutation_extended_entity_requires_resolution
- test_mutation_extended_entity_propagates_to_owner
- test_mutation_extended_entity_partial_fields
- test_mutation_extended_entity_cross_subgraph
- test_mutation_extended_entity_with_external_fields
- test_mutation_extended_entity_reference_tracking
- test_mutation_extended_entity_cascade_updates

**Group 3: Response Format (1 test)**
- test_mutation_response_subscription_trigger

**Group 4: Cross-Subgraph Coordination (7 tests)**
- test_mutation_coordinate_two_subgraph_updates
- test_mutation_coordinate_three_subgraph_updates
- test_mutation_reference_update_propagation
- test_mutation_circular_reference_handling
- test_mutation_multi_subgraph_transaction
- test_mutation_subgraph_failure_rollback
- test_mutation_subgraph_timeout_handling

**Group 5: Error Scenarios (6 tests)**
- test_mutation_entity_not_found
- test_mutation_invalid_field_value
- test_mutation_missing_required_fields
- test_mutation_authorization_error
- test_mutation_duplicate_key_error
- (other errors)

**Group 6: Performance (2+ tests)**
- test_mutation_latency_single_entity
- test_mutation_latency_batch_updates
- test_mutation_concurrent_request_handling

---

## GREEN Phase Strategy

### Prioritization

**Phase 3.1 Priority (Database Execution)**
1. Implement basic query execution (test_database_query_execution_basic)
2. Handle PostgreSQL entity resolution (5 tests)
3. Add transaction support (6 tests)
4. Implement connection pooling (4 tests)
5. Cross-database support (5 tests)
6. Performance optimization (6 tests)

**Phase 3.2 Priority (Mutation Execution)**
1. Implement local mutation execution (11 tests)
2. Extended entity mutations (7 tests)
3. Cross-subgraph coordination (7 tests)
4. Error handling (6 tests)
5. Performance testing (2 tests)
6. Response formatting (1 test)

### Implementation Order

**Week 1: Database Query Execution Basics**
```
database_resolver.rs:
  - Implement resolve_entities_from_db() with real queries
  - Execute SELECT with WHERE IN clause
  - Map database rows to federation format
  - Handle NULL values
  - Tests passing: 5 basic entity resolution tests
```

**Week 2: Database Features**
```
database_resolver.rs (continued):
  - Add transaction support via adapter
  - Implement connection pooling abstraction
  - Add retry logic for transient failures
  - Handle cross-database type coercion
  - Tests passing: 21 database integration tests (5+6+4+5+1)
```

**Week 3: Mutation Execution Basics**
```
mutation_executor.rs:
  - Implement execute_local_mutation() with actual SQL
  - Determine mutation type from query
  - Build and execute UPDATE/INSERT/DELETE
  - Return updated entity representation
  - Tests passing: 11 local mutation tests
```

**Week 4: Advanced Mutations**
```
mutation_executor.rs (continued):
  - Implement execute_extended_mutation() with HTTP/DB propagation
  - Add multi-subgraph coordination
  - Handle transaction rollback on failure
  - Add error handling and recovery
  - Tests passing: 7 extended + 7 coordination + 6 error + more
```

---

## Technical Details

### DatabaseAdapter Integration

Use existing DatabaseAdapter trait from Cycle 1:

```rust
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_raw_query(&self, sql: &str) -> Result<Vec<HashMap<String, Value>>>;
    // (other methods available)
}
```

Implementation approach:
1. Build SQL using query_builder (WHERE IN clauses)
2. Call adapter.execute_raw_query()
3. Transform HashMap results to federation format
4. Handle errors and missing entities

### Transaction Support

```rust
pub async fn resolve_entities_from_db_transactional(
    &self,
    typename: &str,
    representations: &[EntityRepresentation],
    selection: &FieldSelection,
    transaction: Option<&Transaction>,
) -> Result<Vec<Option<Value>>> {
    // If transaction provided, use it
    // Otherwise, create implicit transaction
}
```

### Extended Mutation Propagation

For mutations on extended (non-owned) entities:

```rust
pub async fn execute_extended_mutation(
    &self,
    typename: &str,
    mutation_name: &str,
    variables: &Value,
) -> Result<Value> {
    // 1. Find authoritative subgraph from metadata
    let authoritative = self.metadata.find_authoritative_subgraph(typename)?;
    
    // 2. Send mutation request (HTTP or direct DB)
    match authoritative {
        SubgraphType::Http(url) => {
            // Send HTTP mutation request
            send_mutation_to_subgraph(&url, typename, mutation_name, variables).await
        }
        SubgraphType::DirectDatabase(connection) => {
            // Send direct database mutation
            execute_mutation_on_database(connection, typename, variables).await
        }
    }
}
```

---

## Success Criteria

### Cycle 3.1 (Database Execution)
- [ ] All 26 database integration tests pass
- [ ] Query execution works with real PostgreSQL
- [ ] Connection pooling implemented
- [ ] Transaction support working
- [ ] Cross-database type coercion correct
- [ ] Performance targets met (<5ms local, <20ms cross-DB)

### Cycle 3.2 (Mutation Execution)
- [ ] All 34 mutation tests pass
- [ ] Local mutations execute correctly
- [ ] Extended mutations propagate to owner subgraph
- [ ] Multi-subgraph transactions coordinate properly
- [ ] Rollback on failure works
- [ ] Error handling comprehensive

### Overall Cycle 3
- [ ] 60/83 tests passing (72% complete)
- [ ] All code refactored and clean
- [ ] Zero clippy warnings
- [ ] Performance benchmarks met
- [ ] Ready for production use

---

## Dependencies

- DatabaseAdapter from Cycle 1 (already implemented)
- query_builder from Cycle 2 (WHERE clauses)
- mutation_query_builder from Cycle 2 (UPDATE/INSERT/DELETE)
- sql_utils from Cycle 2 (SQL safety)
- metadata_helpers from Cycle 2 (metadata lookup)

---

## Risks & Mitigations

| Risk | Probability | Mitigation |
|------|-------------|-----------|
| Database connection pooling complexity | Medium | Use existing connection pool library (sqlx) |
| Multi-database type coercion bugs | Medium | Comprehensive test cases per database |
| Transaction deadlock in concurrent mutations | Low | Serialization + timeout handling |
| Extended mutation circular references | Low | Metadata validation + cycle detection |
| Performance regression with large batches | Medium | Benchmark before/after, optimize query |

---

## Timeline

- **Week 1**: Database query execution basics (5 tests passing)
- **Week 2**: Database features and optimization (26 tests passing)
- **Week 3**: Mutation execution basics (37 tests passing)
- **Week 4**: Advanced mutations and coordination (60 tests passing)

**Completion**: All 60 remaining tests passing, ready for Cycle 4 (Testing & Apollo Compatibility)
