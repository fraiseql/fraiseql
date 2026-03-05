# FraiseQL Remediation Plan — Extension 20

## Scope

This extension covers issues found during a fresh assessment focused on:
- Recently modified files (`async_validators.rs`, `mod.rs`, `coordinator.rs`, `failover.rs`, `propagation.rs`, `in_memory.rs`)
- The `fraiseql` facade crate architecture
- Validation system integration gaps

None of these tracks appear in Extensions 1–19.

---

## Track HH — Facade Crate: `fraiseql-cli` Compiled Unconditionally

### HH1 — `fraiseql-cli` is not an optional dependency in the facade

**Files**:
- `crates/fraiseql/Cargo.toml:3` — `fraiseql-cli = {workspace = true}` (no `optional = true`)
- `crates/fraiseql/src/lib.rs:65` — `pub use fraiseql_cli as cli;` (no `#[cfg(feature = "cli")]`)
- `crates/fraiseql/Cargo.toml` features section — `cli = []` (inert placeholder, never toggles the dep)

**Impact**: Every crate or binary that depends on `fraiseql` — including minimal runtime deployments — compiles the full CLI toolchain. `fraiseql-cli` transitively pulls in `clap`, file I/O, schema diffing, migration tooling, SBOM generation, and code-generation backends. These add significant compile time and binary size to server deployments that will never invoke the CLI.

The `minimal = []` feature profile is especially misleading: it is documented as "Core only, no database backends" but still forces in the complete CLI.

**Fix**:
```toml
# Cargo.toml [dependencies]
fraiseql-cli = {workspace = true, optional = true}

# Cargo.toml [features]
cli = ["dep:fraiseql-cli"]
full = ["server", "observers", "arrow", "wire", "postgres", "mysql", "sqlite", "cli"]
```

```rust
// lib.rs
#[cfg(feature = "cli")]
pub use fraiseql_cli as cli;
```

The standalone `fraiseql` binary (`src/main.rs`) must add `fraiseql = { ..., features = ["cli"] }` to its own dev-or-build path, or simply enable it via the workspace manifest's bin feature.

---

## Track II — Validation Architecture: `EmailFormatValidator` / `PhoneE164Validator` Orphaned from Schema Pipeline

### II1 — No `ValidationRule` variant for email or phone format

**Files**:
- `crates/fraiseql-core/src/validation/rules.rs` — `ValidationRule` enum has no `Email` or `Phone` variant
- `crates/fraiseql-core/src/validation/validators.rs:226` — `create_validator_from_rule()` returns `None` for all `AsyncValidatorProvider`-backed types
- `crates/fraiseql-core/src/runtime/input_validator.rs` — `validate_string_field()` has `_ => Ok(())` fallthrough

**Context**: The recent commit replaced `MockEmailDomainValidator` / `MockPhoneNumberValidator` with the genuine `EmailFormatValidator` and `PhoneE164Validator`. The new validators do real local-regex validation — a real improvement. However, they are completely disconnected from the schema compilation pipeline that is FraiseQL's core value proposition.

The `ValidationRule` enum drives: schema compilation, runtime dispatch via `validate_string_field()`, introspection output (`field_resolver.rs`), and custom type registry. Email/phone format rules can only be applied programmatically, not declared in the schema — so compiled schemas cannot express these constraints. This directly contradicts the architecture principle: "validation rules embedded in `schema.compiled.json`".

**How the gap manifests**:
```rust
// A schema field declaring email validation:
// validation_rules: [{"type": "email"}]
//
// Runtime: create_validator_from_rule(&ValidationRule::Email { .. })
//   → returns None  (silently skips)
// OR: no ValidationRule::Email variant can even be deserialized
```

**Fix — two-step**:

1. Add `ValidationRule::Email` and `ValidationRule::Phone` variants to `rules.rs`:
```rust
/// Email address format validation (RFC 5321 practical subset).
#[serde(rename = "email")]
Email,

/// E.164 international phone number format validation.
#[serde(rename = "phone_e164")]
PhoneE164,
```

2. Dispatch them in `validate_string_field()` in `input_validator.rs`:
```rust
ValidationRule::Email => {
    if EMAIL_REGEX.is_match(value) { Ok(()) }
    else { Err(FraiseQLError::Validation { .. }) }
},
ValidationRule::PhoneE164 => { .. },
```

Alternatively, if async dispatch is desired, thread the validators through the existing `AsyncValidatorProvider` dispatch mechanism (which already has `EmailFormatCheck` and `PhoneE164Check` variants) — but this requires making the input validator async.

---

### II2 — `AsyncValidator::timeout()` returns `Duration::ZERO` for both concrete implementations

**Files**:
- `crates/fraiseql-core/src/validation/async_validators.rs:154` — `EmailFormatValidator::new()` uses `timeout_ms = 0`
- `crates/fraiseql-core/src/validation/async_validators.rs:215` — `PhoneE164Validator::new()` uses `timeout_ms = 0`

**Impact**: The `AsyncValidator` trait documents that implementations "should handle timeout and error cases gracefully" and exposes `fn timeout(&self) -> Duration` as part of the public contract. The contract implies callers may use `timeout()` to enforce deadlines:

```rust
// Reasonable caller pattern:
tokio::time::timeout(validator.timeout(), validator.validate_async(value, field)).await
```

With `Duration::ZERO` this returns `Elapsed` immediately for every call, failing all validation. There is no way for callers to distinguish "zero means no timeout" from "zero means instant timeout". The tests that assert `timeout() == 0ms` cement this semantically ambiguous contract.

Since these validators are purely synchronous (regex-only, no I/O), the correct representation is either:
- A sentinel value (`Duration::MAX`) meaning "no timeout applies"
- Or simply not implementing the `AsyncValidator` trait and instead exposing a synchronous `Validator` trait impl

**Fix option A** — use `Duration::MAX` as "no timeout":
```rust
// in new() constructors:
config: AsyncValidatorConfig::new(AsyncValidatorProvider::EmailFormatCheck, u64::MAX),
// update AsyncValidatorConfig docs: 0 means instant, u64::MAX means unlimited
```

**Fix option B** — expose `EmailFormatValidator` as both `Validator` (sync) and `AsyncValidator`:
```rust
impl Validator for EmailFormatValidator {
    fn validate(&self, value: &str, field: &str) -> Result<()> {
        // same regex check, no async overhead
    }
}
```
This makes the validation pipeline connect properly (see II1 above).

---

## Track JJ — Span ID Entropy Reduction in `child_span_id()`

### JJ1 — UUID v4 version nibble reduces span ID entropy by 4 bits

**File**: `crates/fraiseql-observers/src/tracing/propagation.rs:138-150`

**Current implementation**:
```rust
pub fn child_span_id(&self) -> String {
    let uuid_bytes = *uuid::Uuid::new_v4().as_bytes();
    format!(
        "{:016x}",
        u64::from_be_bytes([
            uuid_bytes[0], uuid_bytes[1], uuid_bytes[2], uuid_bytes[3],
            uuid_bytes[4], uuid_bytes[5], uuid_bytes[6], uuid_bytes[7],
        ])
    )
}
```

UUID v4 layout (RFC 4122): byte 6's upper nibble is fixed to `0x4` (version field), and byte 8's two high bits are fixed to `10` (variant field). By extracting bytes 0–7, byte 6 always contributes a `4?` hex pattern, reducing entropy from 64 to ~60 bits. This is benign for collision resistance (2^60 is still very large) but the implementation is misleading — it appears to use 8 random bytes but silently has 4 fixed bits.

The previous implementation was worse (a simple counter), so this is an improvement. However, a cleaner approach exists: use `getrandom` or `rand`'s `random::<u64>()` which are already transitive dependencies via `uuid`:

```rust
pub fn child_span_id(&self) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    // uuid is already a dep — use its lower-level API for full entropy:
    let bytes = uuid::Uuid::new_v4().as_bytes()[8..16]
        .try_into()
        .expect("slice is 8 bytes");
    format!("{:016x}", u64::from_be_bytes(bytes))
}
```

Bytes 8–15 contain: 2 fixed variant bits + 62 random bits — slightly better. The cleanest fix is `rand::random::<u64>()` or `getrandom::getrandom`.

**Priority**: Low — 60-bit entropy is sufficient for span IDs in practice.

---

## Summary Table

| ID   | Severity | File(s) | Issue |
|------|----------|---------|-------|
| HH1  | High     | `crates/fraiseql/Cargo.toml:3`, `crates/fraiseql/src/lib.rs:65` | `fraiseql-cli` always compiled; `cli` feature is a no-op |
| II1  | Medium   | `validation/rules.rs`, `runtime/input_validator.rs`, `validation/validators.rs:226` | `EmailFormatValidator`/`PhoneE164Validator` not reachable from schema validation pipeline |
| II2  | Medium   | `validation/async_validators.rs:154,215` | `AsyncValidator::timeout()` returns `Duration::ZERO`; callers wrapping in `tokio::time::timeout` fail instantly |
| JJ1  | Low      | `tracing/propagation.rs:138` | UUID v4 version nibble reduces span ID entropy from 64 to ~60 bits |
