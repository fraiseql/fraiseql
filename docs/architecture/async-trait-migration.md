# async_trait → Native Async Fn in Traits (RPITIT) Migration

## Status

**Blocked** — waiting on [RFC 3425 (Return Type Notation)](https://github.com/rust-lang/rust/issues/109417) stabilization for `+ Send` propagation on dynamic trait objects.

## Current State

- **198** `#[async_trait]` annotations across **100** files
- All uses are intentional and documented with RFC 3425 blocker comments
- No scattered or legacy usage — the codebase is well-organized for batch migration

## Top Migration Candidates

| Priority | Trait | Crate | Async Methods | Impl Count | File |
|----------|-------|-------|:---:|:---:|------|
| Critical | `DatabaseAdapter` | fraiseql-db | 11 | 33 | `crates/fraiseql-db/src/traits.rs` |
| High | `OAuthProvider` | fraiseql-auth | 4 | 9 | `crates/fraiseql-auth/src/provider.rs` |
| Medium | `BaseKmsProvider` | fraiseql-core | 12 | 1 | `crates/fraiseql-core/src/security/kms/base.rs` |
| Medium | `JobQueue` | fraiseql-observers | 7 | 4 | `crates/fraiseql-observers/src/job_queue/traits.rs` |
| Medium | `EventTransport` | fraiseql-observers | 4 | 3 | `crates/fraiseql-observers/src/transport/mod.rs` |
| Low | `ApqStorage` | fraiseql-core | 6 | 2 | `crates/fraiseql-core/src/apq/storage.rs` |
| Low | `AsyncValidator` | fraiseql-core | 1 | 5 | `crates/fraiseql-core/src/validation/async_validators.rs` |

### Why `DatabaseAdapter` is critical

`DatabaseAdapter` has 33 implementations (PostgreSQL, MySQL, SQLite, SQL Server, plus test mocks and adapters). Every `Box<dyn DatabaseAdapter>` usage requires `+ Send` bounds which `async_trait` currently provides via boxing. Migrating this trait first would remove ~30% of all `#[async_trait]` annotations.

## Blocker: RFC 3425

Native `async fn in trait` (stabilized in Rust 1.75) does **not** propagate `+ Send` bounds on `dyn Trait` objects. This means `tokio::spawn` and similar `Send`-requiring contexts cannot use `Box<dyn Trait>` with native async methods.

RFC 3425 introduces Return Type Notation (`fn method(..) -> impl Send`) to solve this. Until it stabilizes, `async_trait`'s automatic boxing + Send bound is required for all traits used as trait objects.

**Estimated availability**: Rust 1.90+ (tracking issue: rust-lang/rust#109417)

## Migration Plan

Once RFC 3425 is stable:

1. **Phase 1**: Migrate leaf traits with few impls (`AsyncValidator`, `ApqStorage`)
2. **Phase 2**: Migrate observer traits (`JobQueue`, `EventTransport`)
3. **Phase 3**: Migrate auth traits (`OAuthProvider`)
4. **Phase 4**: Migrate `DatabaseAdapter` (largest, most impls)

Each phase should be a separate PR with before/after benchmarks to confirm no boxing overhead regression.

## CI Guard

The annotation count should not grow. If a CI check is desired:

```bash
count=$(grep -r '#\[async_trait' crates/ --include='*.rs' -c 2>/dev/null | awk -F: '{sum+=$2} END {print sum}')
if [ "$count" -gt 198 ]; then
  echo "async_trait annotation count grew to $count (limit: 198)"
  exit 1
fi
```
