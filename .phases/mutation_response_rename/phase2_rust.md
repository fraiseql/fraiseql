# Phase 2: Rust Layer Updates (v1.8.0)

## Objective
Update Rust code documentation and comments to use `mutation_response` as the canonical name.

## Duration
1 hour

## Note on v1.8.0 Strategy
The Rust layer doesn't interact with PostgreSQL type names directly - it parses JSON.
This phase only updates documentation/comments to reflect the new canonical name.

## Files to Modify
- `fraiseql_rs/src/mutation/mod.rs`
- `fraiseql_rs/src/lib.rs`

---

## Task 2.1: Update Mutation Module

**File**: `fraiseql_rs/src/mutation/mod.rs`

### Changes:
1. Line 3: Update module doc
   ```rust
   //! Transforms PostgreSQL mutation_response JSON into GraphQL responses.
   //! Note: mutation_result_v2 is deprecated but still supported (v1.8.0+).
   ```

2. Line 18: Update function doc
   ```rust
   /// 2. **Full format**: Complete mutation_response with status, message, etc.
   ///    (mutation_result_v2 also supported for backward compatibility)
   ```

3. Search and replace all comments:
   - Update primary references to use `mutation_response`
   - Add note about backward compatibility where relevant

### Verification:
```bash
! grep -i "mutation_result_v2" fraiseql_rs/src/mutation/mod.rs
grep "mutation_response" fraiseql_rs/src/mutation/mod.rs | wc -l
# Expected: 2-3 occurrences
```

---

## Task 2.2: Rebuild Rust

```bash
cd fraiseql_rs
cargo clean
cargo build --release
cargo test
```

**Expected**: All pass

---

## Acceptance Criteria
- [ ] Primary documentation uses `mutation_response`
- [ ] Backward compatibility notes added where relevant
- [ ] Rust builds successfully
- [ ] Rust tests pass

## Git Commit
```bash
git add fraiseql_rs/
git commit -m "docs(rust): update to use mutation_response terminology

- Update module documentation to use mutation_response
- Add backward compatibility notes for mutation_result_v2
- No functional changes (Rust parses JSON, not SQL types)"
```

## Next: Phase 3 - Python Layer

---

**Phase Status**: ⏸️ Ready to Start
**Version**: v1.8.0 (documentation only)
**Breaking**: No
