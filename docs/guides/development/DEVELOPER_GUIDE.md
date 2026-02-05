<!-- Skip to main content -->
---
title: FraiseQL Developer Guide
description: Welcome to the FraiseQL development guide! This document covers setup, development workflow, testing, and contribution guidelines.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# FraiseQL Developer Guide

Welcome to the FraiseQL development guide! This document covers setup, development workflow, testing, and contribution guidelines.

## Table of Contents

1. [Development Setup](#development-setup)
2. [Project Structure](#project-structure)
3. [Development Workflow](#development-workflow)
4. [Testing Strategy](#testing-strategy)
5. [Code Standards](#code-standards)
6. [Debugging & Troubleshooting](#debugging--troubleshooting)
7. [Contributing](#contributing)

## Development Setup

### Prerequisites

- **Rust 1.75+**: Install from [rustup.rs](https://rustup.rs/)
- **PostgreSQL 14+**: Required for integration tests
- **Git**: Version control
- **Optional**: `cargo-watch` for auto-rebuild on file changes

### Initial Setup

```bash
<!-- Code example in BASH -->
# Clone the repository
git clone https://github.com/FraiseQL/FraiseQL.git
cd FraiseQL

# Install Rust (if needed)
rustup update

# Verify installation
cargo --version  # Should be 1.75+
rustc --version

# Install optional tools
cargo install cargo-watch  # Auto-rebuild
cargo install cargo-edit   # Dependency management
```text
<!-- Code example in TEXT -->

### Database Setup

For integration tests, you need PostgreSQL:

```bash
<!-- Code example in BASH -->
# Start PostgreSQL (if using Docker)
docker run -d \
  --name postgres-FraiseQL \
  -e POSTGRES_PASSWORD=password \
  -e POSTGRES_DB=fraiseql_test \
  -p 5432:5432 \
  postgres:15

# Or connect to existing PostgreSQL
export DATABASE_URL="postgresql://user:password@localhost/fraiseql_test"
```text
<!-- Code example in TEXT -->

### First Build

```bash
<!-- Code example in BASH -->
# Full build (release)
cargo build --release

# Or debug build (faster)
cargo build

# Run tests
cargo test --lib

# Run full test suite
cargo test

# Check formatting
cargo fmt --check

# Run linter
cargo clippy --all-targets --all-features
```text
<!-- Code example in TEXT -->

## Project Structure

```text
<!-- Code example in TEXT -->
FraiseQL/
├── crates/
│   ├── FraiseQL-core/          # Core execution engine
│   │   ├── src/
│   │   │   ├── compiler/       # Schema compilation pipeline
│   │   │   ├── runtime/        # Query execution
│   │   │   ├── cache/          # Query result caching
│   │   │   ├── db/             # Database adapters
│   │   │   └── schema/         # Schema definitions
│   │   └── tests/              # Integration tests
│   │
│   ├── FraiseQL-server/        # HTTP server
│   │   ├── src/
│   │   │   ├── routes/         # HTTP endpoints
│   │   │   ├── middleware/     # Request middleware
│   │   │   ├── config.rs       # Server configuration
│   │   │   └── server.rs       # Server implementation
│   │   └── benches/            # Performance benchmarks
│   │
│   ├── FraiseQL-cli/           # CLI tool
│   │   ├── src/
│   │   │   ├── commands/       # CLI subcommands
│   │   │   └── main.rs
│   │   └── tests/
│   │
│   └── FraiseQL-wire/          # Protocol layer
│       └── src/                # PostgreSQL wire protocol
│
├── docs/                        # Documentation
│   ├── LINTING.md              # Code quality guide
│   ├── DEVELOPER_GUIDE.md      # This file
│   ├── PERFORMANCE.md          # Performance tuning
│   └── architecture/           # Architecture docs
│
├── tools/                       # Development tools
│   └── scripts/                # Build scripts
│
├── Cargo.toml                  # Workspace root
├── Cargo.lock                  # Dependency lock file
└── .cargo/config.toml          # Cargo configuration
```text
<!-- Code example in TEXT -->

## Development Workflow

### Setting Up a Feature Branch

```bash
<!-- Code example in BASH -->
# Create feature branch from dev
git checkout dev
git pull origin dev
git checkout -b feature/your-feature-name

# Verify clean state
git status  # Should show "nothing to commit, working tree clean"
```text
<!-- Code example in TEXT -->

### Development Cycle

```bash
<!-- Code example in BASH -->
# 1. Make changes
vim crates/FraiseQL-core/src/some_file.rs

# 2. Format code
cargo fmt

# 3. Run clippy
cargo clippy --all-targets --all-features

# 4. Run tests
cargo test --lib

# 5. Run integration tests (if needed)
cargo test --test '*'

# 6. Commit
git add .
git commit -m "feat(core): Add new feature description"

# 7. Push and create PR
git push -u origin feature/your-feature-name
```text
<!-- Code example in TEXT -->

### Using cargo-watch for Fast Iteration

```bash
<!-- Code example in BASH -->
# Auto-check on file changes
cargo watch -x check

# Auto-test on changes
cargo watch -x "test --lib"

# Auto-build in another terminal
cargo watch -x build
```text
<!-- Code example in TEXT -->

### Running Specific Tests

```bash
<!-- Code example in BASH -->
# Test specific module
cargo test -p FraiseQL-core cache::result::tests

# Run single test
cargo test test_cache_hit -- --exact

# Run with output
cargo test test_feature -- --nocapture

# Run ignored tests
cargo test -- --ignored

# Run with logging
RUST_LOG=debug cargo test test_feature -- --nocapture
```text
<!-- Code example in TEXT -->

## Testing Strategy

### Test Organization

Tests are organized by scope:

```text
<!-- Code example in TEXT -->
Unit Tests (same file)          - Fast, isolated
Integration Tests (tests/)      - Medium speed, database
Benchmark Tests (benches/)      - Performance regression
Doc Tests (in comments)         - API examples
```text
<!-- Code example in TEXT -->

### Writing Tests

```rust
<!-- Code example in RUST -->
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_happy_path() {
        let input = setup_fixture();
        let result = function_under_test(input);
        assert_eq!(result, expected_output);
    }

    #[test]
    fn test_feature_error_case() {
        let invalid_input = create_invalid_fixture();
        let result = function_under_test(invalid_input);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_async_function() {
        let result = async_function().await;
        assert_eq!(result, expected);
    }
}
```text
<!-- Code example in TEXT -->

### Test Coverage

Aim for:

- **Critical paths**: 100% coverage (business logic)
- **Error handling**: 100% coverage (every error branch)
- **Edge cases**: 90%+ coverage
- **Overall**: 80%+ coverage

Run coverage locally:

```bash
<!-- Code example in BASH -->
# Using tarpaulin
cargo tarpaulin --out Html --output-dir coverage

# View results
open coverage/index.html
```text
<!-- Code example in TEXT -->

### Performance Testing

```bash
<!-- Code example in BASH -->
# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench -- cache_hit

# Compare against baseline
cargo bench -- --baseline main
```text
<!-- Code example in TEXT -->

## Code Standards

### Naming Conventions

```rust
<!-- Code example in RUST -->
// Modules: lowercase_snake_case
mod query_matcher { }

// Types: PascalCase
struct QueryMatcher { }
enum QueryType { }
trait QueryExecutor { }

// Constants: SCREAMING_SNAKE_CASE
const MAX_QUERY_DEPTH: usize = 50;

// Functions: lowercase_snake_case
fn execute_query(query: &str) -> Result<String> { }

// Generic types: PascalCase
fn generic_function<T: Trait>(value: T) { }
```text
<!-- Code example in TEXT -->

### Documentation

Every public item must have documentation:

```rust
<!-- Code example in RUST -->
/// Brief summary.
///
/// Longer description with examples and important notes.
///
/// # Arguments
/// * `param` - Description
///
/// # Returns
/// Description of return value
///
/// # Errors
/// Description of error conditions
///
/// # Example
/// ```text
<!-- Code example in TEXT -->
/// let result = function()?;
/// ```text
<!-- Code example in TEXT -->
pub fn function(param: Type) -> Result<Value> {
}
```text
<!-- Code example in TEXT -->

### Error Handling

```rust
<!-- Code example in RUST -->
// ❌ Avoid panics in library code
fn parse_schema(json: &str) -> Schema {
    serde_json::from_str(json).unwrap()  // Bad!
}

// ✅ Return Result
fn parse_schema(json: &str) -> Result<Schema> {
    serde_json::from_str(json)
        .map_err(|e| FraiseQLError::Parse { message: e.to_string() })
}
```text
<!-- Code example in TEXT -->

### Type Annotations

```rust
<!-- Code example in RUST -->
// ❌ Old style
fn get_user(id: i32) -> Option<User> {
    None
}

// ✅ Modern style (Rust 1.65+)
fn get_user(id: i32) -> User | None {
    None
}

// ✅ For collections
let users: Vec<User> = vec![];
let mapping: HashMap<String, u64> = HashMap::new();
```text
<!-- Code example in TEXT -->

## Debugging & Troubleshooting

### Logging

Use the `log` crate for structured logging:

```rust
<!-- Code example in RUST -->
use log::{debug, info, warn, error};

info!("Server starting on {}", addr);
debug!(field = %value, "Detailed debug info");
error!("Failed to execute query: {}", err);
```text
<!-- Code example in TEXT -->

Enable logging in tests:

```bash
<!-- Code example in BASH -->
RUST_LOG=debug cargo test test_name -- --nocapture
```text
<!-- Code example in TEXT -->

### Debugging with Print Statements

```rust
<!-- Code example in RUST -->
// Quick debug print (temporary)
eprintln!("Debug: {:?}", value);

// Better: use dbg! macro
dbg!(&value);

// Best: use proper logging
debug!("Value: {:?}", value);
```text
<!-- Code example in TEXT -->

### Common Issues & Solutions

#### Lifetime Errors

```rust
<!-- Code example in RUST -->
// ❌ Lifetime mismatch
fn process<'a>(input: &'a str) -> &'static str {
    input  // Error: different lifetime
}

// ✅ Return owned data
fn process(input: &str) -> String {
    input.to_string()
}
```text
<!-- Code example in TEXT -->

#### Borrow Checker Issues

```rust
<!-- Code example in RUST -->
// ❌ Multiple mutable borrows
let mut x = vec![1, 2, 3];
let a = &mut x;
let b = &mut x;  // Error!

// ✅ Use references sequentially
let mut x = vec![1, 2, 3];
{
    let a = &mut x;
    a.push(4);
}
let b = &mut x;  // OK
```text
<!-- Code example in TEXT -->

#### Async Issues

```rust
<!-- Code example in RUST -->
// ❌ Not awaiting
async fn fetch_data() -> Data {
    get_data()  // Error: forgot await
}

// ✅ Proper await
async fn fetch_data() -> Data {
    get_data().await
}
```text
<!-- Code example in TEXT -->

### Profiling

```bash
<!-- Code example in BASH -->
# Generate flamegraph
cargo install flamegraph
cargo flamegraph

# View results
open flamegraph.svg
```text
<!-- Code example in TEXT -->

## Contributing

### Before You Start

1. **Check existing issues**: Is this already being worked on?
2. **Create an issue**: Discuss breaking changes, new features
3. **Understand the code**: Read related documentation first
4. **Test locally**: Ensure no regressions

### Creating a Pull Request

1. **Ensure clean history**: Squash fixup commits

   ```bash
<!-- Code example in BASH -->
   git rebase -i main
   ```text
<!-- Code example in TEXT -->

2. **Write clear commit messages**:

   ```text
<!-- Code example in TEXT -->
   feat(scope): Add feature description

   Longer explanation of what and why.

   Fixes #123
   ```text
<!-- Code example in TEXT -->

3. **Add tests**: For new features and bug fixes

4. **Update documentation**: If public API changes

5. **Run full checks**:

   ```bash
<!-- Code example in BASH -->
   cargo fmt
   cargo clippy --all-targets --all-features
   cargo test --lib
   cargo test --test '*'
   ```text
<!-- Code example in TEXT -->

### PR Review Process

- **Maintainers** will review within 48 hours
- **Address feedback**: Commit updates, don't force push
- **Approval**: Usually 1-2 approvals
- **Merge**: Rebase and merge to maintain clean history

### Code Review Checklist

When reviewing code:

- [ ] Tests pass locally
- [ ] Code follows style guide
- [ ] Documentation is clear
- [ ] No performance regressions
- [ ] Error handling is robust
- [ ] No unnecessary dependencies
- [ ] No unsafe code without justification

## Getting Help

- **Documentation**: Start with `docs/` directory
- **Code Comments**: Check existing similar code
- **GitHub Issues**: Search for similar problems
- **Slack**: Ask in #development channel
- **PR Comments**: Ask during code review

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [FraiseQL Architecture](../../architecture/)
- [Linting Guide](./LINTING.md)

## Quick Reference

### Most Common Commands

```bash
<!-- Code example in BASH -->
cargo fmt              # Format code
cargo clippy --all     # Lint code
cargo test --lib       # Run unit tests
cargo build --release  # Build optimized binary
cargo watch -x test    # Auto-test on changes
```text
<!-- Code example in TEXT -->

### Fast Feedback Loop

```bash
<!-- Code example in BASH -->
# Terminal 1: Auto-check on changes
cargo watch -x check

# Terminal 2: Run tests on change
cargo watch -x "test --lib"

# Terminal 3: Edit code
vim src/file.rs
```text
<!-- Code example in TEXT -->

### Useful Cargo Flags

```bash
<!-- Code example in BASH -->
-p CRATE        # Run for specific crate
--release       # Optimized build (slower to compile, faster to run)
--lib           # Only lib target (skip binaries)
-j 4            # Use 4 parallel jobs (useful on slow machines)
--verbose       # Detailed output
```text
<!-- Code example in TEXT -->

Happy coding! 🚀

---

## Troubleshooting

### "Build fails with 'error: linker `cc` not found'"

**Cause:** C++ build tools not installed on system.

**Diagnosis:**

1. Check if `cc` available: `which cc` or `which gcc`
2. Check Rust setup: `rustup show`

**Solutions:**

- Install build tools: `sudo apt-get install build-essential` (Linux)
- Install Xcode Command Line Tools: `xcode-select --install` (macOS)
- Use `rustup`: `rustup install stable`

### "Cargo build fails with 'cannot find package' dependency"

**Cause:** Dependency not downloading or network issue.

**Diagnosis:**

1. Check internet connectivity
2. Try clearing cache: `cargo clean`
3. Check dependency in Cargo.toml spelling

**Solutions:**

- Verify Cargo.toml has correct dependency name/version
- Update index: `cargo update`
- Check for network issues: Try pinging crates.io
- Check if crate is yanked/removed: Look on crates.io

### "Compilation is very slow (>10 minutes)"

**Cause:** Large project or unoptimized linker.

**Diagnosis:**

1. Profile build: `cargo build --release --timings`
2. Check for heavy dependencies in output
3. Measure link time vs compile time

**Solutions:**

- Use `mold` linker: Uncomment in `.cargo/config.toml` (Linux only, 3-5x faster)
- Use incremental compilation: `cargo build -j 4`
- In CI, use `cargo check` first (faster than full build)
- Split into smaller crates to compile in parallel
- Use sccache for distributed caching in CI

### "Tests fail with 'database connection refused'"

**Cause:** Test database not running or not accessible.

**Diagnosis:**

1. Check PostgreSQL running: `docker ps | grep postgres`
2. Verify connection string: `echo $DATABASE_TEST_URL`
3. Test manually: `psql $DATABASE_TEST_URL -c 'SELECT 1;'`

**Solutions:**

- Start test database: `docker-compose -f tests/docker-compose.yml up -d`
- Wait for startup: Database may take 10-20 seconds
- Create test database if missing: `createdb test_db`
- Check DATABASE_URL environment variable is set

### "IDE doesn't show type hints or autocomplete"

**Cause:** Rust analyzer not working or not installed.

**Diagnosis:**

1. Check if rust-analyzer installed: `rustup component list | grep rust-analyzer`
2. Restart IDE/editor
3. Check if project is recognized: `cargo metadata`

**Solutions:**

- Install rust-analyzer: `rustup component add rust-analyzer`
- Reload IDE window
- Check .vscode/settings.json has rust-analyzer path
- Update VSCode to latest version
- Check project root has Cargo.toml

### "Cargo clippy shows warnings I didn't write"

**Cause:** Clippy found issues in existing code or dependencies.

**Diagnosis:**

1. Identify source of warning: Look at file path in error
2. Check if in test code or main code
3. Filter by crate: `cargo clippy -p specific_crate`

**Solutions:**

- Fix warnings if in your code: `cargo clippy --fix --allow-dirty`
- For dependency warnings: Ignore (not your code)
- Add `#[allow(clippy::lint_name)]` if intentional
- Consider upgrading dependency if it has warning

### "Different Rust version required (error: toolchain mismatch)"

**Cause:** Project requires specific Rust version.

**Diagnosis:**

1. Check rust-toolchain.toml for version requirement
2. Check current version: `rustc --version`

**Solutions:**

- Install correct version: `rustup install 1.XX.X`
- Update stable: `rustup update stable`
- Use specific version: `rustup override set 1.XX.X`
- Let rustup handle it: It reads rust-toolchain.toml automatically

### "Git pre-commit hook fails on your changes"

**Cause:** Code quality check failed before commit.

**Diagnosis:**

1. Rerun hook manually to see error
2. Run same check: `cargo clippy --all-targets`
3. Check what hook does: Look at `.git/hooks/pre-commit`

**Solutions:**

- Fix linting issues: `cargo clippy --fix`
- Run formatter: `cargo fmt`
- Skip hook temporarily: `git commit --no-verify` (not recommended)
- Update hook if it's wrong: Edit `.pre-commit-config.yaml`

---

## See Also

- **[Testing Strategy](../testing-strategy.md)** - Unit, integration, and E2E testing approach
- **[Linting & Code Quality](./LINTING.md)** - Code standards and Clippy configuration
- **[Benchmarking Guide](./benchmarking.md)** - Performance benchmarking with Criterion
- **[Profiling Guide](./PROFILING_GUIDE.md)** - Performance profiling and optimization
- **[E2E Testing Guide](./e2e-testing.md)** - End-to-end testing infrastructure
- **[Contributing Guide](../../../CONTRIBUTING.md)** - Contribution workflow and standards
