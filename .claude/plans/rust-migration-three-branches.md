# Rust Migration - Three Branch Integration Plan

**Created**: 2025-12-30
**Target**: v1.9.0 release
**Status**: Planning

---

## Three Branches to Integrate

### Branch 1: `fix/where-clause-edge-cases` (current branch)

**Commits**: 43 since divergence
**Key Work**:

- ✅ WHERE clause filtering bug fix (Issue #124)
- ✅ ID type implementation
- ✅ Python builtin shadowing fix
- ✅ Architecture documentation (4 new diagrams)
- ✅ Field documentation improvements
- ✅ Documentation quality improvements (100+ fixes)

### Branch 2: `fix/nested-field-selection-bug`

**Commits**: ~60 unique commits
**Key Work**:

- ✅ Rust memory safety improvements (Arena allocator bounds)
- ✅ Panic elimination in production code paths
- ✅ JSON recursion depth limits
- ✅ Chaos test stability (145/145 passing)
- ✅ Nested field selection optimization
- ✅ Clippy strict warnings eliminated

### Branch 3: `feature/rust-postgres-driver`

**Commits**: 60 since divergence
**Key Work**:

- ✅ Full Rust PostgreSQL backend (tokio-postgres + deadpool)
- ✅ 66,992 LOC of Rust database code
- ✅ Chaos engineering tests (57 tests, 34 passing)
- ✅ 9-phase implementation plan
- ✅ Complete Rust backend documentation

---

## Merge Order Strategy

### Option 1: Sequential Three-Way Merge (SAFEST)

**Timeline**: 4-5 days

```
Step 1: Merge nested-field-selection → where-clause-edge-cases
Step 2: Merge combined → rust-postgres-driver
Step 3: Test everything
Step 4: Merge to dev → v1.9.0
```

**Rationale**:

- Combines the two smaller feature branches first
- Then merges into the large Rust backend
- Easier to track what breaks at each step

---

### Option 2: Parallel Merge into Rust Branch (FASTER)

**Timeline**: 3-4 days

```
Step 1: Merge where-clause-edge-cases → rust-postgres-driver
Step 2: Merge nested-field-selection → rust-postgres-driver
Step 3: Test everything
Step 4: Merge to dev → v1.9.0
```

**Rationale**:

- Rust backend is the "base" (biggest change)
- Merge both feature branches into it
- Slightly faster but harder to debug conflicts

---

### Option 3: Create Integration Branch from Dev (CLEANEST)

**Timeline**: 4-5 days

```
Step 1: Create integrate/v1.9.0-all-features from dev
Step 2: Merge rust-postgres-driver → integration
Step 3: Merge where-clause-edge-cases → integration
Step 4: Merge nested-field-selection → integration
Step 5: Test everything
Step 6: Merge integration → dev → v1.9.0
```

**Rationale**:

- Clean integration branch
- All branches merge to neutral base
- Easiest to revert individual merges if issues

---

## Recommended: Option 1 (Sequential Three-Way)

### Detailed Execution Plan

#### Phase 1: Merge Nested-Field into WHERE Clause (Day 1)

**Goal**: Combine the two smaller feature branches

```bash
# Step 1: Ensure we're on WHERE clause branch
git checkout fix/where-clause-edge-cases

# Step 2: Create integration branch
git checkout -b integrate/combined-fixes

# Step 3: Merge nested-field fixes
git merge fix/nested-field-selection-bug

# Step 4: Resolve conflicts (expected minimal)
# - Likely conflicts: Cargo.toml versions, CHANGELOG.md
# - Resolution: Keep both changes where possible

# Step 5: Test
make test

# Step 6: Commit merge
git commit -m "merge: combine WHERE clause + Rust safety fixes"
```

**Expected Conflicts**:

- `Cargo.toml` / `fraiseql_rs/Cargo.toml` - version numbers
- `CHANGELOG.md` - both have v1.9.0 entries
- Possibly some doc files

**Deliverable**: Integration branch with both feature sets

---

#### Phase 2: Merge Combined into Rust Backend (Day 2-3)

**Goal**: Integrate all fixes into full Rust backend

```bash
# Step 1: Checkout Rust backend
git checkout feature/rust-postgres-driver

# Step 2: Create final integration branch
git checkout -b integrate/rust-backend-v1.9.0

# Step 3: Merge combined fixes
git merge integrate/combined-fixes

# Step 4: Resolve conflicts (this is the big one)
# Expected conflicts in:
# - fraiseql_rs/src/ (Rust code - Rust backend vs Rust improvements)
# - docs/ (architecture docs vs Rust backend docs)
# - Cargo.toml (versions)
# - CHANGELOG.md (merge all entries)

# Step 5: Test incrementally
make build
make test

# Step 6: Fix any test failures

# Step 7: Commit merge
git commit -m "merge: integrate all v1.9.0 features (WHERE fixes + Rust improvements + Rust backend)"
```

**Expected Conflicts**:

1. **Rust Code** - Both branches modify `fraiseql_rs/src/`:
   - nested-field: Arena safety, panic fixes
   - rust-backend: Full database layer
   - Resolution: Keep both (they likely touch different files)

2. **Documentation**:
   - where-clause: Architecture diagrams
   - nested-field: Performance docs
   - rust-backend: Rust migration docs
   - Resolution: Merge all docs, update navigation

3. **Cargo.toml**:
   - All three have dependencies
   - Resolution: Merge all dependencies

**Deliverable**: Fully integrated v1.9.0 branch

---

#### Phase 3: Testing & Validation (Day 3-4)

**Goal**: Verify everything works together

**Test Suite**:

```bash
# 1. Basic build
make build

# 2. Full test suite
make test

# 3. Specific regression tests
pytest tests/unit/test_where_clause.py -v  # WHERE fixes
pytest tests/unit/test_id_type.py -v        # ID type
pytest tests/chaos/ -v                      # Chaos tests

# 4. Rust-specific tests
cargo test --manifest-path fraiseql_rs/Cargo.toml

# 5. Performance benchmarks
make benchmark

# 6. Memory profiling (if available)
make profile
```

**Expected Issues**:

- Some chaos tests may fail (34/57 → need fixes)
- Rust backend tests might conflict with safety improvements
- Documentation build might have broken links

**Fix Strategy**:

- Fix critical test failures (WHERE clause, ID type must pass)
- Document known issues (chaos tests) for post-release
- Update docs as needed

**Deliverable**: All critical tests passing

---

#### Phase 4: Merge to Dev & Release (Day 4-5)

**Goal**: Ship v1.9.0 with all features

```bash
# Step 1: Final review
git log --oneline origin/dev..integrate/rust-backend-v1.9.0

# Step 2: Create PR
git push -u origin integrate/rust-backend-v1.9.0

gh pr create --base dev \
  --title "feat: v1.9.0 - Full Rust Core + Critical Fixes" \
  --body-file .github/pr-v1.9.0-integration.md

# Step 3: Get approval and merge
gh pr merge --squash

# Step 4: Tag and release
git checkout dev
git pull origin dev
git tag v1.9.0
git push origin v1.9.0

gh release create v1.9.0 \
  --title "v1.9.0 - Full Rust Core" \
  --notes-file .github/release-notes-v1.9.0.md
```

**Deliverable**: v1.9.0 released!

---

## Critical Work from All Branches

### Must Preserve from fix/where-clause-edge-cases

- [x] WHERE clause fixes (src/fraiseql/where_normalization.py)
- [x] WHERE clause tests (tests/unit/test_where_clause.py)
- [x] ID type (src/fraiseql/types/scalars/id_scalar.py)
- [x] ID in CLI (src/fraiseql/cli/commands/generate.py)
- [x] Builtin shadowing fix (src/fraiseql/patterns/trinity.py)
- [x] Architecture diagrams (docs/architecture/*.md)
- [x] Field documentation (docs/core/types-and-schema.md)

### Must Preserve from fix/nested-field-selection-bug

- [x] Arena safety (fraiseql_rs/src/core/arena.rs)
- [x] Panic elimination (fraiseql_rs/src/core/*.rs)
- [x] JSON recursion limits (fraiseql_rs/src/core/transform.rs)
- [x] Chaos test fixes (tests/chaos/)
- [x] Clippy improvements (all Rust code)
- [x] Performance docs (docs/performance/)

### Must Preserve from feature/rust-postgres-driver

- [x] Rust database layer (fraiseql_rs/src/db/*)
- [x] tokio-postgres integration (fraiseql_rs/Cargo.toml)
- [x] Connection pooling (fraiseql_rs/src/db/pool.rs)
- [x] Rust WHERE builder (fraiseql_rs/src/db/where_builder.rs)
- [x] GraphQL parser (fraiseql_rs/src/graphql/*)
- [x] Migration documentation (.phases/rust-postgres-driver/)

---

## Conflict Resolution Guide

### File-by-File Strategy

#### src/fraiseql/where_normalization.py

- **Status**: where-clause has fixes, rust-backend may have changes
- **Resolution**: Prefer where-clause version (has bug fixes)
- **Action**: `git checkout --theirs src/fraiseql/where_normalization.py`

#### fraiseql_rs/src/core/arena.rs

- **Status**: nested-field has safety improvements, rust-backend may differ
- **Resolution**: Merge both (safety + backend changes)
- **Action**: Manual merge

#### docs/architecture/README.md

- **Status**: All three branches may have changes
- **Resolution**: Merge all sections
- **Action**: Manual merge, update navigation

#### Cargo.toml / fraiseql_rs/Cargo.toml

- **Status**: All three have dependency changes
- **Resolution**: Merge all dependencies
- **Action**: Combine all `[dependencies]` entries

#### CHANGELOG.md

- **Status**: All three have v1.9.0 entries
- **Resolution**: Combine all entries under single v1.9.0
- **Action**: Manual merge, organize by category

---

## Timeline Summary

| Phase | Duration | Key Activities |
|-------|----------|----------------|
| **Phase 1** | 1 day | Merge nested-field → where-clause |
| **Phase 2** | 1-2 days | Merge combined → rust-backend |
| **Phase 3** | 1-2 days | Test everything, fix issues |
| **Phase 4** | 1 day | PR, merge, release |

**Total**: 4-5 days (20-30 hours)

---

## Success Criteria

### Must Have

- ✅ All 5991+ tests pass
- ✅ WHERE clause fixes work (Issue #124 tests pass)
- ✅ ID type works
- ✅ Rust safety improvements present (no panics)
- ✅ Rust backend works (basic queries/mutations)
- ✅ Build succeeds

### Should Have

- ✅ Chaos tests improved (>34/57 passing)
- ✅ Performance benchmarks show improvement
- ✅ Documentation complete and accurate
- ✅ All architecture diagrams render

### Nice to Have

- ✅ 100% chaos test pass rate
- ✅ Zero compiler warnings
- ✅ Memory profiling shows improvements

---

## Backup & Rollback Plan

### Before Starting

```bash
# Backup all three branches
git branch backup/where-clause-fixes fix/where-clause-edge-cases
git branch backup/nested-field-fixes fix/nested-field-selection-bug
git branch backup/rust-backend feature/rust-postgres-driver

# Push backups
git push origin backup/where-clause-fixes
git push origin backup/nested-field-fixes
git push origin backup/rust-backend
```

### If Integration Fails

**Option A**: Use where-clause branch only for v1.9.0

```bash
git checkout dev
git merge fix/where-clause-edge-cases
# Release v1.9.0 with fixes but without Rust backend
```

**Option B**: Delay v1.9.0

```bash
# Take more time to integrate properly
# Release v1.8.x patch instead
```

**Option C**: Release separate features in stages

```bash
# v1.9.0: WHERE fixes + ID type
# v1.10.0: Rust safety improvements
# v2.0.0: Full Rust backend
```

---

## Detailed Conflict Analysis

### Expected Conflicts by File

| File | Branches | Conflict Type | Resolution |
|------|----------|---------------|------------|
| `src/fraiseql/where_normalization.py` | where-clause | Bug fixes | Keep where-clause |
| `fraiseql_rs/src/core/arena.rs` | nested-field, rust-backend | Both modify | Merge both |
| `fraiseql_rs/Cargo.toml` | All three | Dependencies | Merge all deps |
| `docs/architecture/README.md` | where-clause, rust-backend | New docs | Merge both |
| `CHANGELOG.md` | All three | v1.9.0 entries | Combine all |
| `src/fraiseql/__init__.py` | All three | Version | Keep 1.9.0 |
| `mkdocs.yml` | where-clause, nested-field | Navigation | Merge both |

---

## Next Steps (Today)

Ready to start? Here's what I'll do:

### Immediate Actions (30 minutes)

1. **Backup all branches**:

   ```bash
   git branch backup/where-clause-fixes
   git branch backup/nested-field-fixes
   git branch backup/rust-backend
   ```

2. **Analyze conflicts**:

   ```bash
   # Test merge 1: nested-field → where-clause
   git checkout -b test/merge1 fix/where-clause-edge-cases
   git merge --no-commit fix/nested-field-selection-bug
   git diff --name-only --diff-filter=U > conflicts-merge1.txt
   git merge --abort

   # Test merge 2: combined → rust-backend
   git checkout -b test/merge2 feature/rust-postgres-driver
   git merge --no-commit test/merge1
   git diff --name-only --diff-filter=U > conflicts-merge2.txt
   git merge --abort
   ```

3. **Document conflicts** and create resolution plan

**Should I proceed with these immediate actions?** 🚀
