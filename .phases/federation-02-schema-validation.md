# Phase 2: Federation Schema Validation

**Duration**: 3 weeks (weeks 5-7)
**Lead Role**: Senior Rust Engineer
**Impact**: HIGH - Prevents invalid federation schemas from deploying
**Goal**: Implement multi-subgraph consistency checking and composition validation

---

## Objective

Extend validation beyond single-subgraph to **multi-subgraph federation**, catching composition errors at compile time before they reach Apollo Router.

### Key Insight
Schema composition errors are exponentially more expensive when caught at runtime vs compile time.

---

## Success Criteria

### Must Have
- [ ] Cross-subgraph consistency validation
- [ ] Conflict detection (field type mismatches)
- [ ] Composition validator producing valid supergraph
- [ ] CLI `compose` command working
- [ ] Configuration support for resolution strategies
- [ ] 65+ new tests passing

### Performance Targets
- [ ] Composition validation: <500ms for 3 subgraphs
- [ ] Conflict detection: <100ms per conflict

---

## Architecture

### Cross-Subgraph Validator

```rust
// crates/fraiseql-cli/src/federation/cross_subgraph_validator.rs

pub struct CrossSubgraphValidator {
    subgraphs: Vec<IntermediateSchema>,
}

impl CrossSubgraphValidator {
    pub fn validate_consistency(&self) -> Result<()> {
        self.validate_key_consistency()?;
        self.validate_external_field_ownership()?;
        self.validate_shareable_conflicts()?;
        self.validate_provides_contracts()?;
        Ok(())
    }
}
```

### CLI `compose` Command

```bash
fraiseql compose \
  --subgraph users:users-schema.json \
  --subgraph orders:orders-schema.json \
  --subgraph products:products-schema.json \
  --output supergraph.json \
  --validate
```

---

## TDD Cycles

### Cycle 1: Cross-Subgraph Validation (Week 5)
- Write tests for field ownership rules
- Implement consistency validator
- Add conflict detection

### Cycle 2: Composition Validator (Week 6)
- Build composed schema from subgraphs
- Validate composed schema structure
- Test with 3+ subgraph combinations

### Cycle 3: CLI Integration & Configuration (Week 7)
- Add `fraiseql compose` command
- Configuration file support (fraiseql.yml)
- Error reporting and suggestions

---

## Key Deliverables

1. **Consistency Validator**: Ensure each @key is unique, each @external has an owner
2. **Conflict Detector**: Detect incompatible field types across subgraphs
3. **Composition Tool**: Build supergraph SDL from subgraphs
4. **CLI Command**: `fraiseql compose` with validation
5. **Configuration**: Support for conflict resolution strategies

---

## Critical Files to Modify

- `crates/fraiseql-cli/src/federation/cross_subgraph_validator.rs` (NEW)
- `crates/fraiseql-cli/src/federation/composition_validator.rs` (NEW)
- `crates/fraiseql-cli/src/commands/compose.rs` (NEW)
- `crates/fraiseql-cli/src/main.rs` - Add compose subcommand

---

## Next Phase Dependencies

Phase 3 (Distributed Transactions) depends on valid composition, so must complete this phase first.

---

**Phase Status**: Planning
**Estimated Tests**: +65
**Estimated Code**: 1,500 lines
