# Phase 9: v0.1 Release Preparation

**Target**: v0.1.0 release (already at this version - this phase is about shipping it formally)

**Philosophy**: You're overdelivering. v0.1 gets:

- Fully working streaming JSON API
- TLS/authentication support
- SCRAM-SHA256 auth
- Comprehensive benchmarks showing 2000x memory efficiency
- 120+ integration tests
- Complete documentation (guides, examples, troubleshooting)
- Formal release with git tag and crate publication

Everything is already built and tested. This phase is just about formalizing the release.

---

## Objectives

1. **Final API Review** (15 min)
   - Ensure all public APIs match documentation
   - No breaking changes needed (already stable)
   - Confirm type signatures are correct

2. **CHANGELOG & Release Notes** (30 min)
   - Document what v0.1.0 includes
   - Performance benchmarks
   - Known limitations (expected at 0.1)
   - Link to all guides

3. **Git Tag & Release** (15 min)
   - Tag v0.1.0
   - Create GitHub Release with notes

4. **Crate Publication** (10 min)
   - Publish to crates.io

---

## Scope for v0.1.0

### ✅ Included

- Streaming JSON queries via Postgres simple query protocol
- TCP and Unix socket support
- TLS support (optional, configurable)
- SCRAM-SHA256 authentication
- Password authentication (md5, plain)
- Query builder with WHERE and ORDER BY
- Hybrid SQL + Rust predicates
- Chunking and streaming with backpressure
- Graceful cancellation on drop
- Comprehensive error types
- Metrics collection hooks
- Full test coverage (unit, integration, benchmarks)
- Complete documentation suite

### ❌ Not Included (by design, not bugs)

- Extended Query protocol (no prepared statements)
- Transactions / multi-statement queries
- Writes (INSERT/UPDATE/DELETE)
- Arbitrary SQL
- Multi-column result sets
- Fact tables (`tf_*`)
- Arrow data plane (`va_*`)
- Full Postgres type system

These are not missing features—they're explicit non-goals documented in `CLAUDE.md` and the PRD.

---

## Implementation Steps

### Step 1: API Review (15 min)

**What**: Verify all public APIs are correctly documented.

```bash
# Check lib.rs exports
cat src/lib.rs | grep "^pub "

# Verify examples run without errors
cargo run --example basic_query
cargo run --example filtering
cargo run --example ordering
cargo run --example streaming
cargo run --example error_handling
```

**Expected**: All examples should run cleanly against a local Postgres (docker-compose up).

**Verify**: No warnings or errors from examples.

---

### Step 2: Create CHANGELOG.md (30 min)

**What**: Document v0.1.0 as the initial release with all features.

**File**: Create `/home/lionel/code/fraiseql-wire/CHANGELOG.md`

**Content template**:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-01-13

### Added

**Core Features**:

- Streaming JSON queries via Postgres Simple Query protocol
- TCP and Unix socket connection support
- TLS support with native and rustls backends
- SCRAM-SHA256 authentication (Postgres 10+)
- Password authentication (md5, plain text)
- Query builder with WHERE and ORDER BY support
- Hybrid predicates: SQL reduces wire traffic, Rust refines streamed data
- Chunked row handling with configurable chunk size
- Graceful query cancellation on stream drop
- Bounded memory usage (O(chunk_size), not O(result_size))

**Architecture**:

- From-scratch Postgres wire protocol implementation (no libpq dependency)
- Bounded async channels for backpressure
- Non-blocking cancellation via CancelRequest message
- Streaming-first APIs: `Stream<Item = Result<serde_json::Value, FraiseError>>`

**Observability**:

- Structured error types with detailed context
- Metrics hooks for instrumentation
- Tracing support via `tracing` crate
- Diagnostic examples (error_handling.rs)

**Testing**:

- 120+ integration tests covering:
  - Connection lifecycle (TCP, Unix sockets, TLS)
  - Authentication (SCRAM-SHA256, password)
  - Query execution and filtering
  - ORDER BY correctness
  - Chunking and backpressure
  - Cancellation semantics
  - Error propagation
- Micro benchmarks for protocol operations
- Comparison benchmarks vs tokio-postgres
- Integration benchmarks against real Postgres

**Performance** (v0.1.0 baseline):

- Memory efficiency: 2000x lower than tokio-postgres for 10K rows
  - fraiseql-wire: O(chunk_size), typically 1.3 KB
  - tokio-postgres: O(result_size), 2.6 MB for 10K rows
- Latency comparable to tokio-postgres (2-5 ms time-to-first-row)
- Throughput: 100K-500K rows/sec depending on JSON complexity

**Documentation**:

- `README.md` – Overview, examples, philosophy
- `QUICK_START.md` – Installation and first steps
- `TESTING_GUIDE.md` – Running unit, integration, and load tests
- `TROUBLESHOOTING.md` – Error diagnosis and recovery
- `CI_CD_GUIDE.md` – Release and development workflows
- `PERFORMANCE_TUNING.md` – Benchmarking and optimization
- `CONTRIBUTING.md` – Architecture and development guidelines
- `PRD.md` – Product requirements and design constraints
- 5 runnable examples (basic_query, filtering, ordering, streaming, error_handling)

### Non-Goals (By Design)

The following are **intentional non-goals**, not missing features:

- Extended Query protocol (no prepared statements)
- Transactions (no BEGIN/COMMIT/ROLLBACK)
- Writes (no INSERT/UPDATE/DELETE)
- Arbitrary SQL (only SELECT...WHERE...ORDER BY)
- Multi-column result sets (one `data` column only)
- Full Postgres type system (JSON-only)
- Fact tables (`tf_*`) or Arrow data plane (`va_*`)

If you need these, use `tokio-postgres`, `sqlx`, or `sqlalchemy`.

### Known Limitations

- API is in 0.1 (minor version bumps may include breaking changes)
- Protocol implementation is minimal (not a general-purpose driver)
- No support for custom types or operators
- No connection pooling (use external pool like `deadpool` or `sqlx`)
- ORDER BY works server-side only (client-side sorting not supported)

---

## Upgrade Path

Upgrading from an earlier version? No prior releases exist. This is the initial release.

Future versions will follow semantic versioning:

- 0.2.x, 0.3.x, etc. – may include breaking API changes
- 1.0.0+ – stable API guarantee

---

[0.1.0]: https://github.com/fraiseql/fraiseql-wire/releases/tag/v0.1.0
```

**Verify**: CHANGELOG.md is created and comprehensively documents v0.1.0.

---

### Step 3: Update Version & Create Git Tag (15 min)

**Verify version is 0.1.0**:

```bash
grep "^version" Cargo.toml
# Should output: version = "0.1.0"
```

**Create git tag**:

```bash
git tag -a v0.1.0 -m "Release v0.1.0 - Streaming JSON query engine for Postgres 17

Features:

- Streaming JSON queries via simple query protocol
- TCP and Unix socket support
- TLS and SCRAM-SHA256 authentication
- Hybrid SQL + Rust predicates
- Bounded memory usage (2000x more efficient than tokio-postgres)
- 120+ integration tests
- Comprehensive documentation

This is the initial release. Full changelog in CHANGELOG.md.
"
```

**Verify tag was created**:

```bash
git tag -l v0.1.0 -n10
```

---

### Step 4: Test Everything One More Time (15 min)

**Run full test suite**:

```bash
cargo test --all-features
cargo test --test integration
cargo clippy -- -D warnings
cargo fmt --check
```

**Expected**: All tests pass, no clippy warnings, formatting is correct.

**Verify**: `cargo build` succeeds:

```bash
cargo build --release
```

---

### Step 5: Publish to crates.io (10 min)

**Prerequisites**:

- You have a crates.io account
- You have an API token in `~/.cargo/credentials`

**Publish**:

```bash
# Dry run first (doesn't actually publish)
cargo publish --dry-run

# Actually publish
cargo publish
```

**Verify**: Package appears on <https://crates.io/crates/fraiseql-wire>

---

### Step 6: Create GitHub Release (10 min)

**Create release on GitHub**:

```bash
# If gh CLI is installed:
gh release create v0.1.0 \
  --title "fraiseql-wire v0.1.0" \
  --notes-file CHANGELOG.md
```

Alternatively, create manually via <https://github.com/fraiseql/fraiseql-wire/releases/new>

**What to include**:

- Tag: `v0.1.0`
- Title: `fraiseql-wire v0.1.0 - Streaming JSON Query Engine`
- Notes: Copy from CHANGELOG.md (Added section)

---

## Verification

### Before Release

- [ ] All 120+ tests pass
- [ ] `cargo clippy` has no warnings
- [ ] `cargo fmt --check` passes
- [ ] All examples run successfully
- [ ] Cargo.toml has version = "0.1.0"
- [ ] CHANGELOG.md is complete and accurate

### At Release

- [ ] Git tag v0.1.0 created
- [ ] Package published to crates.io
- [ ] GitHub release created with notes
- [ ] crates.io shows v0.1.0 as latest

### Post-Release

- [ ] Documentation on docs.rs is updated
- [ ] Examples work with `fraiseql-wire = "0.1"`

---

## Acceptance Criteria

✅ **v0.1.0 is released when**:

1. Git tag `v0.1.0` exists
2. crates.io shows `fraiseql-wire v0.1.0`
3. GitHub release page exists with CHANGELOG
4. All tests still pass
5. Documentation is accessible on docs.rs

---

## Commit Strategy

**Do NOT commit yet.** After completing this phase:

```bash
# After all steps complete, create commit with everything
git add CHANGELOG.md
git commit -m "release(v0.1.0): Initial stable release

Features:

- Streaming JSON queries via Postgres simple query protocol
- TLS and SCRAM-SHA256 authentication
- Hybrid SQL + Rust predicates
- Bounded memory usage (2000x more efficient)
- 120+ integration tests
- Complete documentation suite

See CHANGELOG.md for full details.
"

# Then push tag
git push origin v0.1.0
```

---

## Do NOT Do

- ❌ Don't change the API (it's already stable)
- ❌ Don't add features (this is a release phase, not feature development)
- ❌ Don't remove or rename examples
- ❌ Don't change major dependencies
- ❌ Don't commit before all verification passes

---

## Time Estimate

**Total: ~75 minutes**

- API Review: 15 min
- CHANGELOG creation: 30 min
- Version & tagging: 15 min
- Test suite: 15 min
- Publication: 10 min

You can run this in one sitting or spread across a couple of hours. Publication is the only part that requires external coordination (crates.io API).

---

## Why v0.1 Makes Sense

- ✅ Feature-complete for scope
- ✅ Production-ready performance
- ✅ Comprehensive testing & documentation
- ✅ Stable API design
- ✅ Auditable codebase (from-scratch protocol impl)

Starting at 0.1 signals: "This is a deliberate, focused tool that does one thing well, not a half-baked MVP."

Shipping earlier versions (0.1 vs 0.5 vs 1.0) is about *communication*, not capability.
