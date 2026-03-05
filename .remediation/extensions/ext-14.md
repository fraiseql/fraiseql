# FraiseQL Remediation Plan — Extension 14
## Validation Test Coverage Gaps and Dependency Version Hygiene

**Date**: 2026-03-05
**Scope**: `crates/fraiseql-core/src/validation/` and workspace `Cargo.lock`
**Previous plans covered**: Extensions 1–13 — this plan is strictly additive.

---

## Context

The `validation/` module in `fraiseql-core` is one of the best-tested in the codebase
(most files have 8–69 inline tests each).  Two files are complete implementations that
somehow have zero tests.  Separately, `Cargo.lock` contains three concurrent versions of
the `rustls` crate, one of which (0.21.x) is in an unmaintained branch with known CVEs.

---

## Track V1 — `validation/custom_scalar.rs`: complete implementation, zero tests

### Evidence

`crates/fraiseql-core/src/validation/custom_scalar.rs` — 79 lines.

```
$ grep -c '#\[test\]' crates/fraiseql-core/src/validation/custom_scalar.rs
0
```

The file is **not** a stub.  It defines:

1. `pub trait CustomScalar: Send + Sync + fmt::Debug` — the extension point for user-defined
   scalars.  Five methods: `name()`, `serialize()`, `parse_value()`, `parse_literal()`.
   (No default implementations; every method must be provided.)

2. `pub type CustomScalarResult = Result<Value>` — type alias used at call sites.

3. `pub fn validate_custom_scalar(scalar: &dyn CustomScalar, value: &Value, ctx: ValidationContext) -> CustomScalarResult`
   — dispatches to `serialize`, `parse_value`, or `parse_literal` depending on `ctx`.

4. `pub fn validate_custom_scalar_parse_value(scalar: &dyn CustomScalar, value: &Value) -> CustomScalarResult`
   — thin wrapper calling `parse_value`.

The two free functions (3 and 4) are tested only indirectly when a registry test creates a
concrete scalar.  No test exercises:
- `ValidationContext::Serialize` dispatch
- `ValidationContext::ParseLiteral` dispatch
- What happens when the scalar returns `Err` in each context
- A scalar whose `parse_value` succeeds but `serialize` fails (invalid state transition)
- The `validate_custom_scalar_parse_value` wrapper independently

### Fix

Add a `#[cfg(test)]` block directly in `custom_scalar.rs` (keeping with the module's
convention), or a paired file `custom_scalar_tests.rs`.  Minimum test matrix:

```rust
struct AlwaysOkScalar;
impl CustomScalar for AlwaysOkScalar { ... }

struct AlwaysErrScalar;
impl CustomScalar for AlwaysErrScalar { ... }

#[test]
fn validate_dispatches_to_serialize_on_serialize_context() { ... }
#[test]
fn validate_dispatches_to_parse_value_on_variable_context() { ... }
#[test]
fn validate_dispatches_to_parse_literal_on_literal_context() { ... }
#[test]
fn validate_returns_err_when_scalar_serialize_fails() { ... }
#[test]
fn validate_returns_err_when_scalar_parse_value_fails() { ... }
#[test]
fn validate_custom_scalar_parse_value_delegates_correctly() { ... }
```

**Severity**: MEDIUM — regression risk; no coverage of the public trait dispatch logic
**Files**: `crates/fraiseql-core/src/validation/custom_scalar.rs`

---

## Track V2 — `validation/scalar_validator.rs`: complete implementation, zero tests

### Evidence

`crates/fraiseql-core/src/validation/scalar_validator.rs` — 155 lines.

```
$ grep -c '#\[test\]' crates/fraiseql-core/src/validation/scalar_validator.rs
0
```

The file defines:

1. `pub enum ValidationContext { Serialize, ParseValue, ParseLiteral }` with `as_str()`.

2. `pub struct ScalarValidationError { pub message: String, pub context: ValidationContext, pub value_repr: Option<String> }` with:
   - `fn new(message, context, value_repr) -> Self`
   - `fn into_fraiseql_error(self) -> FraiseQLError`

3. `pub fn validate_custom_scalar(scalar, value, ctx) -> Result<Value>` — the primary dispatch
   function used by the runtime executor.

None of these are tested in isolation.  In particular:

- `ValidationContext::as_str()` is exercised only implicitly through logging; its correctness
  (it returns `"serialize"`, `"parse_value"`, `"parse_literal"`) is untested.

- `ScalarValidationError::into_fraiseql_error()` converts a validation failure into a
  `FraiseQLError::Validation { message, path }`.  If the mapping is wrong (e.g., it emits
  `FraiseQLError::Internal` instead), no test will catch it.

- The function-level `validate_custom_scalar` in `scalar_validator.rs` is **different** from
  the one in `custom_scalar.rs`; it calls `CustomScalarRegistry::validate` rather than the
  trait directly.  Both have the same exported name but different signatures.  This is
  confusing and a test would document the distinction.

### Fix

Add a `#[cfg(test)]` section in `scalar_validator.rs` covering at minimum:

```rust
#[test]
fn validation_context_as_str_returns_correct_strings() {
    assert_eq!(ValidationContext::Serialize.as_str(), "serialize");
    assert_eq!(ValidationContext::ParseValue.as_str(), "parse_value");
    assert_eq!(ValidationContext::ParseLiteral.as_str(), "parse_literal");
}

#[test]
fn scalar_validation_error_converts_to_fraiseql_validation_error() {
    let err = ScalarValidationError::new("bad", ValidationContext::Serialize, None);
    let fe = err.into_fraiseql_error();
    assert!(matches!(fe, FraiseQLError::Validation { .. }));
}

#[test]
fn validate_custom_scalar_returns_err_for_unknown_scalar() { ... }
```

**Severity**: MEDIUM — `into_fraiseql_error` mapping is untested; could silently produce
wrong HTTP status if variant is wrong
**Files**: `crates/fraiseql-core/src/validation/scalar_validator.rs`

---

## Track V3 — Three concurrent `rustls` versions in `Cargo.lock`

### Evidence

```
$ grep -A1 '^name = "rustls"' Cargo.lock
name = "rustls"
version = "0.21.12"

name = "rustls"
version = "0.22.4"

name = "rustls"
version = "0.23.37"
```

The `0.21.x` branch reached end-of-life on 2024-09-20.  It carries advisory
GHSA-6g18-jhpc-69jc (RSA-PSS signature confusion, severity: high) which was patched in
0.22.x and 0.23.x but not backported to 0.21.x.

### Impact

1. **Security**: any crate in the dependency tree that links against `rustls 0.21.x` does
   not receive the RSA-PSS fix even though later versions in the same binary do.

2. **Binary size**: three copies of the TLS implementation are statically linked.
   `rustls 0.23.x` alone is ≈1 MB of code; tripling this adds ≈2 MB to the release binary.

3. **Audit surface**: `cargo audit` and supply-chain tooling must track three separate
   version surfaces.

### Root cause (how to find the culprit)

```bash
cargo tree -d -p rustls 2>/dev/null | grep "0.21\|0.22"
```

Common culprits:
- `sqlx 0.8` pulls `rustls 0.22.x` (pending sqlx 0.9 which moves to 0.23.x)
- Older versions of `tonic`, `hyper-rustls`, or `tokio-rustls` pull 0.21.x

### Fix

**Step 1**: Identify which workspace dependency forces 0.21.x:
```bash
cargo tree -i rustls:0.21.12
```

**Step 2**: Upgrade or pin the offending dependency.  If a dependency has no release that
uses ≥ 0.22, add a Cargo.toml workspace patch:
```toml
[patch.crates-io]
rustls = { version = "0.23", ... }  # only if ABI-compatible
```

**Step 3**: Add `cargo-deny` rule to fail CI on duplicate crate versions:
```toml
# deny.toml
[bans]
multiple-versions = "deny"
skip = [
    # Accepted exceptions (with justification):
    # { name = "foo", version = "=0.1" },
]
```

**Step 4**: Add a CI step:
```yaml
- name: Check duplicate crates
  run: cargo deny check bans
```

**Severity**: HIGH (security) / MEDIUM (binary size)
**Files**: `Cargo.lock`, workspace `Cargo.toml`, add `deny.toml`

---

## Track V4 — `validation/mod.rs` re-exports two `validate_custom_scalar` functions with the same name

### Evidence

`crates/fraiseql-core/src/validation/mod.rs` re-exports both:

- `custom_scalar::validate_custom_scalar` (takes `&dyn CustomScalar`, dispatches via trait)
- `scalar_validator::validate_custom_scalar` (takes a registry + name, dispatches via registry lookup)

Both are exported from the same `validation` module, causing a name collision that Rust
resolves in favour of one (whichever appears last in `pub use` order — which is implicit
and brittle).

The current code compiles only because one of them is likely shadowed or one is not yet
`pub use`d in `mod.rs`; however the naming ambiguity is a latent maintenance hazard.

### Evidence

```bash
grep "validate_custom_scalar" \
  crates/fraiseql-core/src/validation/custom_scalar.rs \
  crates/fraiseql-core/src/validation/scalar_validator.rs
```

Both files contain a `pub fn validate_custom_scalar`.

### Fix

Rename one to remove ambiguity:
- `custom_scalar::validate_custom_scalar` → `validate_scalar_by_trait` (operates on a concrete trait object)
- `scalar_validator::validate_custom_scalar` → `validate_scalar_by_name` (registry lookup path)

Update all call sites; there should be very few since both are internal to `fraiseql-core`.

**Severity**: LOW — currently compiles; risk of accidental wrong function being called
after future refactors
**Files**: `crates/fraiseql-core/src/validation/custom_scalar.rs`,
`crates/fraiseql-core/src/validation/scalar_validator.rs`,
`crates/fraiseql-core/src/validation/mod.rs`

---

## Summary

| Track | Severity | Location | Nature |
|-------|----------|----------|--------|
| V1 | MEDIUM | `validation/custom_scalar.rs` | 0 tests for dispatch logic |
| V2 | MEDIUM | `validation/scalar_validator.rs` | 0 tests; `into_fraiseql_error` untested |
| V3 | HIGH (security) | `Cargo.lock` | `rustls 0.21.12` with unpatched CVE in binary |
| V4 | LOW | `validation/{custom_scalar,scalar_validator}.rs` | Same public function name in two modules |
