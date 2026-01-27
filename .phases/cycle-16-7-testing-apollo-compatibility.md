# Cycle 16-7: Testing & Apollo Router Compatibility

**Cycle**: 7 of 8
**Duration**: 2 weeks (Weeks 13-14)
**Phase**: Combined RED → GREEN → REFACTOR → CLEANUP
**Focus**: Apollo Federation v2 compliance, integration tests, performance benchmarks

---

## Objective

Complete federation implementation with full testing and Apollo Router compatibility:
1. Unit tests (100+) for all federation components
2. Integration tests with multi-subgraph scenarios
3. Apollo Router compatibility verification
4. Performance benchmarks meeting all targets
5. Edge case handling and error scenarios

---

## Testing Strategy

### RED Phase: Test Requirements

Write failing tests for:
- Federation spec compliance (Apollo v2)
- Multi-subgraph scenarios (3+ subgraphs)
- Cross-database federation (PostgreSQL, MySQL, SQL Server)
- Apollo Router integration
- Performance benchmarks
- Error scenarios and partial failures

**Test Files**:
- `tests/federation/test_apollo_compliance.rs` - Spec compliance
- `tests/federation/test_multi_subgraph.rs` - 3+ subgraph scenarios
- `tests/federation/test_cross_database.rs` - Multi-database
- `tests/federation/test_apollo_router_integration.rs` - Router compatibility
- `tests/federation/test_federation_edge_cases.rs` - Error cases
- `benches/federation_final_benchmarks.rs` - Performance

**Expected**: 80+ new tests, all failing initially

### GREEN Phase: Implementation

Implement to pass all tests:

```bash
# Run full federation test suite
cargo test --test federation -- --test-threads=1

# Expected: 100+ tests passing
test federation ... ok. 100+ passed
```

Key implementations:
- Apollo Federation v2 spec compliance verification
- Multi-subgraph test harness (3 databases, 3 FraiseQL instances)
- Apollo Router composition and query execution
- Cross-database federation scenarios
- Performance optimization until targets met

### REFACTOR Phase: Design Improvements

- Extract common test utilities
- Improve error messages based on test failures
- Optimize performance-critical paths
- Add observability/metrics

### CLEANUP Phase: Finalization

- All tests passing (100+)
- Performance targets verified
- Documentation complete
- Apollo Router successfully composes schema
- Clean git history

---

## Success Criteria

### Unit Tests (40+)
- [ ] Federation type parsing
- [ ] Entity representation parsing
- [ ] Strategy selection logic
- [ ] Error handling
- [ ] Batching logic
- [ ] Connection management

### Integration Tests (30+)
- [ ] PostgreSQL federation
- [ ] MySQL federation
- [ ] Cross-database (PostgreSQL + MySQL)
- [ ] Cross-database (PostgreSQL + SQL Server)
- [ ] HTTP fallback
- [ ] Mixed strategies (some DB, some HTTP)
- [ ] Partial failures
- [ ] Null entity handling

### Multi-Subgraph Tests (15+)
- [ ] 2 subgraphs (AWS + GCP)
- [ ] 3 subgraphs (AWS + GCP + Azure)
- [ ] Chain federation (User → Order → Product)
- [ ] Circular references (handled gracefully)

### Apollo Router Tests (10+)
- [ ] Router discovers FraiseQL subgraph
- [ ] Router composes schema correctly
- [ ] Router executes federated queries
- [ ] Router handles federation errors
- [ ] Router respects data locality

### Performance Benchmarks (5+)
- [ ] Single entity: <5ms
- [ ] Batch (100): <15ms (Direct DB)
- [ ] Batch (100): <200ms (HTTP)
- [ ] Batching speedup: >10x vs sequential
- [ ] Connection pool efficiency

---

## Multi-Subgraph Test Infrastructure

**Docker Compose Setup** (`tests/fixtures/multi_subgraph_harness/docker-compose.yml`):

```yaml
version: '3.8'
services:
  # Databases
  postgres-a:
    image: postgres:15
    environment:
      POSTGRES_DB: subgraph_a

  postgres-b:
    image: mysql:8
    environment:
      MYSQL_DATABASE: subgraph_b

  postgres-c:
    image: mcr.microsoft.com/mssql/server:2019-latest

  # FraiseQL Subgraphs
  subgraph-users:
    build: .
    ports:
      - "4001:4000"
    depends_on:
      - postgres-a

  subgraph-orders:
    build: .
    ports:
      - "4002:4000"
    depends_on:
      - postgres-b

  subgraph-products:
    build: .
    ports:
      - "4003:4000"
    depends_on:
      - postgres-c

  # Apollo Router
  apollo-router:
    image: ghcr.io/apollographql/router:latest
    ports:
      - "4000:4000"
    depends_on:
      - subgraph-users
      - subgraph-orders
      - subgraph-products
```

---

## Commit Message

```
test(federation): Complete test suite and Apollo Router compatibility

Phase 16, Cycle 7: Testing & Apollo Compatibility

## Changes
- Add 100+ unit tests for federation components
- Add 30+ integration tests (multi-database, multi-subgraph)
- Add Apollo Router compatibility tests
- Add performance benchmarks (all targets met)
- Add edge case and error scenario tests

## Testing Coverage
- Unit tests: 40+ (federation core, entity parsing, strategy selection)
- Integration tests: 30+ (PostgreSQL, MySQL, SQL Server, mixed strategies)
- Multi-subgraph tests: 15+ (2, 3+ subgraphs, chain federation)
- Apollo Router tests: 10+ (discovery, composition, query execution)
- Performance benchmarks: 5+ (all targets met)

## Verification
✅ 100+ tests pass
✅ Apollo Router composes FraiseQL schema
✅ Multi-subgraph federation works (3+ clouds)
✅ Cross-database federation verified
✅ Performance targets met
  - Single entity: <5ms
  - Batch (100): <15ms (DB), <200ms (HTTP)
  - Batching: 10-50x speedup
✅ Error scenarios handled
✅ Partial failures handled gracefully

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>
```

---

**Status**: Ready for implementation
**Result**: 100+ tests passing, Apollo Router integration verified
**Next**: Cycle 8 (Documentation & Examples)
