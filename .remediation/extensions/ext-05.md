# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension V

*Written 2026-03-05. Sixth assessor's findings.*
*Extends all four preceding plans (`extension`, `extension-2`, `extension-3`, `extension-4`)*
*without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*

---

## Context and Methodology

This assessment:
1. Read all five existing plans (base + four extensions) fully to avoid duplication.
2. Reviewed the unstaged `git diff` across six modified files to audit in-flight work.
3. Conducted targeted deep reads on `fraiseql-auth`, `fraiseql-wire`, `fraiseql-observers`,
   and `fraiseql-secrets` — the areas least covered by previous assessors.
4. Confirmed every finding against the source before reporting it.

Findings already covered in earlier plans are not repeated here. Notably:
- `stop_health_monitor` no-op is already tracked as **Extension III — L1** (same file, same fix).
  K3 below is therefore omitted.
- `eprintln!` in `fraiseql-server/src/backup/` is already tracked as **Extension I — G1**.
  I2 below covers the two additional `eprintln!` calls in `fraiseql-auth`, which are a separate crate.

---

## Unstaged Changes Assessment

The following six files have uncommitted changes at the time of this assessment.
Quality verdict on each:

| File | Change | Verdict |
|---|---|---|
| `fraiseql-core/src/validation/async_validators.rs` | Replaces mock validators with real local regex implementations (`EmailFormatValidator`, `PhoneE164Validator`) | ✅ Good. Removes `MockEmailDomainValidator` from the public API surface. |
| `fraiseql-core/src/validation/mod.rs` | Re-exports the new concrete validators, removes the mocks | ✅ Good. Public API now exports real types. |
| `fraiseql-observers/src/listener/coordinator.rs` | Adds `transition_listener_state()` public method | ✅ Good. Properly documented with `# Errors`. |
| `fraiseql-observers/src/listener/failover.rs` | Replaces stub comment with actual `transition_listener_state()` call | ✅ Good. Removes "In production, would transition state here" comment stub. |
| `fraiseql-observers/src/tracing/propagation.rs` | Replaces sequential span ID increment with UUID v4 randomness | ✅ Good. Old code: `unwrap_or(0) + 1` was deterministic and fragile. |
| `fraiseql-observers/src/transport/in_memory.rs` | Switches from unbounded to bounded MPSC channel (capacity 1 024) | ⚠️ Correct intent, introduces one new risk — see K2 below. |

No in-flight change introduces a regression. All are improvements. However, three of these
changes expose adjacent issues documented below.

---

## Track I — fraiseql-auth Security Gaps (Priority: High)

The previous assessors focused on `fraiseql-server` and `fraiseql-webhooks`. The `fraiseql-auth`
crate has its own security gaps not yet covered.

---

### I1 — JWT Audience Validation Disabled by Default

**File:** `crates/fraiseql-auth/src/jwt.rs:86`

**Problem:**

```rust
pub fn new(issuer: &str, algorithm: Algorithm) -> Result<Self> {
    let mut validation = Validation::new(algorithm);
    validation.set_issuer(&[issuer]);
    // Default: require audience validation, but allow any audience initially
    // Applications should call with_audiences() to restrict to specific audiences
    validation.validate_aud = false;   // ← audience validation disabled
    Ok(Self { ... })
}
```

The comment acknowledges the intent ("require audience validation") while doing the
opposite (`false`). The opt-in `.with_audiences()` method exists and re-enables it,
but it is never *required*.

**Impact:** A JWT minted for Service A (audience: `https://service-a.example.com`) is
accepted by Service B if both instances were initialized with `JwtValidator::new()` and
the same issuer — because neither checks the audience. This violates RFC 6749 §10.16
and OIDC Core §3.1.3.7. The risk is real in multi-service deployments sharing an
identity provider.

**Fix:**

Change the default to require audience validation and add an explicit escape hatch:

```rust
pub fn new(issuer: &str, algorithm: Algorithm) -> Result<Self> {
    let mut validation = Validation::new(algorithm);
    validation.set_issuer(&[issuer]);
    validation.validate_aud = true;      // ← secure default: audience required
    Ok(Self { ... })
}

/// Allow any audience claim (or no audience claim) in the token.
///
/// **Use only for development or when the OIDC provider does not include an `aud`
/// claim.** In production, prefer [`with_audiences`](Self::with_audiences) instead.
pub fn allow_any_audience(mut self) -> Self {
    self.validation.validate_aud = false;
    self
}
```

Update the one integration test that calls `JwtValidator::new()` directly without
`with_audiences()` to use `allow_any_audience()` explicitly.

**Acceptance:**
- `JwtValidator::new(issuer, alg).validate(token_with_wrong_audience)` → `Err(AuthError::InvalidToken)`
- `grep "validate_aud = false" crates/fraiseql-auth/src/jwt.rs` → only inside `allow_any_audience`

---

### I2 — `eprintln!` in Production Auth Code

**Files:**
- `crates/fraiseql-auth/src/jwt.rs:46` — system time failure in `Claims::is_expired()`
- `crates/fraiseql-auth/src/rate_limiting.rs:105` — system time failure in rate limiter

**Problem:**

Previous plan item G1 covered `eprintln!` in `fraiseql-server/src/backup/`. These two
occurrences are in `fraiseql-auth` — a separate crate — and were not covered.

```rust
// jwt.rs:46
eprintln!(
    "CRITICAL: System time error in token expiry check: {}. ...",
    e
);

// rate_limiting.rs:105
eprintln!(
    "CRITICAL: System time error in rate limiter: {}. ...",
    e
);
```

Both are in critical security paths. Operators running this in production with a
structured log aggregator (Datadog, Loki, CloudWatch) will never see these messages.

The fail-safe logic itself is correct:
- jwt.rs treats the token as expired (safe — deny access)
- rate_limiting.rs allows all requests through (safe — avoid lockout during clock issues)

Only the logging mechanism is wrong.

**Fix:**

```rust
// jwt.rs
tracing::error!(error = %e, "CRITICAL: system time error in token expiry check — token rejected");

// rate_limiting.rs
tracing::error!(
    error = %e,
    "CRITICAL: system time error in rate limiter — rate limiting temporarily disabled"
);
```

**Acceptance:**
- `grep -rn "eprintln!" crates/fraiseql-auth/src/ --include="*.rs"` → empty (excluding
  test modules where `eprintln!` to explain test skip is acceptable)

---

## Track J — fraiseql-wire Behavioral Correctness (Priority: Medium)

---

### J1 — LIKE Metacharacter Escape Gap in `Startswith`, `Endswith`, `Istartswith`, `Iendswith`

**File:** `crates/fraiseql-wire/src/operators/sql_gen.rs:272, 280, 288, 296`

**Problem:**

The `Startswith` and `Endswith` operators construct LIKE patterns by embedding `%` in
the bound parameter value:

```rust
// sql_gen.rs:268-274
WhereOperator::Startswith(field, prefix) => {
    // ...
    params.insert(param_num, Value::String(format!("{}%", prefix)));
    Ok(format!("{} LIKE ${}", field_sql, param_num))
}

// sql_gen.rs:284-290
WhereOperator::Endswith(field, suffix) => {
    // ...
    params.insert(param_num, Value::String(format!("%{}", suffix)));
    Ok(format!("{} LIKE ${}", field_sql, param_num))
}
```

The user-supplied string is passed unmodified into a LIKE pattern. In SQL, `_` is a
single-character wildcard and `%` is a multi-character wildcard. This means:

| User calls | Bound parameter | Actual SQL semantics |
|---|---|---|
| `Startswith(f, "order_id")` | `"order_id%"` | "starts with 'order' then one char then 'id'" |
| `Startswith(f, "50% off")` | `"50% off%"` | "starts with '50' then anything then ' off'" |
| `Endswith(f, "user_v2")` | `"%user_v2"` | "ends with 'user' then one char then 'v2'" |

The semantic contract `Startswith(field, prefix)` should mean "field starts with the
literal string `prefix`", but `_` and `%` in the prefix string act as wildcards.

**Note:** This is not a SQL injection vulnerability — parameters are bound safely. It is
a functional correctness issue: results returned by queries using these operators may
include rows that do not literally start/end with the given string.

**The `Icontains` operator is unaffected** — it uses SQL concatenation
(`'%' || $1::text || '%'`) which passes the user string as a literal substring to ILIKE,
where `_` and `%` still act as wildcards. Same underlying problem, same fix needed.

**Fix:**

Escape LIKE metacharacters in the string before embedding it in the LIKE parameter.
PostgreSQL supports a custom escape character via `LIKE pattern ESCAPE 'e'`:

```rust
fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('%', "\\%")
     .replace('_', "\\_")
}

WhereOperator::Startswith(field, prefix) => {
    let escaped = escape_like_pattern(prefix);
    params.insert(param_num, Value::String(format!("{}%", escaped)));
    Ok(format!("{} LIKE ${} ESCAPE '\\'", field_sql, param_num))
}
```

For `Icontains` with the `||` concatenation approach, escaping before binding is
the correct fix:

```rust
WhereOperator::Icontains(field, substring) => {
    params.insert(param_num, Value::String(escape_like_pattern(substring)));
    Ok(format!("{} ILIKE '%' || ${}::text || '%' ESCAPE '\\'", field_sql, param_num))
}
```

**Add regression tests** to the existing compliance suite:

```rust
#[test]
fn test_startswith_with_underscore_in_prefix() {
    // "order_id" as a literal prefix — should NOT match "orderXid_whatever"
    let op = WhereOperator::Startswith(Field::new("name"), "order_id".to_string());
    // ...verify escaped pattern "order\_id%" is bound
}
```

**Acceptance:**
- `WhereOperator::Startswith(f, "foo_bar")` produces a LIKE pattern that matches
  only strings starting literally with `"foo_bar"` (not `"foo-barXYZ"`)
- `WhereOperator::Startswith(f, "50% off")` produces a pattern that matches only
  strings starting literally with `"50% off"`
- All existing 35 operator compliance tests still pass

---

### J2 — `Like` and `Ilike` Operators Are Undocumented Raw Pattern Interfaces

**File:** `crates/fraiseql-wire/src/operators/where_operator.rs:101–105`

**Problem:**

The doc comments for `Like` and `Ilike` do not warn that the string argument is
passed as a raw SQL LIKE/ILIKE pattern, not a literal string:

```rust
/// LIKE pattern matching: `field LIKE pattern`
Like(Field, String),

/// Case-insensitive LIKE: `field ILIKE pattern`
Ilike(Field, String),
```

`%` and `_` in the argument act as SQL wildcards. Unlike `Startswith` (which promises
a prefix match), `Like` and `Ilike` intentionally expose raw pattern syntax — but this
is never stated. A user who passes a file path like `"/usr/local/bin"` through `Like`
to find an exact match will be surprised to discover `_` matches any character.

**Fix (documentation only — no behavior change):**

```rust
/// Raw LIKE pattern matching: `field LIKE pattern`.
///
/// The `pattern` argument is passed directly to SQL `LIKE`. The characters `%`
/// (matches any sequence) and `_` (matches exactly one character) act as SQL
/// wildcards. To match these characters literally, escape them: `\%`, `\_`.
///
/// For literal prefix/suffix/substring matches, prefer [`Startswith`],
/// [`Endswith`], or [`Icontains`], which escape wildcards automatically.
Like(Field, String),
```

Same update for `Ilike`.

**Acceptance:** `cargo doc -p fraiseql-wire` shows a `Like` doc that explains
the metacharacter behavior and links to the escaping alternatives.

---

## Track K — fraiseql-observers Reliability (Priority: Medium)

---

### K1 — NATS Transport ACKs Unparseable Messages (Silent Message Loss)

**File:** `crates/fraiseql-observers/src/transport/nats.rs:302–311`

**Problem:**

When a NATS message cannot be deserialized into an `EntityEvent`, the transport
ACKs the message and returns an error to the stream:

```rust
// nats.rs:302-311
Err(e) => {
    tracing::error!("Failed to parse NATS message: {}", e);
    // Acknowledge invalid message to prevent redelivery
    if let Err(ack_err) = msg.ack().await {
        tracing::error!("Failed to acknowledge invalid message: {}", ack_err);
    }
    Some(Err(e))
},
```

The intent (prevent infinite redelivery of poison pills) is sound. But the consequence
is that any message that cannot be deserialized — including messages sent from a newer
schema version with additional fields — is silently dropped without any dead-letter
queue or operator notification beyond a `tracing::error!` line.

In a multi-version deployment (rolling update scenario), messages from v2 consumers
can contain fields unknown to v1 consumers. With strict deserialization, v1 drops all
v2 messages, ACKs them, and the subscriber appears healthy. This is a version
compatibility trap.

**Fix:**

The immediate behavior (ACK to avoid infinite redelivery) should remain, but:

1. Increment a metric or emit a structured event that can trigger an alert:

```rust
Err(e) => {
    tracing::error!(
        error = %e,
        subject = %msg.subject,
        payload_len = msg.payload.len(),
        "Discarding unparseable NATS message (acknowledged to prevent redelivery)"
    );
    metrics::counter!("fraiseql_observer_nats_discard_total").increment(1);
    // ACK after logging for operator visibility
    if let Err(ack_err) = msg.ack().await { ... }
    Some(Err(e))
},
```

2. Document in the `NatsTransport` doc comment that deserialization failures cause
   message discard, and what schema compatibility guarantees callers should maintain.

3. Consider making this behavior configurable: a `discard_unparseable: bool` flag on
   `NatsTransportConfig` (default `true` for backward compat) that, when `false`, sends
   the message to a dead-letter subject instead of ACKing it.

**Acceptance:**
- Deserialization failure is logged with `subject` and `payload_len` fields
- The discard behavior is documented in `NatsTransport` module docs
- A `fraiseql_observer_nats_discard_total` counter (or equivalent) is incremented

---

### K2 — Bounded Channel `publish` Can Deadlock in Tests

**File:** `crates/fraiseql-observers/src/transport/in_memory.rs:95–101`
*(unstaged change — not yet committed)*

**Problem:**

The in-flight change switches `InMemoryTransport` from an unbounded to a bounded channel
(capacity 1 024). The `publish` method now `await`s if the channel is full:

```rust
// new code (unstaged)
self.sender.send(event.clone()).await.map_err(|e| ...)?;
```

Any test that publishes ≥ 1 024 events without draining between publishes will
deadlock. The previous unbounded channel never blocked. Existing tests that rely on
publish-then-drain patterns with large batches will silently hang rather than fail.

This is not a reason to revert the change (bounded channels are correct; unbounded
channels hide backpressure problems). But existing tests must be audited before
the change is committed.

**Fix:**

Before committing the in-memory change:

```bash
# Find all tests that use InMemoryTransport and publish without interleaved receives
grep -rn "InMemoryTransport\|in_memory" crates/ --include="*.rs" \
  | grep -v "^Binary\|/target/"
```

For each test:
- If it publishes more than 1 event without a receive, add an interleaved drain, or
- Use `InMemoryTransport::with_capacity(n)` with `n` larger than the batch size.

Also update the module doc to warn:

```rust
/// **Caution:** `publish` will await when the channel buffer is full. Tests that
/// publish many events without draining must either use a large capacity via
/// [`InMemoryTransport::with_capacity`] or interleave reads.
```

**Acceptance:**
- `cargo nextest run -p fraiseql-observers` passes without timeouts after the change
  is committed
- Module documentation warns about the backpressure behavior

---

## Track L — Validator Coherence Gap (Priority: Low)

---

### L1 — `AsyncValidatorProvider::ChecksumValidation` Has No Implementation

**File:** `crates/fraiseql-core/src/validation/async_validators.rs`
*(post-unstaged-change state)*

**Problem:**

After the unstaged change is applied, the `AsyncValidatorProvider` enum has three
variants:

```rust
pub enum AsyncValidatorProvider {
    EmailFormatCheck,     // ← implemented: EmailFormatValidator
    PhoneE164Check,       // ← implemented: PhoneE164Validator
    ChecksumValidation,   // ← no implementing struct anywhere
    Custom(String),
}
```

The public API exports `AsyncValidatorProvider::ChecksumValidation`, and
`checksum.rs` exports `LuhnValidator` and `Mod97Validator` — but neither implements
the `AsyncValidator` trait. There is no bridge between the provider enum and the
checksum validators.

A user reading the API would expect that `AsyncValidatorProvider::ChecksumValidation`
is backed by a concrete struct they can use via the `AsyncValidator` dispatch path.
The gap is silent: it will only surface when a user tries to wire up the checksum
provider and discovers no struct implements it.

**Fix:**

Option A (complete the bridge): Create a `ChecksumValidator` struct wrapping
`LuhnValidator` and `Mod97Validator` (selectable via config), implementing
`AsyncValidator`. Document which checksum algorithm each validation uses.

Option B (remove the variant): Remove `ChecksumValidation` from the enum until
the bridge is implemented. Update the checksum module to be a standalone utility
not connected to `AsyncValidator`.

Option C (label it unimplemented):

```rust
/// Checksum validation (Luhn, Mod 97).
///
/// **Not yet connected to an `AsyncValidator` implementation.**
/// Use [`LuhnValidator`] or [`Mod97Validator`] directly in the meantime.
ChecksumValidation,
```

**Acceptance:**
- Either a `ChecksumValidator` implementing `AsyncValidator` exists and is exported,
  or `ChecksumValidation` is removed from the public enum,
  or a doc comment clearly marks it as a placeholder.

---

## Summary of New Findings

| ID | Track | Severity | File | What |
|---|---|---|---|---|
| **I1** | Auth | **High** | `fraiseql-auth/src/jwt.rs:86` | JWT audience validation off by default |
| **I2** | Auth | Medium | `fraiseql-auth/src/jwt.rs:46` `rate_limiting.rs:105` | `eprintln!` in security-critical paths (extends G1 to fraiseql-auth) |
| **J1** | Wire | **Medium** | `fraiseql-wire/src/operators/sql_gen.rs:272,280,288,296` | LIKE metacharacter `_` `%` not escaped in Startswith/Endswith/Icontains |
| **J2** | Wire | Low | `fraiseql-wire/src/operators/where_operator.rs:101–105` | `Like`/`Ilike` docs don't warn about wildcard semantics |
| **K1** | Observers | Medium | `fraiseql-observers/src/transport/nats.rs:302–311` | Unparseable NATS messages ACKed and silently dropped |
| **K2** | Observers | Medium | `fraiseql-observers/src/transport/in_memory.rs` | Bounded channel can deadlock tests (in-flight change) |
| **L1** | Validators | Low | `fraiseql-core/src/validation/async_validators.rs` | `ChecksumValidation` variant has no implementing struct |

*K3 (`stop_health_monitor` no-op) is omitted — already tracked as Extension III — L1.*

---

## Execution Order

Integrate with the existing plans as follows:

### Before next release tag (alongside E1, E2, G1 from Extension 1):
- **I1** — Fix JWT audience validation default (1–2 hours)
- **I2** — Replace `eprintln!` in fraiseql-auth with tracing calls (30 minutes)

### Week 1 (alongside Track A from original plan):
- **J1** — Escape LIKE metacharacters in Startswith/Endswith/Icontains (2–3 hours)
- **J2** — Document Like/Ilike wildcard semantics (30 minutes, docs only)
- **K2** — Audit in-memory transport tests before committing bounded channel change (1 hour)

### Week 2 (alongside Track B):
- **K1** — Add metrics counter + documentation to NATS discard path (2 hours)

### Week 3 (alongside Track C and D):
- **L1** — Resolve ChecksumValidation coherence gap

---

## Definition of Done (Extension V)

In addition to the original plan's and Extensions I–IV's definitions of done:

1. `JwtValidator::new()` rejects tokens with wrong or missing audience by default
2. `grep "validate_aud = false" crates/fraiseql-auth/src/jwt.rs` → only in `allow_any_audience`
3. `grep -rn "eprintln!" crates/fraiseql-auth/src/ --include="*.rs"` → empty (outside test modules)
4. `WhereOperator::Startswith(f, "foo_bar")` matches only strings starting literally with `"foo_bar"`
5. `Like`/`Ilike` doc comments describe metacharacter behavior
6. NATS discard path logs `subject` and `payload_len`; increments a counter
7. `cargo nextest run -p fraiseql-observers` passes without timeouts after bounded channel commit
8. `AsyncValidatorProvider::ChecksumValidation` is either implemented, removed, or documented as placeholder
