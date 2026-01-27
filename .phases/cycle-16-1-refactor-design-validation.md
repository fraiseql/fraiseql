# Cycle 16-1: REFACTOR Phase - Design Validation & Optimization

**Cycle**: 1 of 8
**Phase**: REFACTOR (Improve design without changing behavior)
**Duration**: ~2-3 days
**Focus**: Extract traits, validate design, optimize performance

**Prerequisites**:
- GREEN phase complete with all tests passing
- Code is functional but needs design improvement
- Performance baseline established

---

## Objective

Improve federation core design:
1. Extract resolution strategies into trait
2. Improve error handling consistency
3. Add comprehensive documentation
4. Optimize performance where possible
5. Prepare for multi-database support in next cycles

---

## Refactoring Tasks

### Task 1: Extract Resolution Strategy Trait

**Current State** (GREEN):
```rust
pub enum ResolutionStrategy {
    Local { ... },
    DirectDatabase { ... },
    Http { ... },
}
```

**Desired State**:
```rust
#[async_trait::async_trait]
pub trait EntityResolver: Send + Sync {
    async fn resolve(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<JsonValue>>;
}

pub struct LocalEntityResolver { ... }
pub struct DirectDatabaseResolver { ... }
pub struct HttpEntityResolver { ... }

impl EntityResolver for LocalEntityResolver { ... }
impl EntityResolver for DirectDatabaseResolver { ... }
impl EntityResolver for HttpEntityResolver { ... }
```

**Benefits**:
- Easier to extend with new strategies
- Better separation of concerns
- Testable independently
- Enables polymorphism

**Implementation**:

1. Create `crates/fraiseql-core/src/federation/resolution/mod.rs`
2. Move strategy logic to:
   - `crates/fraiseql-core/src/federation/resolution/local.rs`
   - `crates/fraiseql-core/src/federation/resolution/direct_db.rs`
   - `crates/fraiseql-core/src/federation/resolution/http.rs`
3. Create strategy factory in `entity_resolver.rs`

---

### Task 2: Error Handling Consistency

**Current State**:
- Mix of `Result<T, String>` and `Result<T>` (using FraiseQLError)
- Inconsistent error messages
- Missing context for debugging

**Desired State**:
```rust
// Use FraiseQLError consistently
pub type FederationResult<T> = Result<T, FederationError>;

pub enum FederationError {
    Parse {
        message: String,
        typename: Option<String>,
    },
    Resolution {
        message: String,
        typename: String,
        strategy: String,
    },
    Database {
        message: String,
        query: Option<String>,
    },
    Http {
        message: String,
        url: String,
        status: Option<u16>,
    },
}

impl From<FederationError> for FraiseQLError {
    fn from(err: FederationError) -> Self {
        // Convert federation error to main error type
    }
}
```

**Refactoring**:
1. Create `crates/fraiseql-core/src/federation/error.rs`
2. Define `FederationError` enum
3. Implement `Display` for all variants
4. Add error context (typename, strategy, query)
5. Update all functions to use `FederationResult<T>`

---

### Task 3: Comprehensive Documentation

**Add to all public functions**:
```rust
/// Resolve entities using federation.
///
/// Parses entity representations, selects resolution strategy,
/// and returns resolved entities with requested fields.
///
/// # Arguments
/// * `representations` - Array of entity keys to resolve
/// * `selection` - Requested fields for each entity
///
/// # Returns
/// Array of resolved entities (same order as input), with null for missing entities
///
/// # Errors
/// Returns `FederationError::Resolution` if resolution strategy selection fails
///
/// # Performance
/// Batches all entities of same type in single database query.
/// Expected latency: <5ms for <100 entities.
///
/// # Example
/// ```ignore
/// let users = resolver.resolve(
///     "User",
///     &[EntityRepresentation { id: "123", ... }],
///     &field_selection,
/// ).await?;
/// ```
pub async fn resolve(...) { ... }
```

**Add module-level documentation**:
```rust
//! Federation support for multi-subgraph GraphQL.
//!
//! This module implements Apollo Federation v2 specification:
//! - Entity resolution via `_entities` query
//! - Service SDL via `_service` query
//! - Multiple resolution strategies (local, direct DB, HTTP)
//!
//! # Architecture
//!
//! The federation system works in phases:
//! 1. **Parsing**: Transform `_Any` scalar input to `EntityRepresentation`
//! 2. **Strategy Selection**: Determine how to resolve entity (local/DB/HTTP)
//! 3. **Batching**: Group entities by typename and strategy
//! 4. **Resolution**: Execute queries/requests to get entities
//! 5. **Projection**: Filter results to requested fields
//!
//! # Example
//!
//! ```ignore
//! let executor = FederationExecutor::new(adapter, metadata);
//! let response = executor.handle_entities_query(input).await?;
//! ```
```

---

### Task 4: Performance Optimization

#### 4.1: Batch Query Optimization

**Optimization**: Use prepared statements for repeated queries

```rust
// Cache prepared statements
struct PreparedStatementCache {
    cache: HashMap<String, PreparedStatement>,
}

impl PreparedStatementCache {
    pub async fn get_or_prepare(
        &mut self,
        sql: &str,
        adapter: &dyn DatabaseAdapter,
    ) -> Result<PreparedStatement> {
        if let Some(stmt) = self.cache.get(sql) {
            return Ok(stmt.clone());
        }

        let stmt = adapter.prepare(sql).await?;
        self.cache.insert(sql.to_string(), stmt.clone());
        Ok(stmt)
    }
}
```

**Benefits**:
- Reuse query plans for repeated patterns
- Reduce parsing overhead
- Improve latency from <8ms to <5ms

#### 4.2: Connection Pool Tuning

**Current**: Single connection pool for all types

**Optimized**: Separate pools per resolution strategy
```rust
struct ConnectionPools {
    local: Arc<ConnectionPool>,           // For local database
    remote: HashMap<String, ConnectionPool>, // Per remote database
    http_client: reqwest::Client,          // Shared HTTP client
}
```

**Benefits**:
- Avoid pool contention
- Per-database tuning
- HTTP client reuse

#### 4.3: Deduplication Optimization

**Optimization**: Use HashMap for O(1) deduplication

```rust
pub fn deduplicate_and_preserve_order(
    reps: &[EntityRepresentation],
) -> Vec<EntityRepresentation> {
    let mut seen = std::collections::HashSet::with_capacity(reps.len());
    let mut result = Vec::with_capacity(reps.len());

    for rep in reps {
        let key = (rep.typename.as_str(), &rep.key_fields);
        if seen.insert(key) {
            result.push(rep.clone());
        }
    }

    result
}
```

**Benefits**:
- O(n) instead of O(n²)
- Preserve input order
- Better cache locality

---

### Task 5: Module Organization

**Current Structure**:
```
federation/
├── mod.rs
├── types.rs
├── entity_resolver.rs
├── representation.rs
└── service_sdl.rs
```

**Improved Structure**:
```
federation/
├── mod.rs                          # Public API
├── types.rs                        # Core types
├── error.rs                        # Error types
├── resolution/                     # Resolution strategies
│   ├── mod.rs
│   ├── trait.rs                    # EntityResolver trait
│   ├── local.rs                    # Local resolution
│   ├── direct_db.rs                # Direct DB resolution
│   └── http.rs                     # HTTP fallback
├── entity_resolver.rs              # Orchestration
├── representation.rs               # Parsing
└── service_sdl.rs                  # SDL generation
```

**Benefits**:
- Clear separation of concerns
- Easier to navigate
- Better for future multi-database support

---

### Task 6: Trait Improvements

**Add generic database adapter support**:

```rust
#[async_trait::async_trait]
pub trait EntityResolver: Send + Sync {
    async fn resolve(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> FederationResult<Vec<JsonValue>>;

    // Optional methods with defaults
    fn supports_batching(&self) -> bool {
        true
    }

    fn max_batch_size(&self) -> usize {
        1000
    }

    async fn validate(&self) -> FederationResult<()> {
        Ok(())
    }
}
```

**Benefits**:
- Extensible for future strategies
- Configurable per strategy
- Self-documenting API

---

### Task 7: Testing Improvements

**Add property-based tests** (proptest):

```rust
#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_deduplication_preserves_first_occurrence(
            reps in prop::collection::vec(entity_representation_strategy(), 1..100)
        ) {
            let deduped = deduplicate(&reps);

            // First occurrence of each key should match original
            for rep in deduped {
                let original_idx = reps.iter().position(|r| r.key_fields == rep.key_fields).unwrap();
                assert_eq!(reps[original_idx], rep);
            }
        }

        #[test]
        fn prop_batch_query_constructs_valid_sql(
            keys in prop::collection::vec("[a-z]+", 1..5)
        ) {
            let query = construct_batch_query("users", &keys);
            // Should be valid SQL
            assert!(query.starts_with("SELECT"));
            assert!(query.contains("WHERE"));
        }
    }
}
```

**Benefits**:
- Catch edge cases
- More confidence in correctness
- Generate many test cases automatically

---

### Task 8: Performance Benchmarking

**Add benchmark suite** with Criterion:

```rust
// benches/federation_benchmarks.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_entity_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("entity_resolution");

    group.bench_function("resolve_1_entity", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Resolve 1 entity
            });
    });

    group.bench_function("resolve_100_entities", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                // Resolve 100 entities
            });
    });

    group.bench_function("deduplicate_1000_entities", |b| {
        b.iter(|| {
            deduplicate_representations(&black_box(large_rep_list()))
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_entity_resolution);
criterion_main!(benches);
```

**Targets**:
- Single entity: <5ms
- 100 entities: <8ms
- Deduplication (1000): <1ms

---

## Refactoring Checklist

- [ ] Resolution strategies extracted to trait
- [ ] Error handling consistent (FederationError enum)
- [ ] All public functions documented
- [ ] Module-level documentation complete
- [ ] Batch queries use prepared statements
- [ ] Connection pools separated by type
- [ ] Deduplication uses HashMap (O(n))
- [ ] Module reorganized with resolution/ subdirectory
- [ ] EntityResolver trait improved with optional methods
- [ ] Property-based tests added
- [ ] Criterion benchmarks pass performance targets
- [ ] No compilation warnings
- [ ] Clippy --all-targets --all-features passes

---

## Validation

```bash
# Check refactoring doesn't break tests
cargo test --test federation

# Performance benchmarks
cargo bench --bench federation_benchmarks

# Expected output
# entity_resolution/resolve_1_entity: ~4.5ms
# entity_resolution/resolve_100_entities: ~7.8ms
# entity_resolution/deduplicate_1000: ~0.8ms

# Code quality
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

---

## Files Modified During Refactoring

### Created
- `crates/fraiseql-core/src/federation/error.rs`
- `crates/fraiseql-core/src/federation/resolution/mod.rs`
- `crates/fraiseql-core/src/federation/resolution/trait.rs`
- `crates/fraiseql-core/src/federation/resolution/local.rs`
- `crates/fraiseql-core/src/federation/resolution/direct_db.rs`
- `crates/fraiseql-core/src/federation/resolution/http.rs`
- `crates/fraiseql-core/benches/federation_benchmarks.rs`

### Modified
- `crates/fraiseql-core/src/federation/mod.rs` (updated with new structure)
- `crates/fraiseql-core/src/federation/types.rs` (added documentation)
- `crates/fraiseql-core/src/federation/entity_resolver.rs` (refactored)
- `crates/fraiseql-core/src/federation/representation.rs` (optimized)
- `crates/fraiseql-core/src/federation/service_sdl.rs` (documentation)
- `crates/fraiseql-core/tests/federation/test_entity_resolver.rs` (added property tests)

---

## Next Phase: CLEANUP

After refactoring:
1. Run linter/formatter
2. Fix any remaining warnings
3. Ensure all documentation complete
4. Run final tests
5. Commit with clear message

---

**Status**: [~] In Progress (Refactoring design)
**Next**: CLEANUP Phase - Linting, formatting, final testing
