# FraiseQL v2 Quick Reference

## Essential Commands

```bash
# Development cycle
cargo watch -x check              # Auto-check on file changes
cargo nextest run                 # Run all tests (fast)
cargo clippy --all-targets        # Lint with strict rules

# Build variants
cargo check                       # Fast syntax check
cargo build                       # Debug build
cargo build --release             # Optimized build

# Testing
cargo nextest run test_name       # Run specific test
cargo test -- --nocapture         # Show test output
RUST_LOG=debug cargo test         # Run with logging

# Code quality
cargo fmt                         # Format code
cargo clippy --fix                # Auto-fix lints
cargo doc --open                  # Generate and view docs

# Aliases (from .cargo/config.toml)
cargo c                           # check
cargo t                           # test
cargo br                          # build --release
cargo cov                         # llvm-cov coverage
```

## Project Structure

```
fraiseql/
├── .claude/
│   ├── CLAUDE.md                    # Main dev guide
│   ├── IMPLEMENTATION_ROADMAP.md    # 11-phase plan
│   └── QUICK_REFERENCE.md           # This file
├── crates/
│   └── fraiseql-core/              # Phase 1-5 (core engine)
│       ├── src/
│       │   ├── schema/             # ✅ Compiled schema types
│       │   ├── error.rs            # ✅ Error handling
│       │   ├── config/             # ✅ Configuration
│       │   ├── apq/                # ✅ Auto Persisted Queries
│       │   ├── db/                 # ⏳ Database (Phase 2)
│       │   ├── cache/              # ⏳ Caching (Phase 2)
│       │   ├── security/           # ⏳ Auth (Phase 3)
│       │   ├── compiler/           # ⏳ Compiler (Phase 4)
│       │   └── runtime/            # ⏳ Runtime (Phase 5)
│       └── Cargo.toml
├── docs/                            # Architecture documentation
├── tools/                           # Development tools
└── Cargo.toml                       # Workspace config
```

## Current Status

**Phase 1: Foundation** ✅ Complete
- Copied 4,516 lines from v1
- schema/, error.rs, config/, apq/ modules
- All code compiles, clippy clean

**Phase 2: Database & Cache** ⏳ Next
- Adapt db/ module (90-95% reusable)
- Adapt cache/ module (90-95% reusable)
- Database abstraction traits
- Integration tests

## Architecture at a Glance

```
┌──────────────┐
│ Python/TS    │ Decorators → schema.json
└──────┬───────┘
       │
       ↓
┌──────────────┐
│ fraiseql-cli │ compile → schema.compiled.json
└──────┬───────┘
       │
       ↓
┌──────────────┐
│ fraiseql-    │ Load compiled schema
│ server       │ Execute GraphQL queries
└──────────────┘
```

**Key Principle**: Authoring (Python/TS) and Runtime (Rust) are completely separated.

## Database Support

| Database | Status | Driver |
|----------|--------|--------|
| PostgreSQL | Primary | tokio-postgres |
| MySQL | Secondary | (TBD Phase 2) |
| SQLite | Testing | (TBD Phase 2) |
| SQL Server | Optional | (TBD Phase 2) |
| Oracle | ❌ Not supported | No Rust driver |

## Error Handling Pattern

```rust
use crate::error::{FraiseQLError, Result};

fn my_function() -> Result<String> {
    // Parse error
    let schema = CompiledSchema::from_str(json)
        .map_err(|e| FraiseQLError::parse(e))?;

    // Database error
    pool.get().await
        .map_err(|e| FraiseQLError::database(e))?;

    Ok(result)
}
```

## Testing Pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_function() {
        let result = my_function();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_async_function() {
        let pool = setup_test_pool().await;
        let result = query_database(&pool).await;
        assert_eq!(result.len(), 5);
    }
}
```

## Git Workflow

```bash
# Start new phase
git checkout -b feature/phase-N-name

# During development
cargo watch -x check              # Keep this running
# ... make changes ...
cargo test                        # Before commit
cargo clippy --all-targets        # Check lints

# Commit
git add -A
git commit -m "feat(scope): Phase N - description

## Changes
- Change 1
- Change 2

## Verification
✅ Tests pass
✅ Clippy clean
"

# Push
git push -u origin feature/phase-N-name
```

## Performance Tips

```bash
# Install fast linker (Linux)
sudo pacman -S mold
# Then uncomment in .cargo/config.toml

# Use faster test runner
cargo install cargo-nextest
cargo nextest run                 # 2-3x faster

# Parallel tests
cargo nextest run --test-threads 8

# Coverage report
cargo install cargo-llvm-cov
cargo llvm-cov --html
open target/llvm-cov/html/index.html
```

## Common Issues

### Compilation slow?
- Install mold linker (see Performance Tips)
- Use `cargo check` instead of `cargo build` during development

### Tests failing?
- Check PostgreSQL is running: `systemctl status postgresql`
- Clean build: `cargo clean && cargo test`
- Single test with output: `cargo test test_name -- --nocapture`

### Clippy too strict?
- We use `deny` level intentionally for code quality
- Don't use `#[allow]` without good reason
- Fix the issue or discuss if it's a false positive

## Need Help?

1. Check [`.claude/CLAUDE.md`](.claude/CLAUDE.md) for detailed guidance
2. Review [`.claude/IMPLEMENTATION_ROADMAP.md`](.claude/IMPLEMENTATION_ROADMAP.md) for phase details
3. Look at existing code in `crates/fraiseql-core/src/` for patterns
4. Read architecture docs in `docs/`

## Links

- **GitHub**: https://github.com/fraiseql/fraiseql
- **Docs**: https://fraiseql.com (when published)
- **Rust Book**: https://doc.rust-lang.org/book/
- **Clippy Lints**: https://rust-lang.github.io/rust-clippy/
