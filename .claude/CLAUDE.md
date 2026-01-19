# FraiseQL v2 Development Guide

## Vision

**FraiseQL v2 is a compiled GraphQL execution engine** that transforms schema definitions into optimized SQL at build time, eliminating runtime overhead for deterministic, high-performance query execution.

## Core Architecture Principle

```
Authoring (Python/TS) → Compilation (Rust) → Runtime (Rust)
         ↓                      ↓                    ↓
   schema.json        schema.compiled.json    GraphQL Server
```

**Key Point**: Python/TypeScript are **authoring languages only**. No runtime FFI, no language bindings needed.

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
def get_user(user_id: int) -> User | None:  # ✅ Good
def get_user(user_id: int) -> Optional[User]:  # ❌ Old style
```

---

## Development Workflow

### Implementation Status

FraiseQL v2 is in active development with core engine, database abstraction, caching, and GraphQL execution already implemented. See `.claude/IMPLEMENTATION_ROADMAP.md` for detailed feature status.

### Workflow Pattern

```bash
# 1. Create feature branch
git checkout -b feature/description

# 2. Implement changes
# Follow .claude/IMPLEMENTATION_ROADMAP.md

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

- Python/TypeScript decorators
- Generate `schema.json`
- NO runtime Rust calls
- Pure JSON output

**Compilation Layer:**

- `fraiseql-cli compile schema.json`
- Validate schema structure
- Generate optimized SQL templates
- Output `schema.compiled.json`

**Runtime Layer:**

- Load `schema.compiled.json`
- Execute GraphQL queries
- Pure Rust, zero Python dependencies

### 2. Schema Compilation Flow

```
┌─────────────────┐
│ Python Code     │
│ @fraiseql.type  │
│ class User:     │
│   id: int       │
└────────┬────────┘
         │
         ↓ (generates)
┌─────────────────┐
│ schema.json     │
│ {               │
│   "types": [...] │
│ }               │
└────────┬────────┘
         │
         ↓ (fraiseql-cli compile)
┌─────────────────┐
│ schema.compiled │
│ .json           │
│ Optimized SQL   │
└────────┬────────┘
         │
         ↓ (loaded by)
┌─────────────────┐
│ fraiseql-server │
│ Execute queries │
└─────────────────┘
```

### 3. Database Abstraction

FraiseQL supports multiple databases via **runtime SQL generation**, not ORMs:

- PostgreSQL (primary, most features)
- MySQL (secondary)
- SQLite (local dev, testing)
- SQL Server (enterprise)

**Pattern**: Write database-agnostic traits, implement per-database SQL generation.

### 4. Error Handling

Use `FraiseQLError` enum for all errors:

```rust
pub enum FraiseQLError {
    Parse { message: String, location: Option<String> },
    Validation { message: String, path: Option<String> },
    Database { message: String, code: Option<String> },
    // ... see error.rs for full hierarchy
}

pub type Result<T> = std::result::Result<T, FraiseQLError>;
```

### 5. Testing Strategy

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
// tests/integration/schema_test.rs
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
│   ├── CLAUDE.md                    # This file
│   └── IMPLEMENTATION_ROADMAP.md    # Feature implementation status
├── crates/
│   ├── fraiseql-core/              # Core execution engine
│   ├── fraiseql-server/            # HTTP server
│   ├── fraiseql-cli/               # Compiler CLI
│   └── fraiseql-wire/              # Wire protocol
├── docs/                            # Architecture docs
├── tools/                           # Dev tooling
└── Cargo.toml                       # Workspace config
```

---

## Common Tasks

### Add a New Database Operation

1. Define trait in `db/mod.rs`
2. Implement for each database in `db/postgres.rs`, `db/mysql.rs`, etc.
3. Add tests in `db/tests.rs`
4. Update documentation

### Add a New GraphQL Feature

1. Update schema types in `schema/compiled.rs`
2. Update compiler in `compiler/` (Phase 4)
3. Update runtime in `runtime/` (Phase 5)
4. Add end-to-end test
5. Update docs

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
- Enable in `.cargo/config.toml` (currently commented out)
- Expected speedup: 3-5x faster linking

### Runtime Performance

- Zero-cost abstractions (prefer `impl Trait` over `Box<dyn Trait>`)
- Compile-time schema optimization (Phase 4)
- Connection pooling (Phase 2)
- Query result caching with coherency (Phase 2)
- APQ for repeated queries (Phase 1 ✅)

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

## Next Steps

See `.claude/IMPLEMENTATION_ROADMAP.md` for detailed feature implementation status and priority order. Current focus areas:

- Performance optimization and benchmarking
- Additional database backend support
- Enhanced schema validation
- Improved error handling and observability

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

**Remember**: Python/TypeScript are for authoring. Rust is for runtime. Keep them separated.
