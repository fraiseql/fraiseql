# FraiseQL Endpoint Runtime - Quick Reference

## Document Map

| Document | Purpose |
|----------|---------|
| **00-OVERVIEW.md** | Original 10-phase vision |
| **01-PHASE-1-FOUNDATION.md** | Configuration, lifecycle, health checks |
| **02-PHASE-2-CORE.md** | Rate limiting, CORS, metrics |
| **03-PHASE-3-WEBHOOKS.md** | Webhook signatures, idempotency |
| **04-PHASE-4-FILES.md** | File upload, storage, image processing |
| **04B-PHASE-4B-RESTRUCTURING.md** | ⭐ **START HERE** - Consolidate into fraiseql-server |
| **05-PHASE-5-AUTH.md** | OAuth 2.0, JWT, sessions (extended fraiseql-server) |
| **06-10-PHASES-6-10-OVERVIEW.md** | Roadmap: observers, notifications, search, cache, jobs, interceptors, polish |
| **IMPLEMENTATION-SUMMARY.md** | Strategic decisions, architecture, development workflow |
| **QUICK-REFERENCE.md** | This file |

---

## Architecture Decision: Unified fraiseql-server

### Why Consolidate?

**Problem**: Phases 1-4 created 4 separate crates
```
fraiseql-server
fraiseql-runtime
fraiseql-webhooks
fraiseql-files
```

**Solution**: Merge all into one crate
```
fraiseql-server (unified)
  ├── config/       (from fraiseql-runtime)
  ├── lifecycle/    (from fraiseql-runtime)
  ├── middleware/   (from fraiseql-runtime)
  ├── webhooks/     (from fraiseql-webhooks)
  ├── files/        (from fraiseql-files)
  ├── auth/         (new in Phase 5)
  └── ...
```

### Benefits

| Aspect | Before (4 crates) | After (1 crate) |
|--------|------------------|-----------------|
| Configuration | 4 config systems | 1 unified config |
| Errors | 4 error enums | 1 RuntimeError |
| Middleware | Duplicated | Shared |
| Testing | Complex fixtures | Simple |
| New features | Add crate + imports | Add module |

---

## Phase Status

### ✅ Complete (Phases 1-4)
- **1**: Foundation (560 LOC) - Config, lifecycle, health
- **2**: Core runtime (2,091 LOC) - Rate limit, CORS, metrics
- **3**: Webhooks (2,800 LOC) - Signatures, idempotency
- **4**: Files (2,400 LOC) - Storage, validation, processing

**Total**: 7,851 LOC implemented, all tests passing

### 🔄 Next (Phase 4B - RESTRUCTURING)
- Consolidate 3 crates into fraiseql-server
- Update all imports
- Verify tests still pass
- **Effort**: 3-6 hours
- **Risk**: Low (code-only change, tests provide safety net)

### 📋 Planned (Phase 5+)
- **5**: Auth (OAuth 2.0, JWT)
- **6**: Observers (event reactions)
- **7**: Notifications (email, SMS, push)
- **8A**: Full-text search
- **8B**: Caching
- **8C**: Job queues
- **9**: Interceptors (WASM/Lua)
- **10**: Polish (optimization, observability)

---

## How to Read the Documentation

### For Architecture Understanding
1. Read **IMPLEMENTATION-SUMMARY.md** (strategic overview)
2. Read **04B-PHASE-4B-RESTRUCTURING.md** (consolidation plan)
3. Skim **06-10-PHASES-6-10-OVERVIEW.md** (feature roadmap)

### For Phase 5 Implementation
1. Read **05-PHASE-5-AUTH.md** (complete auth design)
2. Reference config examples
3. Use provided code blocks as templates

### For Phase 6+ Implementation
1. Reference **06-10-PHASES-6-10-OVERVIEW.md**
2. Follow the integration pattern (config → module → routes → tests)
3. Reuse traits and mocks from existing phases

---

## Key Code Patterns

### Configuration Integration
```rust
// Add to RuntimeConfig
pub struct RuntimeConfig {
    pub your_feature: Option<YourFeatureConfig>,
}

// In config.rs
#[derive(Debug, Deserialize)]
pub struct YourFeatureConfig {
    // Your settings
}
```

### Module Structure
```rust
// src/your_feature/mod.rs
pub mod traits;      // Public interfaces
pub mod handler;     // Business logic
pub mod routes;      // HTTP handlers
pub mod testing;     // Mock implementations

pub use handler::YourFeatureHandler;
```

### AppState Integration
```rust
// In state.rs
pub struct AppState {
    // ... existing fields
    pub your_feature: Option<Arc<YourFeatureHandler>>,
}
```

### Route Registration
```rust
// In routes/mod.rs
.nest("/your-feature", your_feature::routes(state.clone()))
```

### Testing
```rust
#[cfg(test)]
mod tests {
    use crate::your_feature::testing::Mock*;

    #[tokio::test]
    async fn test_feature() {
        let mock = Mock::new();
        // Test logic
    }
}
```

---

## Command Reference

### Development
```bash
# Watch for changes
cargo watch -x run

# Run tests
cargo test
cargo test -p fraiseql-server

# Run specific test
cargo test auth::jwt_test

# With logging
RUST_LOG=debug cargo test -- --nocapture

# With features
cargo test --features testing
```

### Build
```bash
# Check compilation
cargo check

# Lint
cargo clippy --all-targets

# Build release
cargo build --release

# Generate docs
cargo doc --open
```

### Phase 4B (Restructuring)
```bash
# After consolidating crates:
cargo check                          # Should pass
cargo test                           # All tests should pass
cargo clippy --all-targets           # No warnings
```

---

## Configuration Examples

### Minimal (Just Server)
```toml
[server]
host = "0.0.0.0"
port = 4000

[database]
url_env = "DATABASE_URL"
```

### With Phase 3-4 (Webhooks + Files)
```toml
[server]
host = "0.0.0.0"
port = 4000

[database]
url_env = "DATABASE_URL"

[webhooks]
tolerance_secs = 300
max_payload_size = "10MB"

[files]
storage = "s3"
max_file_size = "100MB"
```

### With Phase 5 (Auth)
```toml
[auth]
session_type = "jwt"
jwt_secret_env = "JWT_SECRET"

[auth.providers.google]
client_id_env = "GOOGLE_CLIENT_ID"
client_secret_env = "GOOGLE_CLIENT_SECRET"

[auth.providers.github]
client_id_env = "GITHUB_CLIENT_ID"
client_secret_env = "GITHUB_CLIENT_SECRET"
```

### Full Feature (Phase 5+)
```toml
[auth]
# ... auth config

[observers]
# ... observer config (Phase 6)

[notifications]
# ... notification config (Phase 7)

[search]
# ... search config (Phase 8A)

[cache]
# ... cache config (Phase 8B)

[jobs]
# ... job config (Phase 8C)

[interceptors]
# ... interceptor config (Phase 9)
```

---

## Testing Checklist

### Unit Tests
- [ ] Trait implementations work correctly
- [ ] Error handling is comprehensive
- [ ] Mock implementations match trait contracts

### Integration Tests
- [ ] Full request/response flows work
- [ ] Database operations are correct
- [ ] Error responses are properly formatted

### Acceptance Tests
- [ ] All acceptance criteria met
- [ ] No regressions in existing tests
- [ ] Documentation examples work

---

## Decision Tree: Should We Consolidate?

```
Is architecture causing problems?
├─ No → Maybe later, but yes now
└─ Yes (it is) → Consolidate immediately

Will consolidation break things?
├─ No → Safe to proceed
└─ Yes → Tests will catch it, still safe

Is timing good?
├─ Phase 4B is the perfect time
└─ Do it now before Phase 5

Result: **Consolidate now into fraiseql-server**
```

---

## Rollback Strategy

If Phase 4B consolidation causes issues:

1. **Git has you covered**: All commits are atomic
   ```bash
   git revert <consolidation-commit>
   ```

2. **No data changes**: This is code-only refactoring
   - Databases unaffected
   - No migrations needed
   - Easy to revert

3. **Tests provide safety**: Every test must pass before committing

---

## Success Criteria for Phase 4B

- [ ] `cargo check -p fraiseql-server` passes
- [ ] `cargo test -p fraiseql-server` all pass
- [ ] `cargo clippy --all-targets` no warnings
- [ ] Old crates (fraiseql-runtime, webhooks, files) removed
- [ ] All imports updated throughout
- [ ] Documentation updated
- [ ] No new compilation errors

---

## Timeline Estimates

| Phase | Task | Estimate |
|-------|------|----------|
| 4B | Restructure | 3-6 hours |
| 5 | Auth | 6-8 hours |
| 6 | Observers | 4-6 hours |
| 7 | Notifications | 4-6 hours |
| 8A | Search | 3-4 hours |
| 8B | Cache | 2-3 hours |
| 8C | Jobs | 3-4 hours |
| 9 | Interceptors | 4-6 hours |
| 10 | Polish | 4-6 hours |

**Total**: ~40-50 hours for complete implementation (Phases 4B-10)

---

## Common Questions

**Q: Why consolidate? Separate crates are more modular.**
A: These aren't independent features - they're parts of one server. Shared infrastructure (config, error handling, middleware) benefits from being in one place.

**Q: Will consolidation break my code?**
A: Yes, imports change. But it's a straightforward find+replace (`fraiseql_runtime::` → `fraiseql_server::`).

**Q: Can I use just webhooks without auth?**
A: Yes! All features are optional. Configure what you need, leave the rest out.

**Q: When should I migrate?**
A: After Phase 4B completes. The docs will show new import paths.

**Q: What about existing deployments?**
A: New deployments use consolidated version. Existing deployments can stay on Phase 4 if needed (older version).

---

## Architecture Summary

```
           User Requests
                  ↓
         ┌─────────────────┐
         │  HTTP Server    │
         │   (Axum)        │
         └────────┬────────┘
                  ↓
         ┌─────────────────┐
         │  Route Handlers │
         │  (GraphQL,      │
         │   Webhooks,     │
         │   Files, Auth)  │
         └────────┬────────┘
                  ↓
         ┌─────────────────┐
         │  Middleware     │
         │  (Rate limit,   │
         │   CORS, Auth)   │
         └────────┬────────┘
                  ↓
         ┌─────────────────┐
         │  Feature        │
         │  Modules        │
         │  (Webhooks,     │
         │   Files, Auth)  │
         └────────┬────────┘
                  ↓
         ┌─────────────────┐
         │  Database       │
         │  (PostgreSQL)   │
         └─────────────────┘

All in: fraiseql-server
Unified: config, errors, AppState
Shared: middleware, utilities
```

---

## Next Action

1. **Read**: `04B-PHASE-4B-RESTRUCTURING.md`
2. **Plan**: Consolidation steps
3. **Execute**: 3-6 hour refactoring
4. **Verify**: All tests pass
5. **Commit**: "chore: consolidate phases 1-4 into fraiseql-server"
6. **Next**: Phase 5 auth implementation

---

## Resources

- 📖 Full documentation: `/docs/endpoint-runtime/`
- 💻 Code: `/crates/fraiseql-server/`
- 🧪 Tests: `/crates/fraiseql-server/tests/`
- 📝 Migrations: `/crates/fraiseql-server/migrations/`

---

**Remember**: The goal is a unified, maintainable GraphQL server with progressive feature additions. We're consolidating now to make future features easier to add.

Let's build it! 🚀
