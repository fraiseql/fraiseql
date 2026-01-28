# Phase 5: @shareable Field-Level Implementation

**Duration**: 2 weeks (weeks 19-20)
**Lead Role**: Senior Rust Engineer
**Impact**: MEDIUM - Complete type merging across subgraphs
**Goal**: Implement field-level @shareable with resolution strategies and fallback

---

## Objective

Extend @shareable from type-level to **complete field-level sharing**, enabling sophisticated type merging with fallback strategies and load balancing across subgraphs.

### Key Insight
Field-level sharing enables gradual federation - start with simple cases, add complexity incrementally.

---

## Success Criteria

### Must Have
- [ ] Complete type merging across subgraphs
- [ ] 4 resolution strategies (FirstAvailable, ByPriority, RoundRobin, FastestWins)
- [ ] Automatic fallback on subgraph failure
- [ ] Configuration support for resolution strategies
- [ ] 45+ new tests passing

### Performance Targets
- [ ] Field resolution: <50ms per field
- [ ] Strategy evaluation: <5ms
- [ ] Fallback: <100ms max

---

## Architecture

### Type Merger

```rust
// crates/fraiseql-core/src/federation/type_merger.rs

pub struct TypeMerger {
    metadata: Arc<FederationMetadata>,
}

pub struct MergedField {
    pub name: String,
    pub typename: String,
    pub subgraphs: Vec<SubgraphFieldInfo>,
    pub resolution_strategy: ResolutionStrategy,
}

pub enum ResolutionStrategy {
    FirstAvailable,    // Try subgraphs in order
    ByPriority,        // Use explicit priority
    RoundRobin,        // Load balance
    FastestWins,       // Race condition
}
```

### Field Resolver

```rust
// crates/fraiseql-core/src/federation/field_resolver.rs

pub struct FieldResolver {
    merger: Arc<TypeMerger>,
}

impl FieldResolver {
    pub async fn resolve_field(
        &self,
        typename: &str,
        field: &str,
        representation: &EntityRepresentation,
    ) -> Result<Value>;
}
```

---

## TDD Cycles

### Cycle 1: Type Merging Logic (Week 19)
- Collect type definitions across subgraphs
- Build merged field registry
- Detect field conflicts
- Validate @shareable consistency

### Cycle 2: Resolution Strategies & Configuration (Week 20)
- Implement all 4 strategies
- Configuration file support
- Fallback on failure
- Performance optimization

---

## Key Deliverables

1. **Type Merger**: Merge fields across subgraphs
2. **Field Resolver**: Resolve fields with strategies
3. **Fallback Logic**: Automatic failover
4. **Configuration**: fraiseql.yml support
5. **Tests**: 45+ test scenarios

---

## Configuration Example

```yaml
# fraiseql.yml
federation:
  shareable_resolution:
    strategy: by_priority
    priority:
      - users-db      # Try database first
      - users-cache   # Then cache
      - users-api     # Finally API
    fallback: true
    timeout_ms: 100
```

---

**Phase Status**: Planning
**Estimated Tests**: +45
**Estimated Code**: 1,000 lines
