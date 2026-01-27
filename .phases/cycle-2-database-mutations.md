# Cycle 2: Database Integration & Federation Mutations

**Duration**: 4-5 days (parallel work, not sequential)
**Status**: Planning phase
**Dependency**: Cycle 1 complete ✅

---

## Overview

Cycle 2 builds on Cycle 1's federation foundation by:
1. **Connecting to actual databases** for entity resolution (currently mocked)
2. **Implementing federation mutations** for updating entities across subgraphs
3. **Adding cross-database queries** using native database connectors

This enables real-world federation scenarios where entities are resolved from live databases rather than stubs.

---

## Architecture Changes

### Database Integration (What Changes from Cycle 1)

**Current (Cycle 1):**
```rust
// Mock resolution - always returns test data
resolve_entities_local(reps) {
    entities.push(Some(json!({"id": "123", "name": "User"})));
}
```

**After Cycle 2:**
```rust
// Real database resolution
resolve_entities_local(reps, adapter) {
    let sql = construct_query(reps, key_fields);
    let rows = adapter.execute_query(&sql).await?;
    project_results(rows, selection_fields)
}
```

### Mutation Flow (New in Cycle 2)

```
mutation UpdateUser($id: ID!, $name: String!) {
  updateUser(id: $id, name: $name) {
    id
    name
  }
}

Executor receives mutation
  ↓
Detect entity mutation in _mutations (internal, not federation spec)
  ↓
Determine ownership (Local? Extended?)
  ↓
For Local entities: Execute mutation directly on owned subgraph's database
For Extended entities: Propagate to authoritative subgraph via HTTP or DB
  ↓
Resolve updated entity representation
  ↓
Return federation response with updated entity
```

---

## Cycle 2 Structure

### RED Phase: Tests for Database & Mutations (2-3 hours)

#### Test Categories

**1. Database Query Tests** (15 tests)
```
- test_resolve_entity_from_postgres_table
- test_resolve_entities_batch_from_postgres
- test_resolve_entity_composite_key_from_postgres
- test_resolve_entity_where_clause_building
- test_cross_database_postgres_to_mysql
- test_cross_database_postgres_to_sqlserver
- test_database_connection_pooling
- test_database_null_value_handling
- test_database_type_coercion
- test_database_prepared_statements
- test_database_transaction_handling
- test_database_connection_retry
- test_database_query_timeout
- test_database_large_result_set
- test_database_sql_injection_prevention
```

**2. Local Entity Mutation Tests** (10 tests)
```
- test_mutation_update_owned_entity
- test_mutation_create_owned_entity
- test_mutation_delete_owned_entity
- test_mutation_owned_entity_returns_updated_representation
- test_mutation_owned_entity_batch_updates
- test_mutation_composite_key_update
- test_mutation_with_validation
- test_mutation_constraint_violation
- test_mutation_concurrent_updates
- test_mutation_transaction_rollback
```

**3. Extended Entity Mutation Tests** (8 tests)
```
- test_mutation_extended_entity_requires_resolution
- test_mutation_extended_entity_propagates_to_owner
- test_mutation_extended_entity_partial_fields
- test_mutation_extended_entity_cross_subgraph
- test_mutation_extended_entity_with_external_fields
- test_mutation_extended_entity_reference_tracking
- test_mutation_extended_entity_cascade_updates
- test_mutation_extended_entity_conflict_resolution
```

**4. Federation Mutation Response Tests** (6 tests)
```
- test_mutation_response_format_matches_spec
- test_mutation_response_includes_updated_fields
- test_mutation_response_federation_wrapper
- test_mutation_response_error_federation_format
- test_mutation_response_partial_success
- test_mutation_response_subscription_trigger
```

**5. Cross-Subgraph Mutation Tests** (7 tests)
```
- test_mutation_coordinate_two_subgraph_updates
- test_mutation_coordinate_three_subgraph_updates
- test_mutation_reference_update_propagation
- test_mutation_circular_reference_handling
- test_mutation_multi_subgraph_transaction
- test_mutation_subgraph_failure_rollback
- test_mutation_subgraph_timeout_handling
```

**Total RED Phase Tests: 46 tests** (all failing initially)

---

### GREEN Phase: Implementation (6-8 hours)

#### Part A: Database Integration (4-5 hours)

**1. Extend FederationResolver with Database Access**
```rust
// crates/fraiseql-core/src/federation/database_resolver.rs (NEW)

pub struct DatabaseEntityResolver<A: DatabaseAdapter> {
    adapter: Arc<A>,
    metadata: FederationMetadata,
}

impl<A: DatabaseAdapter> DatabaseEntityResolver<A> {
    /// Resolve entities from local database
    pub async fn resolve_entities_from_db(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>> {
        // 1. Extract key values from representations
        let key_values = extract_key_values(representations);

        // 2. Build WHERE IN query
        let where_clause = construct_where_in_clause(
            typename,
            &key_values,
            &self.metadata,
        );

        // 3. Build SELECT fields from selection
        let select_fields = extract_selection_fields(selection);

        // 4. Execute query
        let rows = self.adapter.execute_query(
            typename,
            Some(&where_clause),
            &select_fields,
        ).await?;

        // 5. Project results
        project_results(rows, &select_fields)
    }
}
```

**2. Add Database-Aware Entity Resolution**
```rust
// Modify federation/entity_resolver.rs

pub async fn batch_load_entities<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    fed_resolver: &FederationResolver,
    adapter: Arc<A>,  // NEW: database connection
    selection: &FieldSelection,  // NEW: which fields to fetch
) -> Result<Vec<Option<Value>>> {
    // Dedup and group
    let deduped = deduplicate_representations(representations);
    let grouped = group_entities_by_typename(&deduped);

    let mut results = Vec::new();

    for (typename, reps) in grouped {
        // Determine resolution strategy
        let strategy = fed_resolver.get_or_determine_strategy(&typename);

        match strategy {
            ResolutionStrategy::Local { view_name, key_columns } => {
                // NEW: Query actual database instead of mock
                let db_results = DatabaseEntityResolver::new(adapter.clone())
                    .resolve_entities_from_db(&typename, &reps, selection)
                    .await?;
                results.extend(db_results);
            },
            ResolutionStrategy::DirectDatabase { connection_string, .. } => {
                // Query remote database via connection string
                // (implement in next iteration)
                todo!()
            },
            ResolutionStrategy::Http { subgraph_url } => {
                // Fallback to HTTP (existing)
                // (implement in next iteration)
                todo!()
            }
        }
    }

    Ok(results)
}
```

**3. WHERE Clause Construction**
```rust
// crates/fraiseql-core/src/federation/query_builder.rs (NEW)

pub fn construct_where_in_clause(
    typename: &str,
    key_values: &[Value],
    metadata: &FederationMetadata,
) -> String {
    let fed_type = metadata.types.iter()
        .find(|t| t.name == typename)
        .expect("Type must exist");

    let key_directive = fed_type.keys.first()
        .expect("Type must have @key");

    let key_column = &key_directive.fields[0];  // Simplified: single key

    // Build: WHERE key_column IN ('val1', 'val2', ...)
    let values_str = key_values.iter()
        .map(|v| format!("'{}'", escape_sql_string(v)))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{} IN ({})", key_column, values_str)
}

fn escape_sql_string(value: &Value) -> String {
    // Prevent SQL injection
    value.as_str()
        .unwrap_or("")
        .replace("'", "''")
}
```

**4. Modify Executor to Pass Database Context**
```rust
// Modify runtime/executor.rs

async fn execute_entities_query(
    &self,
    _query: &str,
    variables: Option<&serde_json::Value>,
) -> Result<String> {
    // ... existing code ...

    // Create federation resolver
    let fed_resolver = crate::federation::FederationResolver::new(fed_metadata);

    // Parse field selection from query (NEW)
    let selection = parse_field_selection(_query)?;

    // Batch load entities WITH DATABASE CONNECTION (changed)
    let entities = crate::federation::batch_load_entities(
        &representations,
        &fed_resolver,
        self.adapter.clone(),  // NEW: pass adapter
        &selection,            // NEW: pass field selection
    ).await?;

    // ... rest of response handling ...
}
```

**5. Field Selection Parsing**
```rust
// crates/fraiseql-core/src/federation/selection_parser.rs (NEW)

pub struct FieldSelection {
    pub fields: Vec<String>,
}

pub fn parse_field_selection(query: &str) -> Result<FieldSelection> {
    // Parse GraphQL query to extract requested fields
    // Example: _entities(...) { __typename id name email }
    // Returns: FieldSelection { fields: ["__typename", "id", "name", "email"] }

    let parsed = crate::graphql::parse_query(query)?;
    let mut fields = Vec::new();

    // Extract from _entities selection set
    for field in &parsed.selections {
        if field.name == "_entities" {
            fields.extend(field.selection_set.iter().map(|s| s.clone()));
        }
    }

    Ok(FieldSelection { fields })
}
```

#### Part B: Federation Mutations (2-3 hours)

**1. Mutation Type Detection**
```rust
// crates/fraiseql-core/src/federation/mutation_detector.rs (NEW)

pub fn is_federation_mutation(query: &str) -> bool {
    // Check if this is a mutation affecting federated entities
    // Pattern: mutation { updateUser(id: "123", ...) { ... } }

    let trimmed = query.trim();
    trimmed.starts_with("mutation") || trimmed.contains("mutation {")
}

pub fn extract_mutation_name(query: &str) -> Option<String> {
    // Extract which entity type is being mutated
    // Used to determine ownership (local vs extended)

    let parsed = parse_query(query).ok()?;
    parsed.mutations.first().map(|m| m.name.clone())
}
```

**2. Mutation Executor**
```rust
// crates/fraiseql-core/src/federation/mutation_executor.rs (NEW)

pub struct FederationMutationExecutor<A: DatabaseAdapter> {
    adapter: Arc<A>,
    fed_resolver: FederationResolver,
}

impl<A: DatabaseAdapter> FederationMutationExecutor<A> {
    /// Execute mutation on locally-owned entity
    pub async fn execute_local_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
        selection: &FieldSelection,
    ) -> Result<Value> {
        // 1. Build UPDATE/INSERT/DELETE query
        let sql = self.build_mutation_sql(typename, mutation_name, variables)?;

        // 2. Execute mutation
        let result = self.adapter.execute_mutation(&sql).await?;

        // 3. Resolve updated entity
        let entity_id = extract_entity_id(&result);
        let updated = self.resolve_updated_entity(typename, &entity_id, selection).await?;

        Ok(updated)
    }

    /// Execute mutation on extended entity (owned by another subgraph)
    pub async fn execute_extended_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
        selection: &FieldSelection,
    ) -> Result<Value> {
        // 1. Determine authoritative subgraph
        let auth_subgraph = self.fed_resolver.get_authoritative_subgraph(typename)?;

        // 2. Send mutation to authoritative subgraph
        let response = self.send_mutation_to_subgraph(
            &auth_subgraph,
            mutation_name,
            variables,
        ).await?;

        // 3. Return federation-formatted response
        Ok(response)
    }
}
```

**3. Mutation Query Building**
```rust
// crates/fraiseql-core/src/federation/mutation_query_builder.rs (NEW)

pub fn build_update_query(
    typename: &str,
    variables: &Value,
    fed_metadata: &FederationMetadata,
) -> Result<String> {
    // Build: UPDATE users SET name = 'new' WHERE id = 123

    let table = get_table_for_type(typename, fed_metadata)?;
    let key_field = get_key_field_for_type(typename, fed_metadata)?;

    let mut set_clauses = Vec::new();
    for (field, value) in variables.as_object().unwrap() {
        if field != &key_field {
            set_clauses.push(format!("{} = {}", field, sql_value(value)));
        }
    }

    let key_value = variables.get(&key_field).unwrap();

    Ok(format!(
        "UPDATE {} SET {} WHERE {} = {}",
        table,
        set_clauses.join(", "),
        key_field,
        sql_value(key_value)
    ))
}
```

**4. Mutation Response Formatting**
```rust
// Modify federation/mutation_executor.rs

fn format_mutation_response(entity: Value, mutation_name: &str) -> Value {
    // Return federation-compliant mutation response
    // Same envelope as query responses: { data: { mutationName: {...} } }

    json!({
        "data": {
            mutation_name: entity
        }
    })
}
```

---

### REFACTOR Phase: Design Improvements (2 hours)

**Goals:**
1. Optimize query building for multiple databases
2. Extract database adapters into separate trait implementations
3. Improve error handling for cross-database failures
4. Add query caching for repeated entity resolutions

**Work Items:**
- Extract `DatabaseQueryBuilder` trait for PostgreSQL/MySQL/SQL Server specific syntax
- Implement `PostgresQueryBuilder`
- Implement `MySqlQueryBuilder`
- Create `QueryCache` for resolved entities (optional: implements Apollo DataLoader pattern)
- Improve `FieldSelection` parsing robustness
- Add mutation transaction isolation levels

---

### CLEANUP Phase: Testing & Finalization (1-2 hours)

**Verification:**
- All 46 tests passing
- Clippy: Zero warnings
- All archaeological comments removed
- Test coverage verified
- Documentation updated
- Performance benchmarks

**Database Integration Tests:**
```bash
cargo test -p fraiseql-core database_resolver:: --lib
cargo test -p fraiseql-core mutation_executor:: --lib
cargo test -p fraiseql-core federation:: --lib
```

**Integration Tests:**
```bash
cargo test -p fraiseql-core --test federation_database_integration
cargo test -p fraiseql-core --test federation_mutations_integration
```

---

## Files to Create/Modify

### New Files (Database Integration)
1. `src/federation/database_resolver.rs` - Database entity resolution
2. `src/federation/query_builder.rs` - WHERE clause construction
3. `src/federation/selection_parser.rs` - Field selection parsing
4. `src/federation/database_adapter_ext.rs` - Database adapter extensions

### New Files (Mutations)
5. `src/federation/mutation_detector.rs` - Mutation detection
6. `src/federation/mutation_executor.rs` - Mutation execution
7. `src/federation/mutation_query_builder.rs` - UPDATE/INSERT/DELETE building

### New Test Files
8. `tests/federation_database_integration.rs` - Database resolver tests
9. `tests/federation_mutations_integration.rs` - Mutation tests

### Modified Files
10. `src/federation/entity_resolver.rs` - Add database support
11. `src/runtime/executor.rs` - Pass database context to federation handlers
12. `src/federation/mod.rs` - Add mutation handler routing
13. `src/federation/types.rs` - Add mutation-related types

---

## Implementation Order

### Day 1: Database Integration Foundation
```
1. Create database_resolver.rs
2. Create query_builder.rs
3. Create selection_parser.rs
4. Modify entity_resolver.rs to use real queries
5. Update executor.rs to pass adapter
```

### Day 2: Database Tests Pass
```
6. Create federation_database_integration.rs
7. Implement all 15 database tests
8. Fix query building for all database types
9. Verify cross-database queries work
```

### Day 3: Mutation Foundation
```
10. Create mutation_detector.rs
11. Create mutation_executor.rs
12. Create mutation_query_builder.rs
13. Implement local entity mutations
```

### Day 4: Mutation Tests Pass
```
14. Create federation_mutations_integration.rs
15. Implement all 24 mutation tests
16. Add extended entity mutation support
17. Implement cross-subgraph mutations
```

### Day 5: REFACTOR + CLEANUP
```
18. Database adapter trait extraction
19. Query caching implementation
20. Clippy fixes and linting
21. Final testing and verification
22. Remove archaeological markers
```

---

## Testing Strategy

### Unit Tests (Internal)
- Each module has internal tests
- Test isolation with mock adapters
- ~40 unit tests

### Integration Tests
- Real database setup (Docker Postgres)
- Multi-database scenarios (Postgres + MySQL)
- Mutation + Federation together
- ~46 integration tests

### Performance Benchmarks
- Single entity resolution: <5ms
- Batch 100 entities: <10ms
- Mutation execution: <20ms
- Cross-database query: <50ms

---

## Success Criteria

**Functional:**
- ✅ All 46 tests passing
- ✅ Local entity mutations working
- ✅ Extended entity mutations working
- ✅ Cross-subgraph mutations coordinated
- ✅ Cross-database entity resolution working

**Quality:**
- ✅ Clippy: Zero warnings
- ✅ No TODO/FIXME/HACK comments
- ✅ No phase references
- ✅ Performance targets met

**Database Support:**
- ✅ PostgreSQL full support
- ✅ MySQL full support
- ✅ SQL Server full support
- ✅ Type coercion between databases

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| SQL injection | Prepared statements, parameterized queries, escaping |
| Cross-database type mismatch | Type coercion layer, schema mapping |
| Mutation atomicity | Database transactions, rollback on partial failure |
| Connection pool exhaustion | Connection pooling, timeout handling, circuit breaker |
| Query timeout | Per-query timeout, cancellation, fallback to HTTP |
| Network failures (cross-subgraph) | Retry logic, exponential backoff, HTTP fallback |

---

## Dependencies

**From Cycle 1:**
- ✅ Federation query routing
- ✅ FederationMetadata and types
- ✅ Entity representation parsing
- ✅ SDL generation

**External:**
- `tokio` - async runtime
- Database drivers: `postgres`, `mysql`, `sqlserver`
- `sqlx` or `sqlparser` - query building

---

## Next Cycle (Cycle 3)

After Cycle 2 completes:
- **Cycle 3**: Multi-language authoring (Python/TypeScript federation decorators)
- **Cycle 4**: Performance optimization and batching
- **Cycle 5+**: Advanced features, documentation, examples

---

## Summary

Cycle 2 transforms federation from a routing layer into a real database integration system by:
1. **Connecting** to actual databases for entity resolution
2. **Adding** federation mutations for entity updates
3. **Supporting** cross-subgraph mutations with coordination
4. **Handling** multiple databases with type coercion

This enables real-world multi-database and multi-subgraph federation scenarios.

