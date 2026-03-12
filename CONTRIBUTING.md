# Contributing to FraiseQL v2

Thank you for your interest in contributing to FraiseQL v2! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Development Workflow](#development-workflow)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Release Process](#release-process)

---

## Code of Conduct

Be respectful, professional, and collaborative. We're building something great together!

---

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork**:

   ```bash
   git clone git@github.com:YOUR_USERNAME/fraiseql.git
   cd fraiseql
   ```

3. **Add upstream remote**:

   ```bash
   git remote add upstream git@github.com:fraiseql/fraiseql.git
   ```

---

## Where to Contribute

This repository is a monorepo. Jump to the section that matches your area of interest:

### Rust Engine (most contributors)

Work in: `crates/`

CI commands: `cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run`

| Crate | Purpose |
|-------|---------|
| `fraiseql-core` | Core compilation and execution engine |
| `fraiseql-server` | HTTP/GraphQL server |
| `fraiseql-cli` | Compiler CLI (`fraiseql compile`, `fraiseql serve`) |
| `fraiseql-db` | Database adapters (PostgreSQL, MySQL, SQLite, SQL Server) |
| `fraiseql-auth` | Authentication and authorization |
| `fraiseql-secrets` | Secrets management and field-level encryption |
| `fraiseql-observers` | Event-driven observer system |
| `fraiseql-arrow` | Apache Arrow Flight integration |
| `fraiseql-wire` | Streaming JSON query engine |

Ignore: `sdks/`, `fraisier/`, `examples/`, `k6/`

### Python SDK

Work in: `sdks/official/fraiseql-python/`

CI commands: `uv run pytest`

Ignore: `crates/`, other `sdks/`

### TypeScript SDK

Work in: `sdks/official/fraiseql-typescript/`

CI commands: `npm test`

Ignore: `crates/`, other `sdks/`

### Fraisier (reference implementation)

Work in: `fraisier/`

This is a standalone Python application. See `fraisier/CONTRIBUTING.md` for its own workflow.

CI is independent of the Rust engine — fraisier tests only run on `fraisier/**` changes.

Ignore: `crates/`, `sdks/`

### Documentation

Work in: `docs/`

---

## Development Setup

### Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs/))
- **PostgreSQL** 14+ (for integration tests)
- **Make** (optional, for convenience commands)

### Install Development Tools

```bash
# Install Rust toolchain
rustup toolchain install stable
rustup component add rustfmt clippy rust-analyzer

# Install cargo tools
cargo install cargo-watch cargo-audit cargo-llvm-cov

# Install pre-commit hooks (optional)
pip install pre-commit
pre-commit install
```

### Build the Project

```bash
# Build all crates
make build

# Or with cargo directly
cargo build --all-features
```

### Run Tests

```bash
# Run all tests
make test

# Run integration tests (requires PostgreSQL)
make test-integration

# Run specific test
cargo test test_schema
```

---

## Development Workflow

### 1. Create a Feature Branch

```bash
git checkout v2-development
git pull upstream v2-development
git checkout -b feature/my-feature
```

### 2. Make Changes

- Write code following our [Code Style](#code-style)
- Add tests for new functionality
- Update documentation if needed

### 3. Run Checks Locally

```bash
# Format code
make fmt

# Run linter
make clippy

# Run tests
make test

# Or run all checks
make check
```

### 4. Commit Changes

```bash
git add .
git commit -m "feat(scope): description

- Detailed change 1
- Detailed change 2"
```

**Commit Message Format:**

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style changes (formatting)
- `refactor`: Code refactoring
- `perf`: Performance improvement
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### 5. Push and Create PR

```bash
git push origin feature/my-feature
```

Then create a Pull Request on GitHub targeting `v2-development`.

---

## Code Style

### Rust Style

We follow the official [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/).

**Key points:**

- **Line width**: 100 characters
- **Indentation**: 4 spaces
- **Imports**: Organized with `cargo fmt`
- **Documentation**: Required for public items
- **Error handling**: Use `Result` and `?` operator

**Example:**

```rust
/// Calculate the sum of two numbers.
///
/// # Arguments
///
/// * `a` - First number
/// * `b` - Second number
///
/// # Returns
///
/// Sum of a and b
///
/// # Example
///
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Linting

All code must pass Clippy with no warnings:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### New async traits

New traits used as `dyn Trait` (object-safe, heap-allocated dispatch) **must** use
`#[async_trait]` with the RFC 3425 tracking comment:

```rust
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait MyTrait: Send + Sync {
    async fn my_method(&self) -> Result<()>;
}
```

New traits used only as `impl Trait` or `T: MyTrait` bounds **must not** use
`#[async_trait]` — use RPITIT (return-position `impl Trait` in trait) instead:

```rust
pub trait MyStaticTrait {
    async fn my_method(&self) -> Result<()>;  // RPITIT, no #[async_trait]
}
```

The `make lint-async-trait` CI gate counts `#[async_trait]` usages and fails if the
count grows above the Phase 0 baseline. When RTN + Send stabilises in Rust, all
`#[async_trait]` usages will be migrated and the gate removed.

---

## Testing

See **[`docs/testing.md`](docs/testing.md)** for the full test taxonomy (7 categories,
infrastructure requirements, CI coverage, and decision guide).

### Quick Commands

```bash
make test           # Unit + SQL snapshots + behavioral integration (PostgreSQL)
make test-full      # All categories: unit + snapshots + integration + cross-db + federation
make test-load      # Load testing (requires running server + k6)
make coverage       # Generate test coverage report (target/llvm-cov/html/index.html)
```

### Infrastructure

```bash
make db-up          # Start test databases (PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault)
make db-down        # Stop test databases
make db-reset       # Reset volumes (after schema changes)
```

### Updating SQL Snapshots

If you change the SQL compiler, snapshot tests will fail. To update them:

```bash
# Accept all snapshot changes
INSTA_UPDATE=accept cargo nextest run --test sql_snapshots

# Review each change interactively
cargo insta review

# Commit the updated .snap files
git add crates/fraiseql-core/tests/snapshots/
git commit -m "test(sql): update SQL snapshots after compiler change"
```

**Important**: Review every changed snapshot to verify the new SQL is correct,
not just different.

### Code Coverage

Per-crate coverage floors are enforced in CI:

| Crate | Floor | Rationale |
|-------|-------|-----------|
| `fraiseql-core` | 65% | SQL generation, RLS enforcement, field masking |
| `fraiseql-db` | 65% | Dialect SQL output, injection escaping |
| `fraiseql-auth` | 80% | Authentication paths |
| `fraiseql-secrets` | 80% | Encryption, key management |
| Workspace | 70% | Baseline floor |

Floors are regression guards, not targets — aim for the highest coverage the code naturally
supports, not just the minimum. When adding new code:

- New public functions should have at least one test covering the success path.
- Error branches should be tested where the error is actionable by a caller.

To check coverage locally:

```bash
# Per-crate HTML report
cargo llvm-cov -p fraiseql-core --html
open target/llvm-cov/html/index.html

# Workspace summary
cargo llvm-cov --workspace --summary-only
```

---

## Pull Request Process

### PR Checklist

Before submitting a PR, ensure:

- [ ] Code compiles without warnings
- [ ] All tests pass (`make test`)
- [ ] Code is formatted (`make fmt`)
- [ ] Clippy passes (`make clippy`)
- [ ] Documentation is updated (if needed)
- [ ] Tests are added for new functionality
- [ ] Commit messages follow conventional format
- [ ] PR description explains the change

### PR Review Process

1. **Automated checks** run via GitHub Actions
2. **Code review** by maintainers
3. **Address feedback** if requested
4. **Merge** once approved and CI passes

### After Merge

The PR will be merged into `v2-development`. Your contribution will be included in the next release!

---

## Release Process

Releases are managed by maintainers:

1. Version bump in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create git tag (`v2.x.x`)
4. CI builds and publishes to crates.io and PyPI

---

## Architecture Guidelines

FraiseQL v2 is a **compiled GraphQL execution engine**. Key principles:

### 1. Separation of Concerns

- **Compilation Layer**: Schema definition → SQL compilation (build-time via fraiseql-cli)
- **Runtime Layer**: Query execution → Result streaming (runtime via fraiseql-server)
- **Database Layer**: Data storage and retrieval (multi-database support)

See [`docs/architecture/overview.md`](docs/architecture/overview.md) for detailed architecture documentation.

### 2. Layered Optionality

- **Core**: Minimal build includes GraphQL execution engine only
- **Extensions**: Optional features via Cargo features (Arrow, Observers, Wire)
- **Configuration**: All behavior controlled via fraiseql.toml or environment variables

### 3. World-Class Engineering

- **No `unsafe` code** (forbidden at compile time via Cargo.toml lints)
- **Comprehensive error handling** with Result types and context
- **Extensive documentation** for all public APIs
- **Thorough testing** (2,400+ tests: unit, integration, E2E, chaos)
- **Performance-conscious** design with zero-copy patterns and compile-time optimization

---

## Build Performance

### Faster Linking (Linux)

To dramatically speed up compilation, install the `mold` linker for 3-5x faster builds:

**Arch Linux:**
```bash
sudo pacman -S mold
```

**Ubuntu/Debian:**
```bash
sudo apt-get install mold
```

**Other distributions:**
See [mold releases](https://github.com/rui314/mold/releases)

The mold linker is configured in `.cargo/config.toml` and used automatically.
- Full rebuild: ~60s → ~15s
- Incremental linking: 2-5s → 0.5s

---

## Unwrap Policy

Production code (`src/` files outside test modules) must not use `.unwrap()` directly.

Instead:
- Use `.expect("reason why this cannot fail")` — panics with context on failure
- Use `#[allow(clippy::unwrap_used)] // Reason: <justification>` only when the
  unwrap is in an infallible code path that clippy cannot statically prove safe

All new `#[allow(clippy::unwrap_used)]` annotations **must** include a `// Reason:` comment.

**Enforcement**: `clippy::unwrap_used = "deny"` is set at the workspace level
(`Cargo.toml [workspace.lints.clippy]`). Any new `.unwrap()` in production code —
i.e. outside a `#[cfg(test)]` block or a test file — will **fail the build**.
The `cargo clippy --workspace -- -D warnings` CI check catches this automatically.

The secondary gate (`make lint-unwrap`) counts `#[allow(clippy::unwrap_used)]` annotations
in production code and enforces a maximum (currently 1). This prevents annotation
proliferation: each annotation represents a deliberate exception rather than a
suppressed violation. To raise the baseline, update `UNWRAP_ALLOW_LIMIT` in the
`Makefile` and include a PR comment explaining why each new allow is necessary.

Test code (`#[cfg(test)]` modules and `tests/` files) may use `.unwrap()` freely
and must declare `#[allow(clippy::unwrap_used)] // Reason: test code` at the module level.

Empty or placeholder `.expect()` calls (`.expect("")`, `.expect("TODO")`) are also
rejected by `make lint-expect` — they are functionally equivalent to `.unwrap()`.

---

## Getting Help

- **Questions**: Open a GitHub Discussion
- **Bugs**: File a GitHub Issue
- **Security**: Email <security@fraiseql.dev>

---

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (MIT OR Apache-2.0).

---

**Thank you for contributing to FraiseQL v2!** 🚀
