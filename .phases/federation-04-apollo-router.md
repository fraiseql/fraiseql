# Phase 4: Apollo Router Integration Testing

**Duration**: 3 weeks (weeks 16-18)
**Lead Role**: Senior Rust Engineer
**Impact**: HIGH - Production certification with Apollo ecosystem
**Goal**: Verify FraiseQL compatibility with Apollo Router and produce deployment guide

---

## Objective

**Certify FraiseQL as production-ready Apollo Federation v2 implementation** through comprehensive testing with Apollo Router across all directive combinations and scenarios.

### Key Insight
Apollo Router is the de facto federation gateway. Certification unlocks enterprise adoption.

---

## Success Criteria

### Must Have
- [ ] Docker Compose test harness (3+ subgraphs)
- [ ] 40+ integration tests covering all scenarios
- [ ] Apollo Federation v2 spec compliance verified
- [ ] Performance benchmarks: P95 <100ms, P99 <200ms
- [ ] Production deployment guide complete
- [ ] All tests passing

### Performance Targets
- [ ] Router latency P95: <100ms
- [ ] Router latency P99: <200ms
- [ ] Throughput: >1000 queries/second with 3 subgraphs
- [ ] Error recovery: <5 seconds

---

## Architecture

### Test Harness

```yaml
# tests/apollo-router/docker-compose.yml
version: '3.8'
services:
  apollo-router:
    image: ghcr.io/apollographql/router:v1.40.0
    ports: ["4000:4000"]
    environment:
      APOLLO_ROUTER_CONFIG_PATH: /config/router.yaml
      APOLLO_ROUTER_SUPERGRAPH_PATH: /config/supergraph.graphql

  users-subgraph:
    build: ./subgraphs/users
    ports: ["4001:4001"]
    environment:
      DATABASE_URL: postgres://user:pass@users-db:5432/users

  orders-subgraph:
    build: ./subgraphs/orders
    ports: ["4002:4002"]
    environment:
      DATABASE_URL: mysql://user:pass@orders-db:3306/orders

  products-subgraph:
    build: ./subgraphs/products
    ports: ["4003:4003"]
    environment:
      DATABASE_URL: sqlserver://sa:Password@products-db:1433/products
```

### Test Harness Rust API

```rust
// tests/apollo-router/src/harness.rs

pub struct ApolloRouterHarness {
    router_url: String,
    subgraphs: Vec<SubgraphInfo>,
    docker_compose: DockerCompose,
}

impl ApolloRouterHarness {
    pub async fn start() -> Result<Self>;
    pub async fn query(&self, query: &str) -> Result<GraphQLResponse>;
    pub async fn stop(self) -> Result<()>;
}
```

---

## TDD Cycles

### Cycle 1: Test Infrastructure (Week 16)
- Docker Compose setup with 3 subgraphs
- Rust test harness for running queries
- Health checks and startup verification

### Cycle 2: Integration Tests (Week 17)
- Entity resolution tests (20+ scenarios)
- @requires/@provides tests
- @shareable field resolution tests
- Error handling tests

### Cycle 3: Certification & Benchmarking (Week 18)
- Performance benchmarks
- Apollo Federation v2 spec compliance
- Deployment guide
- Examples and documentation

---

## Test Scenarios

1. **Basic Entity Resolution** - Query across subgraphs
2. **Batch Entity Resolution** - Multiple entities with batching
3. **@requires Enforcement** - Missing required fields error
4. **@provides Validation** - Field resolution with dependencies
5. **@shareable Resolution** - Multiple subgraph ownership
6. **Error Recovery** - Subgraph failure handling
7. **Large Result Sets** - Performance under load
8. **Complex Queries** - Nested mutations and queries

---

## Key Deliverables

1. **Docker Test Harness**: Ready-to-run with 3 subgraphs
2. **Integration Tests**: 40+ test scenarios
3. **Performance Benchmarks**: Documented latencies
4. **Deployment Guide**: Production checklist
5. **Examples**: Docker Compose examples for different databases

---

## Critical Files to Create

- `tests/apollo-router/docker-compose.yml`
- `tests/apollo-router/src/harness.rs`
- `tests/apollo-router/tests/integration_test.rs`
- `docs/APOLLO_ROUTER_INTEGRATION.md`

---

**Phase Status**: Planning
**Estimated Tests**: +40
**Estimated Code**: 800 lines
