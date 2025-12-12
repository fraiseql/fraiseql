# Execution Guide - Auto-Populate Mutation Fields

## For the User (Human Orchestrator)

This guide tells you exactly what to run at each step.

---

## Phase 1: Research and Design

**What it does**: Reads codebase and documents implementation approach

**Commands to run**:
```bash
cd /home/lionel/code/fraiseql

# Run Phase 1 with opencode
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-1-research-and-design.md"

# Wait for completion, then report back to Claude Code:
# "Phase 1 complete"
```

**Expected duration**: 5-10 minutes

**What Claude Code will do next**: Review research findings and verify approach

---

## Phase 2: Implement Rust Changes

**What it does**: Modifies Rust response builder (adds 4 lines)

**Commands to run**:
```bash
cd /home/lionel/code/fraiseql

# Compile Rust extension first
cd fraiseql_rs && cargo build --release && cd ..

# Install updated extension
uv pip install -e .

# Run Phase 2 with opencode
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-2-implement-rust-changes.md"

# Wait for completion, then report back to Claude Code:
# "Phase 2 complete"
```

**Expected duration**: 10-15 minutes (mostly Rust compilation)

**What Claude Code will do next**: Verify Rust changes compile and Python imports work

---

## Phase 3: Test Implementation

**What it does**: Creates tests and verifies implementation works

**Commands to run**:
```bash
cd /home/lionel/code/fraiseql

# Run tests first to ensure clean state
cd fraiseql_rs && cargo test && cd ..

# Run Phase 3 with opencode
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-3-test-implementation.md"

# Wait for completion, then run final test verification:
cd fraiseql_rs && cargo test && cd ..

# Report back to Claude Code:
# "Phase 3 complete - all tests passing"
```

**Expected duration**: 15-20 minutes

**What Claude Code will do next**: Review test results and verify no regressions

---

## Phase 4: Documentation and Commit

**What it does**: Updates documentation and commits changes

**Commands to run**:
```bash
cd /home/lionel/code/fraiseql

# Run Phase 4 with opencode
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-4-documentation-and-commit.md"

# Wait for completion, then report back to Claude Code:
# "Phase 4 complete - ready for commit"
```

**Expected duration**: 20-30 minutes

**What Claude Code will do next**: Review documentation, verify commit message, and commit changes

---

## Complete Workflow (All Phases)

**If you want to run all phases sequentially**:

```bash
cd /home/lionel/code/fraiseql

# Phase 1: Research
echo "=== Phase 1: Research and Design ==="
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-1-research-and-design.md"
read -p "Phase 1 complete. Press Enter to continue to Phase 2..."

# Phase 2: Implement
echo "=== Phase 2: Implement Rust Changes ==="
cd fraiseql_rs && cargo build --release && cd ..
uv pip install -e .
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-2-implement-rust-changes.md"
read -p "Phase 2 complete. Press Enter to continue to Phase 3..."

# Phase 3: Test
echo "=== Phase 3: Test Implementation ==="
cd fraiseql_rs && cargo test && cd ..
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-3-test-implementation.md"
cd fraiseql_rs && cargo test && cd ..
read -p "Phase 3 complete. Press Enter to continue to Phase 4..."

# Phase 4: Document and Commit
echo "=== Phase 4: Documentation and Commit ==="
opencode run -m xai/grok-code-fast-1 ".phases/auto-populate-mutation-fields/phase-4-documentation-and-commit.md"
echo "=== All phases complete! ==="
```

---

## Alternative: Manual Implementation (No opencode)

If you prefer to implement manually (without opencode):

### Phase 1: Read the plans
```bash
cat .phases/auto-populate-mutation-fields/phase-1-research-and-design.md
# Read and understand the approach
```

### Phase 2: Make the changes
```bash
# Edit this file:
vim fraiseql_rs/src/mutation/response_builder.rs

# Add these 4 lines after line 106:
#
# // Add status (always "success" for success responses)
# obj.insert("status".to_string(), json!(result.status.to_string()));
#
# // Add errors (always empty array for success responses)
# obj.insert("errors".to_string(), json!([]));

# Compile
cd fraiseql_rs && cargo build --release && cd ..
uv pip install -e .
```

### Phase 3: Test
```bash
# Create test file
vim fraiseql_rs/src/mutation/tests/auto_populate_fields_tests.rs
# Copy content from phase-3-test-implementation.md

# Update mod.rs
echo "mod auto_populate_fields_tests;" >> fraiseql_rs/src/mutation/tests/mod.rs

# Run tests
cd fraiseql_rs && cargo test && cd ..
```

### Phase 4: Document
```bash
# Update CHANGELOG.md
vim CHANGELOG.md
# Add v1.9.0 entry (see phase-4-documentation-and-commit.md)

# Create migration guide
vim docs/migrations/v1.8-to-v1.9.md
# Copy content from phase-4-documentation-and-commit.md

# Commit
git add -A
git commit -m "feat(mutations): auto-populate status and errors fields in success responses"
```

---

## Quick Reference: Commands by Phase

| Phase | Main Command | Duration |
|-------|-------------|----------|
| 1 | `opencode run ".phases/auto-populate-mutation-fields/phase-1-research-and-design.md"` | 5-10 min |
| 2 | `cargo build --release && opencode run ".phases/auto-populate-mutation-fields/phase-2-implement-rust-changes.md"` | 10-15 min |
| 3 | `cargo test && opencode run ".phases/auto-populate-mutation-fields/phase-3-test-implementation.md"` | 15-20 min |
| 4 | `opencode run ".phases/auto-populate-mutation-fields/phase-4-documentation-and-commit.md"` | 20-30 min |

**Total estimated time**: 50-75 minutes

---

## What to Tell Claude Code

After each phase completes, report back to Claude Code with:

**Phase 1**:
```
"Phase 1 complete. Research findings documented."
```

**Phase 2**:
```
"Phase 2 complete. Rust changes compiled successfully."
```

**Phase 3**:
```
"Phase 3 complete. All tests passing (X existing + 6 new)."
```

**Phase 4**:
```
"Phase 4 complete. Documentation updated and ready for commit."
```

---

## Troubleshooting

### opencode not found
```bash
# Install opencode
pip install opencode

# Or use full path
python -m opencode run "..."
```

### Rust compilation fails
```bash
# Check error message carefully
cd fraiseql_rs
cargo build --release 2>&1 | tee build.log
cd ..

# Report error to Claude Code with:
# "Phase 2 failed with compilation error: [error message]"
```

### Tests fail
```bash
# Run with verbose output
cd fraiseql_rs
cargo test -- --nocapture 2>&1 | tee test.log
cd ..

# Report to Claude Code with:
# "Phase 3 failed with test errors: [error summary]"
```

### Python import fails
```bash
# Force reinstall
uv pip install --force-reinstall -e .

# Check if extension exists
ls fraiseql_rs/target/release/*.so

# Test import
python3 -c "import fraiseql._fraiseql_rs; print('OK')"
```

---

## Expected File Changes

After all phases complete, you should see:

**Modified files**:
- `fraiseql_rs/src/mutation/response_builder.rs` (+4 lines)
- `fraiseql_rs/src/mutation/tests/mod.rs` (+1 line)
- `CHANGELOG.md` (+~100 lines)

**New files**:
- `fraiseql_rs/src/mutation/tests/auto_populate_fields_tests.rs` (~200 lines)
- `docs/migrations/v1.8-to-v1.9.md` (~400 lines)
- `RELEASE_NOTES_v1.9.0.md` (~150 lines)

**Total changes**: ~855 lines (mostly documentation)

**Core implementation**: Only 4 lines of Rust code!

---

## Success Indicators

✅ **Phase 1 Success**:
- No errors during file reading
- Research document mentions "Option A: Rust-Only Solution"

✅ **Phase 2 Success**:
- Cargo build completes without errors
- Python can import: `python3 -c "import fraiseql._fraiseql_rs; print('OK')"`

✅ **Phase 3 Success**:
- Output shows: `test result: ok. XX passed; 0 failed`
- New tests present: `test mutation::tests::auto_populate_fields_tests::*`

✅ **Phase 4 Success**:
- CHANGELOG.md has "## [1.9.0]" entry
- Migration guide exists at `docs/migrations/v1.8-to-v1.9.md`
- All documentation spell-checked

---

## Final Verification

Before asking Claude Code to commit:

```bash
# Run all tests
cd fraiseql_rs && cargo test && cd ..

# Check for warnings
cd fraiseql_rs && cargo clippy && cd ..

# Verify Python tests (if any)
uv run pytest tests/ -v

# Review changes
git status
git diff

# Everything looks good? Tell Claude Code:
# "All phases complete and verified. Ready for final commit."
```

---

## Post-Implementation

After Claude Code commits:

1. **Create PR** (if using GitHub):
   ```bash
   git push origin feature/auto-populate-mutation-fields
   gh pr create --title "feat: Auto-populate mutation fields (v1.9.0)"
   ```

2. **Test in real project** (e.g., PrintOptim):
   ```bash
   cd ~/code/printoptim_backend
   uv pip install --upgrade /home/lionel/code/fraiseql
   # Test mutations
   ```

3. **Monitor for issues**:
   - Check if any tests break
   - Verify GraphQL responses have new fields
   - Test with frontend

---

**Questions?** Ask Claude Code for clarification on any step.

**Ready to start?** Run Phase 1 command and report back when complete!
