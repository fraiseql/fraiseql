# FraiseQL Development Guide

## What FraiseQL Is

FraiseQL is a schema-driven, multi-protocol data gateway. It validates and enriches type definitions at build time into a single metadata artifact (`schema.compiled.json`), then serves data over six protocols -- generating parameterized SQL dynamically per request against PostgreSQL, MySQL, SQLite, or SQL Server.

## Core Architecture

```
Authoring               Compilation              Runtime
(Python/TS)            (Rust CLI)               (Rust Server)
    ↓                      ↓                        ↓
schema.json    +    fraiseql.toml      →    schema.compiled.json    →    Multi-Protocol Server
(types, queries,     (security, transports,   (enriched types,           (GraphQL, REST, gRPC,
 mutations,           observers, session        config, optimization       Arrow Flight, MCP,
 fact tables)         variables, federation)    hints)                     WebSocket)
```

**Key Points**:
- Python/TypeScript are **authoring languages only** -- no runtime FFI, no language bindings
- The compiled artifact contains **metadata, configuration, and SQL fragments** (mutation function calls, projection hints, filter templates) -- full query construction (WHERE clauses, SELECT projections, JOINs) is generated at runtime by dialect-aware generators
- All six protocols share the **same execution core**: query matching, WHERE clause AST construction, RLS injection, parameterized SQL generation, result projection

**What compilation does**: validates schema correctness, enriches with synthetic types (Relay pagination, 49 rich filter scalar types, aggregate/window inputs), generates JSONB projection hints, embeds all subsystem configuration into one deployable artifact.

**What compilation does NOT do**: produce complete SQL queries, query execution plans, or eliminate per-request SQL generation. (Mutation `sql_source` values and rich filter SQL templates are embedded at compile time; full queries are assembled at runtime.)

---

## Project Standards

### Technology Stack

| Component | Technology | Why |
|-----------|-----------|-----|
| **Core engine** | Rust | Zero-cost abstractions, memory safety |
| **Schema authoring** | Python/TypeScript | Developer ergonomics |
| **Database drivers** | PostgreSQL (primary), MySQL, SQLite, SQL Server | Native Rust drivers only |
| **Testing** | cargo-nextest | 2-3x faster than cargo test |
| **Linting** | Clippy (pedantic + deny) | Strict code quality |

**NOT SUPPORTED**: Oracle (no Rust drivers)

### Code Quality

```toml
# All warnings are errors
clippy::all = "deny"
clippy::pedantic = "deny"
clippy::cargo = "deny"
unsafe_code = "forbid"
```

### Type Annotations

Use modern Python 3.10+ style:

```python
def get_user(user_id: int) -> User | None:  # Good
def get_user(user_id: int) -> Optional[User]:  # Old style, don't use
```

---

## Development Workflow

### Crate Map

| Crate | Role |
|-------|------|
| `fraiseql` | Root binary crate — CLI + server entry point, re-exports feature-gated crates |
| `fraiseql-core` | Schema types, query matching, execution engine, cache, APQ, RLS |
| `fraiseql-server` | HTTP/WebSocket server, route handlers for all six protocols |
| `fraiseql-cli` | Compiler, schema validator/converter/optimizer, CLI commands |
| `fraiseql-db` | Database adapters, WHERE generator, dialect system, rich filters |
| `fraiseql-error` | `FraiseQLError` enum, `Result` type alias, error context |
| `fraiseql-federation` | Apollo Federation v2, entity resolution, saga coordinator |
| `fraiseql-observers` | Event listeners, observer executor, NATS transport, action dispatch |
| `fraiseql-arrow` | Apache Arrow Flight service, columnar streaming |
| `fraiseql-auth` | Authentication providers (JWT, OIDC, API key) |
| `fraiseql-secrets` | HashiCorp Vault integration, secrets backends |
| `fraiseql-webhooks` | Webhook delivery with retry |
| `fraiseql-wire` | PostgreSQL wire protocol implementation |
| `fraiseql-test-utils` | Shared test helpers |

### Workflow Pattern

```bash
# 1. Create feature branch
git checkout -b feature/description

# 2. Implement changes

# 3. Verify build
cargo check
cargo clippy --all-targets --all-features
cargo test

# 4. Commit with descriptive message
git commit -m "feat(scope): Clear description of work

## Changes

- Change 1
- Change 2

## Verification
✅ cargo check passes
✅ cargo clippy passes
✅ tests pass
"

# 5. Push and create PR
git push -u origin feature/description
```

### Fast Development Cycle

```bash
# Watch for changes and auto-check
cargo watch -x check

# Run specific tests
cargo nextest run test_name

# Check with strict linting
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Architecture Guidelines

### 1. Authoring vs Runtime Separation

**Authoring Layer:**
- Python/TypeScript decorators generate `schema.json`
- No runtime Rust calls, pure JSON output

**Compilation Layer (`fraiseql-cli compile`):**
- Validates schema structure (duplicate names, type refs, circular deps, SQL identifiers)
- Enriches with synthetic types (Relay pagination, cascade types, rich filter scalars)
- Generates JSONB projection hints for large types
- Embeds security, transport, observer, and federation configuration
- Outputs `schema.compiled.json`

**Runtime Layer (`fraiseql-server`):**
- Loads `schema.compiled.json` as a lookup table and configuration source
- Generates parameterized SQL dynamically per request via `GenericWhereGenerator<Dialect>`
- Serves six protocols from the same execution core
- Pure Rust, zero Python dependencies

**Key Point**: The server is generic over `DatabaseAdapter` trait, enabling type-safe database swapping and easy testing with mocks.

### 2. Schema Compilation and Configuration Flow

```
┌─────────────────────────┐
│ Developer Setup         │
├─────────────────────────┤
│ 1. Python Code          │
│    @fraiseql.type       │
│    class User:          │
│      id: int            │
│                         │
│ 2. fraiseql.toml        │
│    [security]           │
│    [rest]               │
│    [grpc]               │
│    [observers]          │
│    [session_variables]  │
│    [federation]         │
└────────┬────────────────┘
         │
         ↓ (generates)
┌──────────────────────────┐
│ schema.json +            │
│ fraiseql.toml config     │
└────────┬─────────────────┘
         │
         ↓ (fraiseql-cli compile)
┌─────────────────────────────────┐
│ schema.compiled.json            │
│ {                               │
│   "types": [...],               │
│   "queries": [...],             │
│   "mutations": [...],           │
│   "subscriptions": [...],       │
│   "fact_tables": {...},         │
│   "observers": [...],           │
│   "security": {...},            │
│   "federation": {...},          │
│   "rest_config": {...},         │
│   "grpc_config": {...},         │
│   "mcp_config": {...},          │
│   "observers_config": {...},    │
│   "subscriptions_config": {...},│
│   "session_variables_config":.. │
│ }                               │
└────────┬────────────────────────┘
         │
         ↓ (loaded by)
┌─────────────────────────────────┐
│ fraiseql-server                 │
├─────────────────────────────────┤
│ 1. Load schema.compiled.json   │
│ 2. Build query/mutation indexes│
│ 3. Apply env var overrides     │
│ 4. Initialize protocol routers │
│    - GraphQL + WebSocket       │
│    - REST (auto-generated)     │
│    - gRPC (from proto desc.)   │
│    - Arrow Flight              │
│    - MCP (schema as tools)     │
│ 5. Initialize subsystems       │
│    - Observer event listeners  │
│    - Subscription manager      │
│    - Federation resolver       │
│ 6. Serve requests              │
│    (SQL generated per-request) │
└─────────────────────────────────┘
```

### 3. Multi-Protocol Architecture

All protocols converge on the same execution core. The pattern is:

```
Protocol Handler → Translate to QueryMatch → RLS + WHERE AST → SQL Generation → Execute → Project Results
```

| Protocol | Feature Flag | Port | How it maps to the core |
|----------|-------------|------|-------------------------|
| **GraphQL** | (always on) | 8000 | Direct query matching against `CompiledSchema` |
| **REST** | `rest` | 8000 | Auto-generated resource routes from types; query string → WHERE AST |
| **gRPC** | `grpc` | 50052 | Dynamic dispatch from protobuf descriptors via `prost_reflect` |
| **Arrow Flight** | `arrow` | 50051 | GraphQL query execution → JSON → Arrow RecordBatch streaming |
| **MCP** | `mcp` | 8000 | Each query/mutation exposed as an MCP tool for LLM clients |
| **WebSocket** | (always on) | 8000 | GraphQL subscriptions via `graphql-transport-ws` or `graphql-ws` |

### 4. Database Abstraction

FraiseQL supports multiple databases via **runtime SQL generation**, not ORMs:

- PostgreSQL (primary, most features)
- MySQL (secondary)
- SQLite (local dev, testing)
- SQL Server (enterprise)

SQL is constructed per-request by `GenericWhereGenerator<D: SqlDialect>`, which walks a `WhereClause` AST and emits dialect-specific parameterized SQL. All user values are bound as positional parameters (`$1`/`?`/`@p1`), never interpolated.

**Data model**: Types map to views/tables where fields are extracted from a JSONB column (`data->>'field'`). This decouples the GraphQL schema from the physical table schema.

### 5. Analytical Queries (Fact Tables)

Fact tables follow the `tf_*` naming convention and support aggregation and window functions:

- **Measures**: numeric SQL columns (INT, BIGINT, DECIMAL, FLOAT) for direct aggregation
- **Dimensions**: JSONB `data` column for flexible GROUP BY
- **Calendar dimensions**: pre-computed temporal buckets in JSONB columns (avoids per-query DATE_TRUNC overhead)
- **Aggregates**: COUNT, SUM, AVG, MIN, MAX, STDDEV, VARIANCE, ARRAY_AGG, JSON_AGG, STRING_AGG
- **Window functions**: ROW_NUMBER, RANK, DENSE_RANK, LAG, LEAD, FIRST_VALUE, LAST_VALUE, running totals

### 6. Event System (Observers + Subscriptions)

**Observers** react to database changes with side effects:
```
DB mutation → pg_notify / change_log polling / NATS JetStream
  → EventMatcher → ConditionParser (DSL) → ActionDispatcher
  → webhook, email, Slack, SMS, search index, cache invalidation
  → retry with backoff, dead letter queue
```

**Subscriptions** deliver real-time updates to connected clients:
```
DB mutation → pg_notify → SubscriptionManager → SubscriptionMatcher
  → WebSocket broadcast (graphql-transport-ws or graphql-ws protocol)
```

### 7. Federation (Apollo Federation v2)

FraiseQL acts as a composed subgraph supporting:
- `_entities` resolution (local DB, HTTP to remote subgraph, or direct DB connection)
- `_service { sdl }` auto-generation with federation directives
- Entity batching (max 1,000 per call) with deduplication
- Saga-based distributed transactions with compensation and crash recovery
- Circuit breaker per entity type

### 8. Error Handling

Use `FraiseQLError` enum for all errors:

```rust
pub enum FraiseQLError {
    Parse { message: String, location: Option<String> },
    Validation { message: String, path: Option<String> },
    Database { message: String, code: Option<String> },
    // ... see fraiseql-error crate for full hierarchy
}

pub type Result<T> = std::result::Result<T, FraiseQLError>;
```

### 9. Security

**Runtime security** (evaluated per-request, not at compile time):

| Feature | Mechanism |
|---------|-----------|
| **RLS (Row-Level Security)** | WHERE clause injection from `RLSPolicy::evaluate()` -- tenant isolation, owner-based access, admin bypass |
| **Field-Level RBAC** | Per-field allow/mask/reject based on user scopes |
| **Session Variables** | JWT claims and HTTP headers injected as PostgreSQL GUCs (`SET LOCAL app.user_id = ...`) |
| **Trusted Documents** | Query allowlisting via SHA-256 manifest (strict or permissive mode) |
| **APQ** | Automatic Persisted Queries with in-memory or Redis storage |
| **Rate Limiting** | Per-user/per-IP brute-force protection on auth endpoints |
| **Audit Logging** | Track secret access for compliance |
| **Error Sanitization** | Hide implementation details in error responses |
| **PKCE State Encryption** | Protect OAuth state parameters |
| **Secrets Management** | HashiCorp Vault integration with multiple backends |

All configurable via `fraiseql.toml` and environment variable overrides.

### 10. Testing Strategy

**Unit tests**: Per-module in `mod.rs` or `tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // ...
    }
}
```

**Integration tests**: `tests/` directory with database setup

```rust
#[tokio::test]
async fn test_query_execution() {
    let pool = setup_test_db().await;
    // ...
}
```

**Benchmarks**: Criterion benchmarks for performance analysis

---

## Key Files & Directories

```
fraiseql/
├── .claude/
│   └── CLAUDE.md                    # This file
├── crates/
│   ├── fraiseql-core/              # Schema types, execution engine, cache, RLS
│   ├── fraiseql-server/            # Multi-protocol HTTP/WS/gRPC server
│   ├── fraiseql-cli/               # Compiler CLI (validate, convert, optimize)
│   ├── fraiseql-db/                # Database adapters, WHERE generator, dialects
│   ├── fraiseql-error/             # FraiseQLError, Result, ErrorContext
│   ├── fraiseql-federation/        # Apollo Federation v2, sagas
│   ├── fraiseql-observers/         # Event listeners, NATS, action dispatch
│   ├── fraiseql-arrow/             # Arrow Flight service
│   ├── fraiseql-auth/              # JWT, OIDC, API key authentication
│   ├── fraiseql-secrets/           # Vault integration
│   ├── fraiseql-webhooks/          # Webhook delivery
│   ├── fraiseql-wire/              # PostgreSQL wire protocol
│   └── fraiseql-test-utils/        # Shared test helpers
├── docs/architecture/               # Architecture documentation
├── tools/                           # Dev tooling
└── Cargo.toml                       # Workspace config
```

---

## Common Tasks

### Add a New Database Operation

1. Define trait method in `fraiseql-db/src/traits.rs`
2. Implement for each database in `db/{postgres,mysql,sqlite,sqlserver}/`
3. Add dialect method in `db/dialect/trait_def.rs` if SQL syntax differs
4. Add tests
5. Update documentation

### Add a New Protocol Feature

1. Add config struct in `fraiseql-core/src/schema/config_types.rs`
2. Add route handler in `fraiseql-server/src/routes/`
3. Wire into router in `fraiseql-server/src/routing.rs`
4. Feature-gate with `#[cfg(feature = "...")]`
5. Add integration test

### Add a New Rich Filter Scalar

1. Add operator variant in `fraiseql-db/src/filters/operators.rs`
2. Add per-dialect SQL template in `fraiseql-cli/src/schema/sql_templates.rs`
3. Add WHERE generator case in `fraiseql-db/src/where_generator/generic.rs`
4. Add compiler conversion in `fraiseql-cli/src/schema/converter/`
5. Add tests

### Fix a Bug

1. Write failing test first (TDD)
2. Fix the bug
3. Verify test passes
4. Check no regressions: `cargo test`
5. Commit with `fix(scope):` prefix

---

## Performance Guidelines

### Compilation Speed

- Use `mold` linker on Linux: `sudo pacman -S mold`
- Enable in `.cargo/config.toml`
- Expected speedup: 3-5x faster linking

### Runtime Performance

- Zero-cost abstractions (prefer `impl Trait` over `Box<dyn Trait>`)
- JSONB projection hints reduce payload significantly for large types
- Connection pooling for database connections
- Query result caching with automatic invalidation (64-shard LRU, per-entry TTL)
- Automatic Persisted Queries (APQ) reduce bandwidth for repeated queries
- O(1) query/mutation/subscription name lookups via index maps

### Testing Performance

- Use `cargo nextest` (2-3x faster)
- Parallel test execution: `cargo nextest run --test-threads 8`

---

## Documentation Standards

### Code Documentation

```rust
/// Brief one-line summary.
///
/// Longer description with examples:
///
/// ```
/// let schema = CompiledSchema::from_file("schema.json")?;
/// ```
///
/// # Errors
///
/// Returns `FraiseQLError::Parse` if JSON is invalid.
pub fn from_file(path: &str) -> Result<Self> {
    // ...
}
```

### Commit Messages

```
<type>(<scope>): <description>

## Changes

- Specific change 1
- Specific change 2

## Verification
✅ Tests pass
✅ Clippy clean
```

**Types**: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

---

## Troubleshooting

### Compilation Errors

```bash
# Clean build
cargo clean && cargo check

# Check specific crate
cargo check -p fraiseql-core

# Verbose output
cargo check --verbose
```

### Clippy Warnings

```bash
# Auto-fix where possible
cargo clippy --fix --allow-dirty

# Show all warnings
cargo clippy --all-targets --all-features
```

### Test Failures

```bash
# Run single test with output
cargo test test_name -- --nocapture

# Run tests in specific file
cargo nextest run --test integration_test

# Run with logging
RUST_LOG=debug cargo test
```

---

## Quick Reference

```bash
# Development
cargo watch -x check              # Auto-check on save
cargo nextest run                 # Run tests (fast)
cargo clippy --all-targets        # Lint code

# Build
cargo build                       # Debug build
cargo build --release             # Release build

# Documentation
cargo doc --open                  # Build and open docs

# Aliases (from .cargo/config.toml)
cargo c                           # cargo check
cargo t                           # cargo test
cargo br                          # cargo build --release
```

---

**Remember**: Python/TypeScript are for authoring. Rust is for runtime. SQL is generated per-request, not at compile time.
