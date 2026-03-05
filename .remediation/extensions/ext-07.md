# FraiseQL Remediation Plan — Extension 7

**Assessor:** 8th independent review
**Scope:** fraiseql-auth, fraiseql-observers/tracing — code paths not covered by Extensions 1–6
**Focus:** Auth endpoint correctness, constant-time API contract, W3C spec compliance
**Out of scope:** Benchmarks (handled by velocitybench)

---

## Track W — Authentication Endpoint Correctness

Three distinct correctness problems in `fraiseql-auth/src/handlers.rs` and `rate_limiting.rs`.
None of these appear in Extensions 1–6.

---

### W1 — `auth_refresh` does not validate session expiry before issuing a new token

**File:** `crates/fraiseql-auth/src/handlers.rs`, lines 278–314

**Observed behaviour:** The handler fetches the session from the store (line 279) and immediately
proceeds to rate-limit and issue a new token. It never calls `session.is_expired()`.

```rust
// handlers.rs:278-299 (simplified)
let session = state.session_store.get_session(&token_hash).await?;
// ← no expiry check here
if state.rate_limiters.auth_refresh.check(&session.user_id).is_err() { ... }
let access_token = format!("new_access_token_{}", uuid::Uuid::new_v4());
```

**Impact:** An attacker who captures a refresh token before its session expires can keep calling
`/auth/refresh` indefinitely after the session should have been invalidated. In-memory session
stores do not auto-evict, so sessions live forever unless explicitly revoked. Redis-backed stores
with TTLs would mitigate this at the storage layer, but the absence of an application-layer check
means the in-memory backend has no lifetime enforcement.

**Severity:** HIGH — session lifetime controls are silently bypassed.

**Fix:** Add `if session.is_expired() { return Err(AuthError::SessionExpired) }` immediately
after `get_session()`. The `Session` struct already exposes `is_expired()`, so this is a one-line
addition. Pair with a test that refreshes an expired session and asserts `401`.

---

### W2 — `auth_refresh` issues a UUID-prefixed string, not a real JWT

**File:** `crates/fraiseql-auth/src/handlers.rs`, line 299

**Observed behaviour:**

```rust
// Line 299
let access_token = format!("new_access_token_{}", uuid::Uuid::new_v4());
```

The `/auth/refresh` endpoint returns `"new_access_token_<uuid>"` as the `access_token` field.
This string is not a JWT; it has no signature, no claims, and no expiry. Any downstream service
that validates the token will reject it. The endpoint documents itself as returning a "Refresh
access token", implying a real JWT with a bounded lifetime.

**Impact:** Feature Theater in a security-critical path. The entire token refresh flow produces
unusable output. Callers that check the token type header will fail immediately; callers that
trust the opaque token will experience silent downstream auth failures.

**Severity:** HIGH — the feature is advertised as complete but is entirely non-functional.

**Fix:** Replace the placeholder with actual JWT generation using the `JwtValidator::generate`
infrastructure that already exists in `jwt.rs`. The generated token must be signed, include
`sub`/`iat`/`exp`/`iss`/`aud` claims, and have a bounded `expires_in`.

---

### W3 — Rate limiter disables itself (fails open) on system clock failure

**File:** `crates/fraiseql-auth/src/rate_limiting.rs`, lines 97–113 and 191

**Observed behaviour:** When `SystemTime::now()` fails, the private `system_clock()` function
returns `u64::MAX`. In `KeyedRateLimiter::check()`:

```rust
// rate_limiting.rs:191
if now >= record.window_start.saturating_add(self.config.window_secs) {
    // CASE 1: Window has expired → start new window, allow request
    record.count = 1;
    record.window_start = now;
    Ok(())
}
```

Because `saturating_add` can produce at most `u64::MAX`, the condition
`u64::MAX >= u64::MAX` is always true. Every request resets the window and is unconditionally
allowed through. The inline comment (line 101–104) describes this as intentional "fail-safe to
allow requests during time issues", but for a brute-force protection component, fail-open
is the wrong safety model.

Compare with `jwt.rs:53`, where the same `u64::MAX` value correctly causes tokens to be treated
as expired (fail-safe, correct direction for authentication). The two modules disagree on which
direction "safe" is.

**Impact:** During any period of system clock instability — even brief NTP resync jumps — the
`failed_login_attempts` limiter is fully disabled. An attacker who can trigger or predict clock
events gains unlimited login attempts.

**Severity:** HIGH — brute-force protection is silently disabled on clock failures.

**Mitigation options (in ascending strictness):**
1. Log + return a fixed, non-MAX sentinel (e.g. `0`) that keeps the current window active rather
   than resetting it. This is the least disruptive fix.
2. Return `Err(AuthError::InternalError)` from `check()` when the clock fails, causing the
   request to be rejected. This is fail-closed and matches the JWT module's intent.
3. Document the fail-open choice explicitly in the `RateLimitConfig` as an opt-in field
   (`fail_open_on_clock_error: bool`) so callers can choose.

The `with_clock` constructor already makes this testable (the doc comment on line 129 even shows
`|| u64::MAX` as the test fixture), so a regression test can be added immediately.

---

## Track X — Constant-Time Comparison API Contract Violation

### X1 — `compare_padded` silently truncates to 1024 bytes when `fixed_len > 1024`

**File:** `crates/fraiseql-auth/src/constant_time.rs`, lines 85–113

**Observed behaviour:**

```rust
pub fn compare_padded(expected: &[u8], actual: &[u8], fixed_len: usize) -> bool {
    let mut expected_padded = [0u8; 1024];
    let mut actual_padded   = [0u8; 1024];

    let pad_len = fixed_len.min(1024);  // ← silently caps
    // ...
    expected_padded[..pad_len].ct_eq(&actual_padded[..pad_len]).into()
}
```

The function's documented contract is: *"Always compares at `fixed_len` bytes, padding with
zeros if necessary."* When `fixed_len > 1024`, only 1024 bytes are compared. Two inputs that
differ only beyond byte 1024 will compare as equal.

The test that exercises this code path (`test_compare_padded_exceeds_max_buffer`, line 307)
uses `b"test"` for both inputs — they are identical regardless of the buffer cap — and
asserts success. The test comment even acknowledges the truncation: *"Should still work, capping
at 1024"*. No test checks the case where inputs differ in the bytes-1025–2048 range.

**Impact:** Any caller passing `fixed_len > 1024` gets a comparison that only covers the first
1024 bytes. Because `compare_jwt_constant` hardcodes 512 this is safe in the current call sites,
but the public API makes no such guarantee. A future caller using `compare_padded` for a
token type larger than 1024 bytes (e.g. a long opaque token or a custom claim string) could
silently accept forged values whose prefix matches.

**Severity:** MEDIUM — latent security vulnerability; safe today, footgun for future callers.

**Fix options:**
1. **Preferred:** Replace the fixed `[0u8; 1024]` with `vec![0u8; fixed_len]`. Heap allocation
   during security comparison is acceptable given the threat model (network I/O already dominates).
2. **Alternative:** `debug_assert!(fixed_len <= 1024)` plus a doc note stating the 1024-byte
   limitation explicitly — this makes the contract honest without changing behaviour.

The existing test `test_compare_padded_exceeds_max_buffer` must be updated to also assert the
_negative_ case: two tokens that agree in bytes 0–1023 but differ in byte 1024+ must compare
as not-equal after the fix.

---

## Track Y — W3C Trace Context Compliance

Two independent violations of the W3C Trace Context specification (https://www.w3.org/TR/trace-context/)
found in `crates/fraiseql-observers/src/tracing/propagation.rs`.

---

### Y1 — `TraceContext::Default` generates all-zero IDs, which the W3C spec forbids

**File:** `crates/fraiseql-observers/src/tracing/propagation.rs`, lines 155–163

**Observed behaviour:**

```rust
impl Default for TraceContext {
    fn default() -> Self {
        Self {
            trace_id:    "0".repeat(32),  // "00000000000000000000000000000000"
            span_id:     "0".repeat(16),  // "0000000000000000"
            trace_flags: 0x00,
            trace_state: None,
        }
    }
}
```

**W3C spec §3.2.2.2 (trace-id):**
> *"All bytes as zero (`00000000000000000000000000000000`) is considered invalid."*

**W3C spec §3.2.2.3 (parent-id / span-id):**
> *"All bytes as zero (`0000000000000000`) is considered invalid."*

The `Default` implementation actively produces an object that the spec declares invalid. If this
value is serialised into a `traceparent` header and forwarded, a strict tracing backend (Jaeger,
Zipkin, Honeycomb, OpenTelemetry Collector in strict mode) will reject or discard the span.

**Severity:** MEDIUM — correct execution in production requires that no `Default` context is ever
propagated. This is an implicit invariant with no runtime guard.

**Fix:** Remove the `Default` impl (letting the compiler enforce explicit construction) or
replace the all-zero IDs with a valid UUID v4-derived trace ID and span ID following the same
pattern as `child_span_id()`. Add a test asserting that no propagated header contains all-zero
fields.

---

### Y2 — `from_traceparent_header` misreads tracestate as a 5th traceparent segment

**File:** `crates/fraiseql-observers/src/tracing/propagation.rs`, lines 86–117

**Observed behaviour:**

```rust
pub fn from_traceparent_header(header: &str) -> Option<Self> {
    let parts: Vec<&str> = header.split('-').collect();
    // ...
    Some(Self {
        trace_id,
        span_id,
        trace_flags,
        trace_state: parts.get(4).map(|s| s.to_string()),  // ← line 116
    })
}
```

This extracts a 5th `-`-delimited field from the `traceparent` value and stores it as
`trace_state`. There are two separate problems:

**Problem A — W3C spec violation for version 00.** Spec §3.3:
> *"If the `version` field is `0x00`, the header `traceparent` MUST NOT contain more than four
> fields. Parsers MUST reject a `traceparent` header with version `0x00` if there are more than
> four fields."*

The current parser accepts and partially interprets such headers instead of rejecting them.

**Problem B — Wrong field.** The `tracestate` value is carried in a completely separate HTTP
header (`tracestate:`, not a segment of `traceparent:`). The `from_headers` function correctly
reads it from the separate header (lines 121–130). `from_traceparent_header` should never touch
`trace_state`; callers who need it must use `from_headers`.

**Test that asserts the wrong behaviour:**

```rust
// test at line 266–274
fn test_from_traceparent_header_with_tracestate() {
    let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-vendor=value";
    let ctx = TraceContext::from_traceparent_header(header);
    assert!(ctx.is_some());  // ← should be assert!(ctx.is_none()) per spec
    let ctx = ctx.unwrap();
    assert_eq!(ctx.trace_state, Some("vendor=value".to_string()));  // ← entirely wrong
}
```

**Impact:** Any tracing backend following the spec strictly will drop spans carrying a
`traceparent` built from a permissive parse. Conversely, a legitimate `traceparent` whose
trace-flags happen to look like `01-something` (e.g. from a future version or a middleware
that adds a proprietary field) will have a silently corrupted `trace_state`.

**Severity:** MEDIUM — spec violation with observable correctness impact in mixed-vendor
tracing pipelines.

**Fix:**
1. In `from_traceparent_header`: reject headers where `parts.len() != 4` when `version == "00"`.
   Remove the `trace_state` extraction from this function entirely; set it to `None`.
2. Update the affected test to assert `ctx.is_none()` for the 5-field input.
3. Add a test confirming that `from_headers` correctly populates `trace_state` from the separate
   `tracestate` header (one already exists at line 276; verify it exercises the round-trip).

---

## Summary Table

| ID | File | Severity | One-liner |
|----|------|----------|-----------|
| W1 | `handlers.rs:279` | HIGH | `auth_refresh` never checks `session.is_expired()` |
| W2 | `handlers.rs:299` | HIGH | `auth_refresh` returns `"new_access_token_<uuid>"` not a JWT |
| W3 | `rate_limiting.rs:111` | HIGH | clock failure disables brute-force rate limits (fail-open) |
| X1 | `constant_time.rs:94` | MEDIUM | `compare_padded` silently caps comparison at 1024 bytes |
| Y1 | `propagation.rs:158` | MEDIUM | `TraceContext::Default` produces W3C-invalid all-zero IDs |
| Y2 | `propagation.rs:116` | MEDIUM | tracestate erroneously parsed from traceparent body |

Six issues total. None overlap with Extensions 1–6.

---

## Relationship to Existing Plans

- Extensions 1–6 covered GET-handler auth bypass (Track E), unauthenticated RBAC (Track E),
  JWT audience disabled by default (Extension 5 Track I), and `eprintln` in auth code (Track G).
- This extension targets different code paths: the refresh endpoint, the rate limiter's failure
  mode, the padding function's undocumented cap, and the tracing propagation parser.
- The `eprintln` calls at `rate_limiting.rs:105` and `jwt.rs:46` should be converted to
  structured `tracing::error!` calls as part of Track G remediation from Extension 2; that work
  is not duplicated here.
