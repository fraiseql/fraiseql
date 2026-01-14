# Clippy Strict Pedantic Compliance Plan

## Objective
Make fraiseql-wire pass `cargo clippy -- -D warnings -W clippy::pedantic` without errors.

## Current Status
- **Total Violations**: 496 errors across entire codebase
- **Violations in operators module**: ~31 missing documentation errors (newly created code)
- **Violations in existing code**: ~465 errors in protocol, auth, stream, connection, metrics, util modules

## Violation Breakdown (By Category)

### Top 5 Violation Types (across entire codebase)

| Category | Count | Priority | Effort |
|----------|-------|----------|--------|
| Missing documentation for struct fields | 31 | **HIGH** | Medium |
| Item in documentation missing backticks | 120 | **MEDIUM** | Low |
| Variables in format strings (use inline) | 95 | **MEDIUM** | Low |
| Missing `#[must_use]` attribute | 44 | **MEDIUM** | Low |
| Missing `# Errors` in Result docs | 39 | **MEDIUM** | Low |
| Missing `#[must_use]` on `Self` methods | 23 | **MEDIUM** | Low |

### Geographic Distribution

**Operators Module** (newly created, ~31 errors):
- Missing struct field documentation: ~31 errors
- Unused import: 1 error
- All violations are in `where_operator.rs` and `field.rs`

**Existing Codebase** (~465 errors):
- `auth/scram.rs`: Format strings, must_use, error docs
- `connection/conn.rs`: Unused variables, assignments, format strings
- `stream/`: Dead code fields, format strings, must_use
- `protocol/`: Format strings, must_use, error docs
- `metrics/`: Must_use, error docs
- `util/`: Must_use

## Implementation Strategy

### Phase 1: Fix Operators Module (Quick Win) - ~1 hour
**Goal**: Make newly-created operators module fully compliant

**Tasks**:
1. **Remove unused import** in `where_operator.rs:7`
   - Remove: `use super::order_by::FieldSource;`
   - Impact: 1 error fixed

2. **Add struct field documentation** across operators module
   - Files: `where_operator.rs`, `field.rs`, `order_by.rs`
   - Add doc comments for each field in enums and structs
   - Pattern:
     ```rust
     /// Description of what this field represents
     field: Type,
     ```
   - Expected lines to add: ~150-200 lines of documentation
   - Violations fixed: ~31 errors

3. **Verify operators module clean**
   - Run: `cargo clippy -- -D warnings -W clippy::pedantic src/operators/` -lib
   - Result: 0 errors

### Phase 2: Fix Existing Codebase Issues (Systematic) - ~3-4 hours

This handles the bulk of violations in the existing, pre-operators code.

#### Phase 2a: Low-Hanging Fruit - Documentation Fixes (~1.5 hours)
1. **Backticks in documentation** (120 errors)
   - Find: Documentation items missing backticks
   - Fix: Wrap code items in backticks (e.g., `` `Field` ``, `` `JsonStream` ``)
   - Files affected: Multiple (auth, protocol, stream, client)

2. **Missing `# Errors` sections** (39 errors)
   - Find: Functions returning `Result` without error docs
   - Fix: Add `# Errors` section to doc comments
   - Pattern:
     ```rust
     /// Does something.
     ///
     /// # Errors
     ///
     /// Returns an error if [specific condition].
     ```

3. **Missing `# Panics` sections** (4 errors)
   - Find: Functions that can panic without documenting it
   - Fix: Add `# Panics` section if panics are intentional

#### Phase 2b: Format String Modernization (~1 hour)
1. **Variables directly in format strings** (95 errors)
   - Find: `format!("{}", var)` style
   - Fix: Use inline syntax: `format!("{var}")`
   - Script approach: Can be automated with regex replacements
   - Example:
     ```rust
     // Before
     write!(f, "Error: {}", msg)

     // After
     write!(f, "Error: {msg}")
     ```

#### Phase 2c: `#[must_use]` Attributes (~67 errors)
1. **Methods without `#[must_use]`** (44 errors)
   - Find: Methods that return `T` and are commonly forgotten
   - Fix: Add `#[must_use]` attribute
   - Pattern:
     ```rust
     #[must_use]
     pub fn method(&self) -> Result<String> { ... }
     ```

2. **Methods returning `Self` without `#[must_use]`** (23 errors)
   - Find: Builder methods and similar
   - Fix: Add `#[must_use = "builder method returns Self, should be used in a chain or assigned"]`

#### Phase 2d: Dead Code Cleanup (~13 errors)
1. **Unused fields** (6-8 errors)
   - Evaluate: Are they truly unused or are they used by serialization/debug derives?
   - Options:
     - Add `#[allow(dead_code)]` with justification if used by derives
     - Remove if actually unused

2. **Unused variables** (4-5 errors)
   - Fix: Prefix with `_` (e.g., `let _status = ...`)
   - Or: Use them (if possible)

3. **Unused methods** (1-2 errors)
   - Evaluate: Remove or mark with `#[allow(dead_code)]`

#### Phase 2e: Other Violations (~10 errors)
1. **Numeric precision warnings** (13 errors)
   - Type: `u64` to `f64` cast precision loss
   - Fix: Either accept loss with comment or use alternative type
   - Pattern: `#[allow(clippy::cast_precision_loss)]` or refactor

2. **Implicit patterns** (4 errors)
   - Pattern matches like `()` that should be explicit
   - Fix: Use specific destructuring

3. **Redundant code** (6 errors)
   - Unnecessary `.to_vec()`, `.clone()`, etc.
   - Fix: Remove the redundant operation

### Phase 3: Configuration & Verification (~30 minutes)

1. **Add clippy configuration to Cargo.toml or lib.rs**
   ```rust
   #![deny(clippy::pedantic)]
   #![allow(clippy::module_name_repetitions)] // If needed for naming conventions
   ```

2. **Run full verification**
   ```bash
   cargo clippy -- -D warnings -W clippy::pedantic
   cargo test
   ```

3. **Verify no regressions**
   - Run all existing tests
   - Benchmark (if available)

## Detailed Implementation Order

### Order (Recommended Execution Sequence)

**Day 1 - Morning (Phase 1 & 2a)** (~2-3 hours):
1. ✅ Fix operators module (31 violations)
2. ✅ Add backticks in documentation (120 violations)
3. ✅ Add missing `# Errors` sections (39 violations)

**Day 1 - Afternoon (Phase 2b & 2c)** (~2-3 hours):
4. ✅ Modernize format strings (95 violations)
5. ✅ Add `#[must_use]` attributes (67 violations)

**Day 2 - Morning (Phase 2d & 2e)** (~1-2 hours):
6. ✅ Clean up dead code (13 violations)
7. ✅ Fix numeric precision and other warnings (~10 violations)

**Day 2 - Afternoon (Phase 3)** (~30 minutes):
8. ✅ Configuration and final verification

## Risk Assessment

**Low Risk**:
- Documentation additions (comments only)
- Format string modernization (no semantic change)
- `#[must_use]` additions (no semantic change)
- Prefixing unused variables with `_` (no semantic change)

**Medium Risk**:
- Removing truly unused fields/methods (verify they're not used elsewhere)
- Dead code cleanup (might affect serialization/derives)

**Verification Strategy**:
- Run `cargo test` after each phase
- Use `cargo clippy --fix` for auto-fixable issues where appropriate
- Manual review of risky changes

## Success Criteria

- ✅ `cargo clippy -- -D warnings -W clippy::pedantic` passes with 0 errors
- ✅ All existing tests pass
- ✅ No functional code changes (only documentation/attributes)
- ✅ Code remains backward compatible

## Files That Will Need Changes (By Module)

**Must Change**:
- `src/operators/where_operator.rs` - Add 30+ field docs
- `src/operators/field.rs` - Add field docs
- `src/operators/order_by.rs` - Add field docs
- `src/auth/scram.rs` - Format strings, docs, must_use
- `src/connection/conn.rs` - Unused vars, assignments
- `src/stream/*.rs` - Dead code, must_use
- `src/protocol/*.rs` - Format strings, docs
- Multiple files - Backticks in docs, errors sections

**Likely Changes**:
- `src/metrics/*.rs`
- `src/util/*.rs`
- `src/client/*.rs`
- `src/json/*.rs`

**Unlikely Changes**:
- `src/lib.rs` (main library exports)

## Estimated Effort

| Phase | Task | Hours | Effort |
|-------|------|-------|--------|
| 1 | Operators module docs | 1.0 | Low |
| 2a | Documentation fixes | 1.5 | Low |
| 2b | Format strings | 1.0 | Low |
| 2c | Must_use attributes | 1.0 | Medium |
| 2d | Dead code | 0.5 | Medium |
| 2e | Other violations | 0.5 | Medium |
| 3 | Config & verify | 0.5 | Low |
| **Total** | **~6 hours** | | |

## Tools & Automation

**Can Use**:
- `cargo clippy --fix` - Auto-fixes some violations
- Search & replace - For systematic format string fixes
- Script to add missing doc sections

**Cannot Use**:
- Full automation (requires understanding context)

## Notes

- The codebase is well-structured; most violations are in documentation/attributes
- Operators module is brand new, so adding docs is straightforward
- Existing violations are in mature code - need careful evaluation
- No breaking changes expected
- All changes are additive (docs) or corrective (fixing anti-patterns)
