# FraiseQL Linting & Code Quality Guide

## Prerequisites

**Required Knowledge:**
- Rust language syntax and idioms
- Clippy linter concepts and warnings
- Cargo build system
- Code quality principles and best practices
- Rust formatting conventions (rustfmt)
- Documentation comment syntax in Rust

**Required Software:**
- Rust 1.75+ with full toolchain (rustup)
- Clippy (usually included with Rust)
- rustfmt (usually included with Rust)
- Cargo (usually included with Rust)
- Git (for pre-commit hooks setup)
- A text editor supporting Rust diagnostics

**Required Infrastructure:**
- FraiseQL source repository (cloned locally)
- ~2GB free disk space for build artifacts
- Internet connection for downloading dependencies
- Build environment (Linux, macOS, or Windows)

**Optional but Recommended:**
- IDE/Editor integration (rust-analyzer, IntelliJ)
- Pre-commit framework for Git hooks
- CI/CD system (GitHub Actions, GitLab CI)
- Code review tools

**Time Estimate:** 10-20 minutes for first lint run, 2-3 hours for fixing existing issues

## Overview

FraiseQL maintains strict code quality standards through Clippy linting, formatting checks, and comprehensive testing. This document outlines the linting rules, best practices, and workflow for ensuring code quality across the project.

## Clippy Configuration

### Enabled Lint Groups

FraiseQL enables the following Clippy lint groups:

```rust
#![warn(clippy::all)]       // All clippy lints
#![warn(clippy::pedantic)]  // Pedantic lints for code quality
```

### Core Rules

- **`unsafe_code = forbid`**: No unsafe code allowed anywhere in the codebase
- **`missing_docs = warn`**: All public items must have documentation
- **`missing_errors_doc = allow`**: Error cases documented in text, not required per-variant

## Allowed Lints (With Justification)

These pedantic lints are allowed with explicit justification:

| Lint | Reason | Example |
|------|--------|---------|
| `doc_markdown` | Would require 150+ doc changes for backticks | Too noisy for current codebase |
| `uninlined_format_args` | Style preference, not a bug | `format!("{}", x)` is fine |
| `struct_excessive_bools` | AutoParams and ServerConfig use bool flags | Cleaner than enums for independent flags |
| `cast_possible_truncation` | Many intentional u64→u32 casts | Used for intentional conversions |
| `cast_precision_loss` | Intentional f64 conversions | Used for metric aggregation |
| `module_name_repetitions` | Common in Rust APIs | `crate::foo::FooBuilder` is idiomatic |
| `must_use_candidate` | Too noisy for builder methods | Added selectively with `#[must_use]` |
| `missing_errors_doc` | Extensive doc additions needed | Use narrative documentation instead |
| `too_many_arguments` | Some complex functions need many args | Refactor only if logic warrants |
| `redundant_closure_for_method_calls` | Sometimes clearer with closures | Allowed for readability |
| `match_same_arms` | Sometimes clearer when explicit | Allowed for pattern clarity |

## Linting Workflow

### Pre-commit Checks

Before committing, run:

```bash
# Check formatting
cargo fmt --check

# Run strict clippy
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --lib

# Optional: Run benchmark for performance regression
cargo bench --no-run
```

### CI/CD Pipeline

The GitHub Actions CI automatically runs:

1. **Format check**: `cargo fmt --check`
2. **Clippy strict**: `cargo clippy --all-targets --all-features -- -D warnings`
3. **Tests**: `cargo test --lib`
4. **Doctests**: `cargo test --doc`

All checks must pass before PR merge.

### Common Linting Errors & Fixes

#### Missing Documentation

```rust
// ❌ Missing docs
pub fn execute_query(query: &str) -> Result<String> { }

// ✅ Documented
/// Execute a GraphQL query against the compiled schema.
///
/// # Arguments
/// * `query` - The GraphQL query string
///
/// # Returns
/// Query execution result as JSON string
///
/// # Errors
/// Returns error if query is invalid or execution fails
pub fn execute_query(query: &str) -> Result<String> { }
```

#### Unnecessary Box/Arc

```rust
// ❌ Clippy warns
let x = Box::new(value);

// ✅ Let compiler infer when possible
let x = value;
```

#### Pedantic Improvements

```rust
// ❌ Redundant else
if condition {
    return Ok(());
} else {
    return Err(err);
}

// ✅ Clearer
if condition {
    return Ok(());
}
return Err(err);
```

## Allowed Exceptions

Exceptions to strict linting are rare. If an exception is needed:

1. Add `#![allow(clippy::lint_name)]` at the top of the crate with justification
2. Document the reason in a comment with line-specific `#[allow(...)]`
3. Re-evaluate in future refactorings

Example:

```rust
#![allow(clippy::too_many_arguments)]
// QueryExecutor needs many args for type information, database connection, etc.
// Consider refactoring into a builder if logic becomes too complex.
```

## Best Practices

### 1. Use Explicit Types When Unclear

```rust
// ❌ Type not obvious
let results = query.execute().collect();

// ✅ Explicit
let results: Vec<Row> = query.execute().collect();
```

### 2. Document Complex Logic

```rust
// ❌ No explanation
if x.len() > 1024 && y.is_some() {
    process(z);
}

// ✅ Explained
// Cache invalidation when schema size exceeds threshold
if x.len() > 1024 && y.is_some() {
    process(z);
}
```

### 3. Use Builder Pattern for Complex Types

```rust
// ✅ Readable API
let config = ServerConfig::builder()
    .bind_addr("127.0.0.1:8000".parse()?)
    .metrics_enabled(true)
    .build()?;
```

### 4. Prefer Iterator Adapters

```rust
// ❌ Explicit loop
let mut results = Vec::new();
for item in items {
    if item.valid() {
        results.push(item.transform());
    }
}

// ✅ Iterator chain
let results: Vec<_> = items
    .iter()
    .filter(|item| item.valid())
    .map(|item| item.transform())
    .collect();
```

## Formatting Standards

### Code Formatting

Use `rustfmt` with project defaults:

```bash
cargo fmt
```

### Commit Message Format

```
<type>(<scope>): <description>

<body>

<footer>
```

**Types**: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

**Scopes**: `core`, `server`, `cli`, `wire`, `cache`, `compiler`, `runtime`

## Performance Linting

Watch for these performance-related lints:

- `clone_on_copy` - Don't clone Copy types
- `unnecessary_allocation` - Avoid unnecessary Vec/String allocations
- `map_entry` - Use entry API for HashMap when appropriate
- `large_stack_arrays` - Large arrays on stack instead of heap

## Documentation Standards

All public items require documentation:

```rust
/// Brief one-line summary.
///
/// Longer description explaining the purpose, behavior, and any important notes.
/// Can span multiple paragraphs.
///
/// # Arguments
/// * `param1` - Description
/// * `param2` - Description
///
/// # Returns
/// Description of return value
///
/// # Errors
/// * `Error::Variant` - When this happens
///
/// # Example
/// ```
/// let result = function(arg1, arg2)?;
/// assert_eq!(result, expected);
/// ```
///
/// # Performance
/// O(n log n) complexity due to sorting
///
/// # Panics
/// Never panics - returns error instead
pub fn function(param1: Type1, param2: Type2) -> Result<Type3> {
    // ...
}
```

## Testing & Linting Integration

Clippy warnings often indicate design issues. Before ignoring:

1. **Understand the warning** - Read the clippy explanation
2. **Consider refactoring** - Is there a better design?
3. **Add tests** - If design must stay, add comprehensive tests
4. **Document** - Explain why the unusual pattern is needed

Example:

```rust
#[test]
fn test_excessive_bools_intentional() {
    // The ServerConfig struct uses many bools for feature flags.
    // This is intentional - each flag is independent and orthogonal.
    let config = ServerConfig {
        cors_enabled: true,
        compression_enabled: false,
        tracing_enabled: true,
        // ... more flags
    };
    assert!(config.cors_enabled);
}
```

## Continuous Improvement

Linting rules are evaluated quarterly:

1. Review new Clippy lints (new Rust versions)
2. Assess if allowed lints can now be enforced
3. Update documentation and configuration
4. Plan refactoring if needed

## Troubleshooting

### Clippy False Positives

If you believe a Clippy warning is a false positive:

1. Open an issue with the warning details
2. Include minimal reproduction
3. Explain why it's not applicable
4. Reference the Clippy issue number

### Formatting Conflicts

If `rustfmt` and `clippy` conflict:

1. Run `cargo fmt` first
2. Run `cargo clippy --fix` to auto-fix lint issues
3. Review changes carefully
4. Run tests to ensure behavior unchanged

### CI Failures

If CI fails for linting:

1. Run `cargo clippy --all-targets --all-features -- -D warnings` locally
2. Fix warnings or add documented exceptions
3. Run full test suite before pushing
4. Commit with descriptive message

## Resources

- [Clippy Documentation](https://doc.rust-lang.org/clippy/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [rustfmt Configuration](https://rust-lang.github.io/rustfmt/)

## Questions?

For questions about linting, code style, or exceptions:

1. Check this guide first
2. Review similar patterns in codebase
3. Open a discussion in PR comments
4. Ask in project Slack channel
