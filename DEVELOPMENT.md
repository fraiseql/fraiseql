# FraiseQL v2 Development Guide

Quick reference for developers working on FraiseQL v2.

For current feature set and version status, see `README.md`.

## Quick Start

```bash
# Clone repository
git clone git@github.com:fraiseql/fraiseql.git
cd fraiseql

# Install Rust tools
rustup component add rustfmt clippy rust-analyzer
cargo install cargo-watch cargo-audit cargo-llvm-cov

# Build and test
make build
make test

# Watch for changes during development
make watch-check
```

## Available Features

**Core Engine:**
- GraphQL execution with multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)
- Compiled schema with automatic SQL optimization
- Automatic WHERE type generation with database-specific operators
- Federation support with distributed transaction patterns

**Enterprise Features:**
- Encryption at rest and credential rotation
- Secrets management integration (HashiCorp Vault, environment variables, file-based)
- OAuth2/OIDC authentication with extensible provider system
- Rate limiting and audit logging
- Change Data Capture (CDC) with event dispatch
- Role-based access control (RBAC)
- Multi-tenant data isolation

**Optional Extensions (feature-gated):**
- Arrow Flight for columnar data streaming
- Event system with webhook and action dispatch
- PostgreSQL wire protocol compatibility

See `README.md` for complete and current feature overview.

## Development Commands

```bash
# Build
make build              # Debug build
make build-release      # Release build

# Testing
make test               # Run all tests
make test-integration   # Integration tests (requires PostgreSQL)
make test-e2e          # End-to-end tests
make coverage          # Generate coverage report

# Code Quality
make fmt               # Format code
make clippy            # Run linter
make check             # Run all checks (fmt + clippy + test)

# Documentation
make doc               # Build and open docs

# Benchmarks
make bench             # Run performance benchmarks

# Database
make db-setup-local    # Create test database locally
make db-teardown-local # Drop test database
make db-reset          # Reset test database
make db-up             # Start database container (Docker)
make db-down           # Stop database container

# Development
make watch             # Watch and run tests
make watch-check       # Watch and run checks
```

## Project Structure

```
fraiseql/
├── crates/
│   ├── fraiseql-core/              # Core execution engine (schema, query execution)
│   ├── fraiseql-server/            # HTTP server (Axum-based)
│   ├── fraiseql-cli/               # Compiler CLI tool
│   ├── fraiseql-arrow/             # Arrow Flight support (optional)
│   ├── fraiseql-observers/         # Event system, webhooks, actions (optional)
│   ├── fraiseql-observers-macros/  # Macros for observer system
│   ├── fraiseql-wire/              # PostgreSQL wire protocol (optional)
│   └── fraiseql-error/             # Error types and utilities
├── tests/
│   ├── integration/       # Integration tests (database-dependent)
│   ├── e2e/              # End-to-end tests
│   ├── fixtures/         # Test data and database fixtures
│   └── common/           # Shared test utilities
├── benches/              # Performance benchmarks (Criterion)
├── docs/                 # Architecture and reference documentation
├── tools/                # Development utilities
└── .github/workflows/    # CI/CD pipelines
```

**Note:** Python and TypeScript are authoring languages only (for schema definition). The runtime is pure Rust with no FFI or Python dependencies.

## Code Style

### Formatting

- **Line width**: 100 characters
- **Indentation**: 4 spaces
- **Import organization**: Automatic with `cargo fmt`

```bash
# Format code
make fmt

# Check formatting
cargo fmt --all -- --check
```

### Linting

We use **strict Clippy** with pedantic mode:

```bash
# Run Clippy
make clippy

# Clippy fails on warnings in CI
cargo clippy --all-targets --all-features -- -D warnings
```

**Allowed pedantic lints** (see `Cargo.toml`):

- `too_many_lines` - Some modules will be large
- `module_name_repetitions` - Common pattern in Rust
- `similar_names` - Sometimes unavoidable
- `must_use_candidate` - Use selectively
- `missing_errors_doc` - Can be verbose
- `missing_panics_doc` - Can be verbose

### Documentation

All public items **must** be documented:

```rust
/// Short description (one line)
///
/// Longer description with details.
///
/// # Arguments
///
/// * `arg` - Description
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// When this function returns an error
///
/// # Examples
///
/// ```
/// use fraiseql_core::Example;
/// let result = Example::new();
/// ```
pub fn example(arg: i32) -> Result<String, Error> {
    // ...
}
```

## Testing Strategy

### Test Levels

1. **Unit Tests** (in module files)

   ```rust
   #[cfg(test)]
   mod tests {
       #[test]
       fn test_function() {
           assert_eq!(2 + 2, 4);
       }
   }
   ```

2. **Integration Tests** (`tests/integration/`)

   ```rust
   #[test]
   fn test_module_integration() {
       // Test multiple modules together
   }
   ```

3. **End-to-End Tests** (`tests/e2e/`)

   ```rust
   #[tokio::test]
   async fn test_complete_flow() {
       // Test full request → response flow
   }
   ```

### Test Utilities

Use `tests/common/` for shared helpers:

```rust
use common::{init_test_logging, db, schema, assert, fixtures};

#[tokio::test]
async fn my_test() {
    init_test_logging();
    let pool = db::create_test_pool().await;
    // ... test code
    db::cleanup_test_db(&pool).await;
}
```

### Test Suite

The project includes **2,400+ tests** covering:
- Unit tests (per-module)
- Integration tests (multi-module, database-dependent)
- End-to-end tests (full request/response flows)
- Chaos engineering tests (failure scenarios and consistency)

### Coverage Target

**85%+ line coverage** for all modules.

```bash
make coverage
# Opens target/llvm-cov/html/index.html

# Run specific test suites
cargo test --lib              # Unit tests only
make test-integration         # Integration tests (requires database)
make test-e2e                 # End-to-end tests
```

## Performance

### Benchmarks

Use Criterion for benchmarks:

```bash
make bench
# Open target/criterion/report/index.html
```

**Benchmark template**:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_function(c: &mut Criterion) {
    c.bench_function("name", |b| {
        b.iter(|| {
            // Code to benchmark
            black_box(expensive_function());
        });
    });
}

criterion_group!(benches, bench_function);
criterion_main!(benches);
```

## CI/CD

### GitHub Actions

All PRs must pass:

- ✅ Format check (`cargo fmt`)
- ✅ Clippy lints (`cargo clippy`)
- ✅ Tests on Linux, macOS, Windows
- ✅ Integration tests (PostgreSQL, MySQL)
- ✅ Coverage threshold (85%+)
- ✅ Security audit (`cargo audit`)
- ✅ Documentation build

### Pre-commit Hooks

Install for automatic checks:

```bash
pip install pre-commit
pre-commit install
```

Runs before each commit:

- `cargo fmt`
- `cargo clippy`
- Trailing whitespace
- TOML formatting

## Rust Analyzer

### VSCode Setup

Recommended extensions installed via `.vscode/extensions.json`:

- `rust-lang.rust-analyzer` - Rust language support
- `tamasfe.even-better-toml` - TOML support
- `vadimcn.vscode-lldb` - Debugging

### Configuration

Settings in `.vscode/settings.json`:

- Format on save
- Clippy on check
- Inlay hints enabled
- Auto-import enabled

### Features

- **Code completion** - Smart suggestions
- **Inline errors** - See Clippy warnings inline
- **Quick fixes** - Apply suggestions with one click
- **Go to definition** - Navigate codebase
- **Find references** - See all usages
- **Rename symbol** - Safe refactoring
- **Run tests** - Click to run individual tests

## Debugging

### VSCode Debugging

Configurations in `.vscode/launch.json`:

1. **Debug unit tests**

   ```json
   "name": "Debug unit tests"
   ```

2. **Debug integration tests**

   ```json
   "name": "Debug integration tests"
   "env": { "DATABASE_URL": "..." }
   ```

3. **Debug CLI**

   ```json
   "name": "Debug CLI"
   ```

### Logging

Use `tracing` for structured logging:

```rust
use tracing::{debug, info, warn, error, instrument};

#[instrument]
fn my_function(arg: i32) {
    debug!("Called with arg={}", arg);
    info!("Processing...");
    if error {
        error!("Failed!");
    }
}
```

Set log level with `RUST_LOG`:

```bash
RUST_LOG=debug cargo test
RUST_LOG=fraiseql_core=trace cargo run
```

## Contributing

See `CONTRIBUTING.md` for full guidelines.

### Quick checklist

- [ ] Code compiles without warnings
- [ ] Tests pass (`make test`)
- [ ] Formatted (`make fmt`)
- [ ] Clippy clean (`make clippy`)
- [ ] Documentation updated
- [ ] Tests added for new features

## Resources

- **Project Overview**: `README.md`
- **Architecture & Design**: `.claude/ARCHITECTURE_PRINCIPLES.md`
- **Security Configuration**: `docs/SECURITY_CONFIGURATION.md`
- **Testing Standards**: `TESTING.md`
- **Contributing Guidelines**: `CONTRIBUTING.md`
- **Rust Book**: <https://doc.rust-lang.org/book/>
- **Clippy Lints**: <https://rust-lang.github.io/rust-clippy/>
- **Criterion Guide**: <https://bheisler.github.io/criterion.rs/book/>

---

**Happy coding!** 🦀
