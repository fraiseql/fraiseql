# Quick Start: CASCADE Fix Implementation

**Time:** 4-6 hours
**Difficulty:** Low
**Prerequisites:** Rust, Python, FraiseQL repo cloned

---

## TL;DR

```bash
# 1. Create feature branch
cd /home/lionel/code/fraiseql
git checkout -b fix/cascade-nesting-v1.8.0a5

# 2. Create the parser module
# Copy code from 01_IMPLEMENTATION_PLAN.md → Step 2.1

# 3. Update mod.rs
# Add: mod postgres_composite;
# Update: build_mutation_response() to try new parser first

# 4. Run tests
cd fraiseql_rs
cargo test

# 5. Build and test with PrintOptim
uv build
uv pip install -e .
cd /home/lionel/code/printoptim_backend_manual_migration
uv run pytest tests/api/mutations/scd/allocation/test_debug_cascade_v2.py -v

# 6. Verify CASCADE location
# ✅ CASCADE at success level
# ✅ CASCADE NOT in entity

# 7. Commit and release
git commit -m "fix(mutations): CASCADE at success wrapper level (v1.8.0-alpha.5)"
uv publish
```

---

## Step-by-Step Guide

### Step 1: Setup (5 min)

```bash
cd /home/lionel/code/fraiseql
git checkout dev
git pull
git checkout -b fix/cascade-nesting-v1.8.0a5
```

### Step 2: Create Parser Module (1 hour)

**Create file:** `fraiseql_rs/src/mutation/postgres_composite.rs`

**Copy the complete module from:** `01_IMPLEMENTATION_PLAN.md` → Step 2.1

**Key points:**
- 8-field struct matching PrintOptim's `mutation_response`
- CASCADE at Position 7
- Error handling with clear messages

### Step 3: Update Entry Point (15 min)

**Edit:** `fraiseql_rs/src/mutation/mod.rs`

**Add near top:**
```rust
mod postgres_composite;  // NEW
```

**Update `build_mutation_response()` function:**
```rust
// Try 8-field parser first, fallback to simple format
let result = match postgres_composite::PostgresMutationResponse::from_json(mutation_json) {
    Ok(pg_response) => pg_response.to_mutation_result(entity_type),
    Err(_) => MutationResult::from_json(mutation_json, entity_type)?,
};
```

### Step 4: Add Tests (1 hour)

**Edit:** `fraiseql_rs/src/mutation/tests.rs`

**Add test module at end:**
```rust
#[cfg(test)]
mod cascade_fix_tests {
    use super::*;
    use crate::mutation::postgres_composite::PostgresMutationResponse;

    #[test]
    fn test_parse_8field_mutation_response() {
        let json = r#"{
            "status": "created",
            "message": "Success",
            "entity_id": "uuid",
            "entity_type": "Allocation",
            "entity": {"id": "uuid"},
            "updated_fields": [],
            "cascade": {"updated": []},
            "metadata": {}
        }"#;

        let result = PostgresMutationResponse::from_json(json).unwrap();
        assert_eq!(result.status, "created");
        assert!(result.cascade.is_some());
    }

    #[test]
    fn test_cascade_from_position_7() {
        let json = r#"{
            "status": "ok",
            "message": "OK",
            "entity": {},
            "cascade": {"updated": [{"id": "1"}]}
        }"#;

        let pg_response = PostgresMutationResponse::from_json(json).unwrap();
        let result = pg_response.to_mutation_result(None);

        assert!(result.cascade.is_some());
    }
}
```

### Step 5: Run Rust Tests (10 min)

```bash
cd fraiseql_rs

# Format code
cargo fmt

# Lint
cargo clippy --all-targets

# Run tests
cargo test

# Should see: test result: ok. XX passed; 0 failed
```

### Step 6: Build Python Package (10 min)

```bash
cd /home/lionel/code/fraiseql

# Build Rust extension
cd fraiseql_rs
cargo build --release

# Build Python package
cd ..
uv build

# Install locally
uv pip install -e .

# Verify version
python -c "import fraiseql; print('FraiseQL loaded successfully')"
```

### Step 7: Test with PrintOptim (30 min)

```bash
cd /home/lionel/code/printoptim_backend_manual_migration

# Run CASCADE diagnostic test
uv run pytest tests/api/mutations/scd/allocation/test_debug_cascade_v2.py::test_cascade_diagnostic_full_chain -v

# Look for in output:
# ✅ CASCADE at success level: {...}
# ✅ allocation object has NO cascade field

# Run broader mutation tests
uv run pytest tests/api/mutations/scd/allocation/ -v
```

### Step 8: Verify Fix (10 min)

**Check test output for:**

```json
{
  "createAllocation": {
    "__typename": "CreateAllocationSuccess",
    "allocation": {
      "__typename": "Allocation",
      "id": "...",
      // ✅ NO "cascade" field here
    },
    "cascade": {
      // ✅ CASCADE HERE - CORRECT!
      "updated": [...],
      "invalidations": [...]
    }
  }
}
```

### Step 9: Version Bump (10 min)

**Edit:** `fraiseql_rs/Cargo.toml`
```toml
version = "1.8.0-alpha.5"
```

**Edit:** `pyproject.toml`
```toml
version = "1.8.0a5"
```

**Edit:** `CHANGELOG.md` (add at top)
```markdown
## [1.8.0-alpha.5] - 2025-12-06

### Fixed
- CASCADE nesting bug: CASCADE now at success wrapper level, not in entity
- Added support for PrintOptim's 8-field mutation_response composite type

### Technical
- New postgres_composite module for 8-field parsing
- Zero breaking changes, backward compatible
```

### Step 10: Commit & Publish (15 min)

```bash
cd /home/lionel/code/fraiseql

# Stage changes
git add .

# Commit
git commit -m "fix(mutations): CASCADE at success wrapper level (v1.8.0-alpha.5)

- Add postgres_composite module for 8-field mutation_response
- Extract CASCADE from Position 7 (explicit field)
- Fix nesting bug: CASCADE now at wrapper, not entity
- Zero breaking changes

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"

# Build distribution
uv build

# Publish to PyPI
uv publish

# Verify
pip install fraiseql==1.8.0a5
python -c "import fraiseql; print(fraiseql.__version__)"
```

---

## Troubleshooting

### Issue: Compilation Error

**Error:**
```
error[E0432]: unresolved import `crate::mutation::postgres_composite`
```

**Fix:**
```rust
// Ensure this line is in fraiseql_rs/src/mutation/mod.rs
mod postgres_composite;
```

### Issue: Tests Fail

**Error:**
```
thread 'cascade_fix_tests::test_parse_8field_mutation_response' panicked
```

**Fix:**
- Check JSON structure has all 8 fields
- Verify field names match exactly (status, message, etc.)
- Check for typos in struct field names

### Issue: CASCADE Still in Entity

**Symptom:**
```json
"allocation": {
  "cascade": {...}  // Still here!
}
```

**Fix:**
- Verify `to_mutation_result()` extracts cascade from `self.cascade`
- Check `response_builder.rs` places cascade at wrapper level
- Ensure you're using the latest fraiseql build

### Issue: Build Fails

**Error:**
```
error: failed to compile `fraiseql` due to previous error
```

**Fix:**
```bash
# Clean and rebuild
cargo clean
cargo build

# Or just rebuild Python extension
cd fraiseql_rs
cargo build --release
```

---

## Verification Checklist

Before publishing:

- [ ] Rust tests pass: `cargo test`
- [ ] Python tests pass: `pytest tests/`
- [ ] PrintOptim tests pass
- [ ] CASCADE at success level (verified manually)
- [ ] CASCADE NOT in entity (verified manually)
- [ ] Version bumped in 2 files
- [ ] CHANGELOG updated
- [ ] Git commit created
- [ ] Package builds: `uv build`

---

## Time Estimate

| Task | Time |
|------|------|
| Setup & branch | 5 min |
| Create parser module | 1 hour |
| Update entry point | 15 min |
| Add tests | 1 hour |
| Run tests & fix issues | 30 min |
| Build Python package | 10 min |
| Test with PrintOptim | 30 min |
| Verify fix | 10 min |
| Version bump & changelog | 10 min |
| Commit & publish | 15 min |
| **Total** | **~4 hours** |

Add 1-2 hours buffer for unexpected issues.

---

## Success Criteria

You'll know you're done when:

1. ✅ All Rust tests pass
2. ✅ All Python tests pass
3. ✅ PrintOptim test shows CASCADE at correct level
4. ✅ No CASCADE in entity objects
5. ✅ Package published to PyPI
6. ✅ PrintOptim can upgrade to v1.8.0a5

🎉 **Phase Complete!**

---

## Next Steps

After release:

1. Update PrintOptim's `pyproject.toml`:
   ```toml
   fraiseql = ">=1.8.0a5"
   ```

2. Test in PrintOptim dev environment

3. Deploy to staging

4. Monitor for issues

5. Plan next FraiseQL release (stable 1.8.0)
