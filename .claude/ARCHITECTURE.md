# FraiseQL v2 Architecture Overview

## Design Philosophy

FraiseQL v2 is a **compiled GraphQL execution engine** that transforms schema definitions into optimized SQL at build time.

**Core principle**: **Separate schema definition from execution artifacts**

This separation enables:
- Database-specific optimizations without changing schema
- Schema reuse across backends (PostgreSQL, MySQL, SQLite, SQL Server)
- Simplified testing and maintenance
- Strong security guarantees via parameterized queries

## Component Architecture

```
┌──────────────────────────────────────────────────────────┐
│         GraphQL Schema Definition                        │
│   (Type definitions, fields, relationships, queries)    │
│   Authoring: Python / TypeScript / YAML / CLI          │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
        ┌────────────────────────────┐
        │  Schema Compiler           │
        │  Multi-phase processing:   │
        │  - Parse                   │
        │  - Validate                │
        │  - Lower                   │
        │  - Codegen                 │
        └────────────┬───────────────┘
                     │
        ┌────────────┴───────────────┐
        ▼                            ▼
  ┌──────────────┐          ┌─────────────────┐
  │ SQL          │          │ Runtime         │
  │ Templates    │          │ Artifacts       │
  │ (per-DB)     │          │ (Schema defs)   │
  └──────────────┘          └─────────────────┘
        │                            │
        └────────────┬───────────────┘
                     ▼
        ┌────────────────────────────┐
        │  Runtime Executor          │
        │  - Query Validation        │
        │  - Authorization           │
        │  - SQL Generation/Exec     │
        │  - Result Projection       │
        └────────────┬───────────────┘
                     │
                     ▼
        ┌────────────────────────────┐
        │  Database Adapter          │
        │  PostgreSQL / MySQL /      │
        │  SQLite / SQL Server       │
        └────────────┬───────────────┘
                     │
                     ▼
        ┌────────────────────────────┐
        │  Transactional Database    │
        │  - Tables (tb_*)           │
        │  - Views (v_*)             │
        │  - Procedures (fn_*)       │
        │  - CDC Events              │
        └────────────────────────────┘
```

## Compilation Pipeline

The compiler transforms schema definitions through multiple phases:

```
INPUT: schema.json (from decorators)
  │
  ├─→ Phase 1: Parser (parser.rs)
  │   Converts JSON → Authoring IR
  │   - Syntax validation
  │   - AST construction
  │
  ├─→ Phase 2: Validator (validator.rs)
  │   Type checking and semantic validation
  │   - Field type binding
  │   - Circular reference detection
  │   - Auth rule validation
  │
  ├─→ Phase 3: Lowering (lowering.rs)
  │   IR optimization for execution
  │   - Fact table extraction
  │   - Query optimization
  │   - Template preparation
  │
  ├─→ Phase 4: SQL Generation (separate pipeline)
  │   Database-specific SQL templates
  │   - Parameterized queries
  │   - Index optimization hints
  │   - Query plan caching
  │
  └─→ Phase 5: Codegen (codegen.rs)
      Generate CompiledSchema
      - Runtime metadata
      - Schema introspection data
      - Field mappings

OUTPUT: CompiledSchema + SQL Templates (ready for runtime)
```

## Security Model

### Threat: SQL Injection

**Attack Vector**: User input directly interpolated into SQL strings

**Prevention Mechanisms**:
1. **Parameterized Queries**: All user values passed as database parameters
2. **Type System**: `u32` for limits/offsets can't contain SQL code
3. **Compile-Time Validation**: Column names fixed at schema definition time
4. **Identifier Regex**: GraphQL names validated against `^[a-zA-Z_][a-zA-Z0-9_]*$`

**Example**:
```rust
// Safe: value is parameter, never interpolated
// SELECT * FROM users WHERE id = $1
// Parameters: [user_id_from_input]

// Unsafe: string interpolation (not used in FraiseQL)
// SELECT * FROM users WHERE id = '{}' -- ❌ NEVER
```

### Threat: Denial of Service

**Attack Vector**: Expensive queries consuming resources

**Prevention Mechanisms**:
1. **Query Timeouts**: Configurable per execution
2. **Connection Pool Limits**: Max concurrent connections
3. **Result Streaming**: Avoid loading entire results in memory
4. **Nesting Depth Limits**: Prevent deeply nested JSONB extraction (planned)
5. **Result Size Limits**: Configurable byte cap on responses (planned)

### Threat: Data Races

**Attack Vector**: Concurrent access to shared state without synchronization

**Prevention Mechanisms**:
1. **Interior Mutability**: `Cell<T>` for single-threaded contexts only
2. **Thread-Safe Collections**: `Arc<T>` for shared immutable data
3. **Atomic Operations**: `AtomicUsize` for counters when needed
4. **Rust Type System**: Enforces ownership and borrowing rules

**Example**:
```rust
// Single-threaded context - safe with Cell
param_counter: Cell<usize>,  // ✅ Safe

// Multi-threaded context - must use Atomic
param_counter: AtomicUsize,  // ✅ Safe across tasks
```

See `crates/fraiseql-core/docs/SECURITY_PATTERNS.md` for detailed security documentation.

## Performance Considerations

### Query Optimization

- **Compile-time schema optimization**: Constants and invariants resolved at build time
- **Database-specific SQL generation**: Leverage database-specific features
- **Query result streaming**: Process results incrementally, not all at once
- **Parameterized query caching**: Database driver can cache query plans

### Memory Management

- **Zero-copy abstractions**: Avoid unnecessary allocations
- **Streaming results**: Don't load entire result sets in memory
- **Arc for shared data**: Reference counting, no deep copies
- **Immutable intermediate state**: GC-friendly data structures

### Compilation Speed

- **Incremental compilation**: Only recompile changed schemas
- **Linker optimization**: Use `mold` on Linux for 3-5x faster linking
- **Parallel test execution**: `cargo nextest` (2-3x faster than `cargo test`)

## Database Support

### Primary: PostgreSQL

- All features supported
- Advanced features: JSONB, window functions, CTEs
- Performance: Full query optimization

### Secondary: MySQL 8.0+

- Core features supported
- JSON paths supported
- Performance: Optimized for MySQL-specific execution

### Tertiary: SQLite

- Development and testing
- Limited advanced features
- Performance: Single-threaded, suitable for local dev

### Enterprise: SQL Server 2019+

- Core features supported
- JSON paths via SQL Server JSON functions
- Performance: Optimized for SQL Server execution

## Testing Strategy

See `skills/fraiseql-testing.md` for complete testing documentation.

### Unit Tests

Per-module tests in `mod.rs` or `tests.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_where_clause_generation() {
        // Test specific behavior
    }
}
```

### Integration Tests

Full schema compilation and execution tests:
```rust
#[tokio::test]
async fn test_query_execution() {
    let pool = setup_test_db().await;
    let schema = compile_schema(schema_json)?;
    let result = execute_query(&schema, &pool, query).await?;
    assert!(result.is_ok());
}
```

### Security Tests

SQL injection and thread-safety scenarios:
```rust
#[test]
fn test_sql_injection_prevention() {
    let generator = PostgresWhereGenerator::new();
    let injection_attempt = r#"'; DROP TABLE users; --"#;
    // Verify injection is parameterized, never interpolated
}
```

## Key Design Decisions

### 1. Separation of Concerns

**Decision**: Schema definition kept separate from SQL templates

**Rationale**:
- Allows testing schema independently from SQL generation
- Enables different SQL strategies per database
- Simplifies debugging and maintenance

**Trade-off**: Slightly more code to maintain (separate codegen phases)

### 2. Immutable Intermediate State

**Decision**: Each compiler phase produces immutable data structures

**Rationale**:
- Ensures reproducible builds (same input = same output)
- Enables thread-safe processing
- Clear data flow through pipeline

**Trade-off**: Cannot optimize in-place; allocate intermediate structures

### 3. Rust for Runtime

**Decision**: Runtime implemented in Rust, not Python/TypeScript

**Rationale**:
- Performance (zero-cost abstractions, memory safety)
- Type safety prevents entire classes of bugs
- No FFI overhead, no runtime Python interpreter

**Trade-off**: Schema must be compiled before use (not dynamic)

### 4. No Runtime Resolvers

**Decision**: All logic belongs in the database, not query resolvers

**Rationale**:
- Deterministic behavior (no runtime side effects)
- Database can optimize across operations
- Easier to reason about performance

**Trade-off**: Complex logic must be expressed as SQL (views, procedures)

## Directory Structure

```
fraiseql/
├── .claude/
│   ├── CLAUDE.md                    # Development guide
│   ├── IMPLEMENTATION_ROADMAP.md    # Feature implementation status
│   └── ARCHITECTURE.md              # This file
│
├── crates/
│   ├── fraiseql-core/              # Core execution engine
│   │   ├── src/
│   │   │   ├── compiler/           # Schema compilation pipeline
│   │   │   ├── db/                 # Database adapters
│   │   │   ├── schema/             # Runtime schema structures
│   │   │   └── error.rs            # Error types
│   │   ├── docs/
│   │   │   └── SECURITY_PATTERNS.md # Security documentation
│   │   └── tests/                  # Integration tests
│   │
│   ├── fraiseql-server/            # HTTP server
│   ├── fraiseql-cli/               # Compiler CLI
│   └── fraiseql-wire/              # Wire protocol
│
├── tests/                          # End-to-end tests
├── docs/                           # Architecture documentation
└── README.md                       # Project overview
```

## Error Handling

FraiseQL uses a typed error enum for all errors:

```rust
pub enum FraiseQLError {
    Parse {
        message: String,
        location: Option<String>,
    },
    Validation {
        message: String,
        path: Option<String>,
    },
    Database {
        message: String,
        code: Option<String>,
    },
    // ... more variants
}

pub type Result<T> = std::result::Result<T, FraiseQLError>;
```

This enables:
- Precise error categorization
- Context-aware error messages
- Proper error propagation through compilation phases

## Future Enhancements

### Planned Features

1. **Query Complexity Analysis**: Cost-based limits on complex queries
2. **Automatic Schema Optimization**: Detect and optimize common patterns
3. **Advanced CDC Support**: First-class Change Data Capture integration
4. **Query Result Caching**: With automatic invalidation
5. **Performance Profiling**: Built-in query performance analysis

### Potential Improvements

1. **Incremental Compilation**: Only recompile changed schemas
2. **Distributed Execution**: Support for cross-database queries
3. **GraphQL Subscriptions**: Real-time query results via CDC
4. **Custom Type Handlers**: User-defined scalar types
5. **Schema Versioning**: Multiple schema versions in single runtime

## Related Documentation

- `.claude/CLAUDE.md`: Development standards and workflow
- `.claude/IMPLEMENTATION_ROADMAP.md`: Feature implementation status
- `crates/fraiseql-core/docs/SECURITY_PATTERNS.md`: Security patterns
- `skills/fraiseql-testing.md`: Testing strategy and patterns

---

**Document Status**: Current as of 2026-01-19
**Last Updated By**: Architecture Phase 2 Documentation
**Version**: 2.0.0-alpha.1
