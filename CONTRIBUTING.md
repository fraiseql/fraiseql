# Contributing to fraiseql-wire

Thank you for considering contributing to fraiseql-wire! This document provides guidelines and instructions for contributing.

## Code of Conduct

This project is committed to providing a welcoming and inspiring community for all. Please read and respect our community standards.

## Development Setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Postgres 17 (for integration tests)
- Docker (optional, for containerized Postgres)

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraiseql-wire.git
cd fraiseql-wire

# Install Rust (if not already installed)
rustup install 1.75

# Verify setup
cargo --version
rustc --version
```

### Docker Setup (Recommended)

For a quick local database without installing Postgres directly:

```bash
# Build Docker image
make docker-build

# Start Postgres container
make docker-up

# Stop container
make docker-down
```

## Running Tests

### Unit Tests

```bash
# Run all unit tests
cargo test --lib

# Run specific module tests
cargo test stream::filter
cargo test client::connection_string

# Run with output
cargo test -- --nocapture
```

### Integration Tests

```bash
# Requires Postgres running (use docker-up or local instance)

# Run all integration tests
cargo test --test integration -- --ignored --nocapture

# Run specific test
cargo test test_streaming_query -- --ignored --nocapture
```

### Full Test Suite

```bash
# Build and run all targets
cargo build --all-targets
cargo test
```

## Code Quality

### Formatting

```bash
# Check formatting
cargo fmt -- --check

# Auto-format code
cargo fmt
```

### Linting

```bash
# Run clippy with all warnings enabled
cargo clippy -- -D warnings

# Stricter mode (includes all clippy warnings)
cargo clippy -- -D warnings -D clippy::all
```

### Type Checking

```bash
# Check without building
cargo check

# Check all examples
cargo check --examples
```

## Documentation

### Building Documentation

```bash
# Build and open documentation
cargo doc --no-deps --open

# Check for doc warnings
cargo rustdoc -- -D missing-docs

# Check specific module
cargo doc --lib fraiseql_wire::client --no-deps --open
```

### Writing Documentation

- All public APIs must have documentation comments
- Use markdown formatting in doc comments
- Include examples for complex APIs
- Document panics, errors, and edge cases
- Keep examples runnable (use no_run for async)

Example:

```rust
/// Execute query and return JSON stream
///
/// # Examples
///
/// ```no_run
/// # async fn example(client: fraiseql_wire::FraiseClient) -> fraiseql_wire::Result<()> {
/// let stream = client.query("user").execute().await?;
/// # Ok(())
/// # }
/// ```
pub async fn execute(self) -> Result<impl Stream<Item = Result<Value>>> {
    // ...
}
```

## Making Changes

### Process

1. **Fork** the repository
2. **Create** a feature branch: `git checkout -b feature/my-feature`
3. **Make** your changes with tests
4. **Run** `cargo test` to verify
5. **Commit** with clear messages: `git commit -m "feat: add my feature"`
6. **Push** to your fork: `git push origin feature/my-feature`
7. **Create** a Pull Request with description

### Commit Message Format

Use conventional commits for clear commit history:

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat:` new feature
- `fix:` bug fix
- `refactor:` code refactoring
- `docs:` documentation changes
- `test:` test additions/changes
- `chore:` build, CI, or tooling changes

Example:

```
feat(client): add connection string parsing for Unix sockets

Enables connecting to local Postgres via Unix domain sockets.
Uses whoami for default user when not specified.

Fixes #42
```

### Testing Requirements

All contributions must include tests:

- Unit tests for new functions/modules
- Integration tests for end-to-end behavior
- Edge case tests for error conditions
- No decrease in overall test coverage

```rust
#[test]
fn test_my_feature() {
    let result = my_function("input");
    assert_eq!(result, "expected");
}

#[tokio::test]
async fn test_my_async_feature() {
    let result = my_async_function("input").await;
    assert_eq!(result, "expected");
}
```

## Design Principles

fraiseql-wire follows strict design principles. Ensure your contribution aligns with these:

### ✅ Belongs in fraiseql-wire

- Improvements to JSON streaming performance
- Better error messages and error handling
- Enhanced observability (tracing, metrics)
- Documentation and examples
- Bug fixes
- API improvements maintaining backward compatibility

### ❌ Does NOT Belong in fraiseql-wire

- General SQL support beyond constraints
- Write operations (INSERT/UPDATE/DELETE)
- Transaction support
- Connection pooling (separate crate)
- Support for non-JSON types
- Features requiring buffering full result sets
- Authentication methods beyond cleartext
- TLS/encryption (use external service)

## Hard Constraints

These constraints are non-negotiable and relied upon throughout:

- Exactly **one column** in result set
- Column must be named `data`
- Column type must be `json` or `jsonb`
- One active query per connection
- Results streamed **in-order**
- No full result set buffering
- Drop stream to cancel query
- No client-side sorting/aggregation
- Protocol encoding/decoding is pure (no I/O)
- Connection state machine is explicit

## Architecture Overview

### Module Structure

```
src/
├── client/          → Public API (FraiseClient, QueryBuilder)
├── stream/          → Streaming abstractions
├── protocol/        → Postgres wire protocol
├── connection/      → Connection management & state machine
├── json/            → JSON validation
├── util/            → Utilities
├── error.rs         → Error types
└── lib.rs           → Library root
```

### Data Flow

```
User Code
  ↓
FraiseClient::connect() → Connection established
  ↓
client.query() → QueryBuilder
  ↓
.where_sql() / .where_rust() / .order_by()
  ↓
.execute() → Postgres query sent
  ↓
FilteredStream (if Rust predicate)
  ↓
Stream<Item = Result<Value>>
  ↓
Consumed by user code
```

## Performance Considerations

- No blocking I/O in async functions
- Minimize allocations in hot paths
- Use references over clones when possible
- Benchmark changes that affect streaming
- Memory should scale with chunk_size, not result size

## CI/CD Workflows

This project uses GitHub Actions for automated testing and releases.

### Continuous Integration

Every push to `main` and pull request triggers:

- **Build & Test**: Compiles with Rust stable, runs unit tests
- **Code Coverage**: Generates coverage report (target: >85%)
- **MSRV**: Tests with Rust 1.70 for backward compatibility
- **Integration Tests**: Runs against Postgres 15 service
- **Documentation**: Checks for doc warnings
- **Security Audit**: Runs `cargo audit` to detect vulnerabilities

See [CI_CD_GUIDE.md](CI_CD_GUIDE.md) for detailed workflow documentation.

### Release Process

The maintainers follow this automated process for releases:

1. Update version in Cargo.toml and CHANGELOG.md
2. Run full test suite locally
3. Execute release script: `./scripts/publish.sh 0.1.0`
4. Script automatically:
   - Validates semver format
   - Verifies clean git state
   - Builds and tests release
   - Creates git tag and pushes
   - Publishes to crates.io
5. GitHub Actions creates release on crates.io

For detailed release instructions, see [CI_CD_GUIDE.md](CI_CD_GUIDE.md#making-a-release).

Contributors don't need to handle releases.

## Getting Help

- **Questions?** Open a [GitHub Discussion](https://github.com/fraiseql/fraiseql-wire/discussions)
- **Found a bug?** Open a [GitHub Issue](https://github.com/fraiseql/fraiseql-wire/issues)
- **Want to discuss design?** Open an issue before starting major work

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Postgres Protocol Docs](https://www.postgresql.org/docs/17/protocol.html)
- [Futures Crate Docs](https://docs.rs/futures/)
- [FraiseQL Architecture Docs](.claude/CLAUDE.md)

## License

By contributing to fraiseql-wire, you agree that your contributions will be licensed under the same license terms (MIT OR Apache-2.0).

Thank you for contributing to fraiseql-wire! 🎉
