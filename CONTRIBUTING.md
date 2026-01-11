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

---

## Testing

### Test Levels

1. **Unit Tests**: Test individual functions/modules
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_addition() {
           assert_eq!(add(2, 2), 4);
       }
   }
   ```

2. **Integration Tests**: Test module interactions
   ```rust
   // tests/integration/test_schema.rs
   #[test]
   fn test_schema_loading() {
       let schema = CompiledSchema::load("test.json").unwrap();
       assert!(schema.is_valid());
   }
   ```

3. **End-to-End Tests**: Test complete flows
   ```rust
   // tests/e2e/test_query_execution.rs
   #[tokio::test]
   async fn test_query_execution() {
       let executor = setup_executor().await;
       let result = executor.execute("query { users { id } }").await.unwrap();
       assert!(!result.has_errors());
   }
   ```

### Test Database

Integration tests require PostgreSQL:

```bash
# Create test database
make db-setup

# Run integration tests
make test-integration

# Clean up
make db-teardown
```

### Coverage

We aim for **85%+ test coverage**:

```bash
# Generate coverage report
make coverage

# View report at target/llvm-cov/html/index.html
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

- **Compiler**: Schema → SQL (compile-time)
- **Runtime**: Query → Execution (runtime)
- **Database**: Data storage and retrieval

### 2. Code Reuse from v1

See `IMPLEMENTATION_ROADMAP.md` for guidance on reusing v1 code:
- **100% Reuse**: Schema, Error, Config, APQ
- **90% Reuse**: Database, Security, Cache
- **Refactor**: Query utilities, GraphQL parsing
- **Rewrite**: Compiler, Runtime

### 3. World-Class Engineering

- **No `unsafe` code** (except in rare, documented cases)
- **Comprehensive error handling** with `Result`
- **Extensive documentation** for all public APIs
- **Thorough testing** (85%+ coverage)
- **Performance-conscious** design

---

## Getting Help

- **Questions**: Open a GitHub Discussion
- **Bugs**: File a GitHub Issue
- **Security**: Email security@fraiseql.dev

---

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (MIT OR Apache-2.0).

---

**Thank you for contributing to FraiseQL v2!** 🚀
