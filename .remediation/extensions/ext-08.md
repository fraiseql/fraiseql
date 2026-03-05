# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension 8

*Written 2026-03-05. Ninth independent assessor.*
*Extends all seven preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*

---

## Context and Methodology

This assessment read all seven existing plans fully, then performed targeted deep-reads on
four areas that previous assessors left unexplored: the `fraiseql-arrow` Flight service
(authentication was covered in Extension VI, but SQL paths were not), the `fraiseql-wire`
SCRAM implementation and TCP configuration, and the `fraiseql-auth` OAuth2 client flow.

| Category | Count | Severity |
|---|---|---|
| SQL injection via Arrow Flight API | 3 | Critical |
| OAuth2 PKCE/CSRF design flaws | 2 | High |
| Silent config no-op (wire keepalive) | 1 | Medium |
| Missing validator implementation | 1 | Medium |
| SCRAM username escaping | 1 | Medium |
| W3C spec violation in default trace context | 1 | Low |

---

## Track Q — Arrow Flight SQL Injection (Priority: Critical)

These three bugs are all in `crates/fraiseql-arrow/src/flight_server/` and share a common
root cause: the Arrow Flight API accepts client-controlled strings and interpolates them
directly into SQL. Unlike the window-query injection identified in Extension II (which is in
`fraiseql-core`), these are in `fraiseql-arrow` and come from the Arrow Flight ticket
deserialization path.

---

### Q1 — `OptimizedView` filter/order_by Are Interpolated Unvalidated into SQL (Critical)

**Files:**
- `crates/fraiseql-arrow/src/flight_server/convert.rs:106–131` (`build_optimized_sql`)
- `crates/fraiseql-arrow/src/flight_server/service.rs:501–523` (`execute_optimized_view`)
- `crates/fraiseql-arrow/src/ticket.rs:70–83` (`FlightTicket::OptimizedView`)

**Problem:**

`FlightTicket::OptimizedView` has `filter: Option<String>` and `order_by: Option<String>`
fields. The ticket is deserialized from client-supplied JSON. Both fields are passed unchanged
to `build_optimized_sql`, which interpolates them directly:

```rust
// convert.rs:115–120
if let Some(where_clause) = filter {
    sql.push_str(&format!(" WHERE {where_clause}"));  // ← user-controlled
}
if let Some(order_clause) = order_by {
    sql.push_str(&format!(" ORDER BY {order_clause}"));  // ← user-controlled
}
```

A malicious authenticated client can send:
```json
{
  "type": "OptimizedView",
  "view": "va_orders",
  "filter": "1=1; DROP TABLE orders; --"
}
```

The `view` name goes through `quote_identifier` (which correctly double-quotes it), but
`filter` and `order_by` do not receive any treatment.

**Severity:** Critical — any authenticated Flight client can execute arbitrary SQL.

**Fix:**

For `order_by`: validate that the value matches an allowlist of column names from the
view's Arrow schema (which is already loaded from `schema_registry` in `execute_optimized_view`).

For `filter`: reject the string approach entirely. Replace with a structured filter type:

```rust
// In ticket.rs
pub enum FilterCondition {
    Eq { column: String, value: serde_json::Value },
    Gt { column: String, value: serde_json::Value },
    Lt { column: String, value: serde_json::Value },
    IsNull { column: String },
    // ...
}
```

Or, as a minimal patch that preserves backward compatibility, validate `filter` against
a strict allowlist: digits, comparison operators, column names matching `[a-zA-Z_][a-zA-Z0-9_]*`,
and quoted string literals with escape checking. Reject anything else with
`Status::invalid_argument`.

**Acceptance:** A test sending `filter: "1=1; DROP TABLE t; --"` must return `invalid_argument`.

---

### Q2 — `BulkExport` table Name and filter Interpolated Without Quoting (Critical)

**File:** `crates/fraiseql-arrow/src/flight_server/service.rs:783–815` (`execute_bulk_export`)

**Problem:**

```rust
// service.rs:810–814
let mut sql = format!("SELECT * FROM {}", table);  // ← unquoted user-controlled table name

if let Some(f) = &filter {
    sql.push_str(" WHERE ");
    sql.push_str(f);  // ← unvalidated user-controlled filter
}
```

Unlike the INSERT path (`build_insert_query`), which correctly applies `quote_identifier`
to the table name (identified in `convert.rs:488–489`), the bulk-export path does not.
A client can send:

```json
{
  "type": "BulkExport",
  "table": "users; DROP TABLE users; --",
  "format": "parquet"
}
```

Both the `table` and `filter` fields come directly from `FlightTicket::BulkExport`, which
is deserialized from client-supplied JSON.

**Fix:**

Apply `quote_identifier` to `table` and validate `table` against an allowlist of exported
table names registered in `schema_registry`. Apply the same structured filter approach
described in Q1 to `filter`.

Minimal patch for `table`:
```rust
// Replace line 810
let mut sql = format!("SELECT * FROM {}", crate::flight_server::convert::quote_identifier_pub(table));
```

(Currently `quote_identifier` is `fn` not `pub fn` — make it accessible or duplicate the
logic; quoting alone raises the bar, but schema-registry allowlisting is the correct fix.)

---

### Q3 — `BatchedQueries` Ticket Executes Arbitrary Client-Supplied SQL (Critical)

**Files:**
- `crates/fraiseql-arrow/src/ticket.rs:109–128` (`FlightTicket::BatchedQueries`)
- `crates/fraiseql-arrow/src/flight_server/service.rs:620–699` (`execute_batched_queries`)

**Problem:**

The `BatchedQueries` ticket type accepts a `queries: Vec<String>` field containing raw SQL
strings. The server executes each string directly against the database adapter without any
parsing, validation, or restriction:

```rust
// service.rs:654–661
if let Some(db) = &self.db_adapter {
    db.execute_raw_query(query)  // ← query is raw client SQL
        .await
        .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
}
```

The ticket documentation includes:
```json
{
  "type": "BatchedQueries",
  "queries": [
    "SELECT * FROM ta_users LIMIT 100",
    "UPDATE users SET role='admin' WHERE 1=1"
  ]
}
```

Any authenticated Flight client (one that successfully completed OIDC handshake) can
execute arbitrary DDL and DML. Row-Level Security applies at the PostgreSQL level but
does not prevent DDL (DROP, ALTER, CREATE) if the service role has those privileges.
This undermines the entire "compile-time SQL" security model of FraiseQL.

**Severity:** Critical — this is an architectural flaw. The `BatchedQueries` ticket type,
as designed, is fundamentally incompatible with the project's security model.

**Fix (architectural):** Remove the `BatchedQueries` ticket type entirely. If bulk retrieval
is needed, replace it with:
- Multiple `OptimizedView` tickets (one per view, each with Q1 fixes applied)
- Or a `NamedQueryBatch { query_names: Vec<String> }` that references pre-compiled named
  queries from the schema registry only

**Fix (minimum viable):** Add a validation step before execution:
```rust
// Reject anything that is not a plain SELECT against an allowlisted view
fn is_safe_query(query: &str, schema_registry: &SchemaRegistry) -> bool {
    let q = query.trim().to_lowercase();
    if !q.starts_with("select ") { return false; }
    // parse FROM clause and check against schema_registry.list_views()
    // ...
}
```

**Acceptance:** A test sending `queries: ["DROP TABLE users"]` must return `invalid_argument`
or `permission_denied`.

---

## Track R — OAuth2 Client PKCE/CSRF Design Flaws (Priority: High)

---

### R1 — `OAuth2Client::authorization_url` Ignores `use_pkce` Flag (High)

**File:** `crates/fraiseql-auth/src/oauth.rs:233–254`

**Problem:**

The `OAuth2Client` has a `use_pkce: bool` field and a `with_pkce(enabled: bool)` builder
method. The field is stored, but `authorization_url` never consults it:

```rust
// oauth.rs:233–254
pub fn with_pkce(mut self, enabled: bool) -> Self {
    self.use_pkce = enabled;  // stored...
    self
}

pub fn authorization_url(&self, redirect_uri: &str) -> Result<String, String> {
    let state = uuid::Uuid::new_v4().to_string();
    let scope = self.scopes.join(" ");

    // use_pkce is never checked here
    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        // ...
    );
    // no code_challenge, no code_challenge_method
    Ok(url)
}
```

The `PKCEChallenge` struct is correctly implemented in the same file (lines 655–699) with
proper `S256` SHA-256 hashing. It is simply never called from `authorization_url`.

**Impact:** Callers who use `OAuth2Client::new(...).with_pkce(true)` receive no PKCE
protection. Public clients (mobile apps, SPAs) are vulnerable to authorization code
interception attacks.

**Fix:** When `use_pkce` is true, generate a `PKCEChallenge` and include it in the URL.
The return type must change to carry both the URL and the `code_verifier` for later use
in `exchange_code`:

```rust
pub fn authorization_url(&self, redirect_uri: &str) -> Result<(String, Option<PKCEChallenge>), String> {
    let state = uuid::Uuid::new_v4().to_string();
    let scope = self.scopes.join(" ");

    let pkce = if self.use_pkce {
        Some(PKCEChallenge::new())
    } else {
        None
    };

    let mut url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        // ...
    );

    if let Some(ref challenge) = pkce {
        url.push_str(&format!("&code_challenge={}&code_challenge_method={}",
            urlencoding::encode(&challenge.code_challenge),
            urlencoding::encode(&challenge.code_challenge_method),
        ));
    }

    Ok((url, pkce))
}
```

`exchange_code` should be updated to accept an optional `code_verifier`:
```rust
pub async fn exchange_code(
    &self,
    code: &str,
    redirect_uri: &str,
    code_verifier: Option<&str>,
) -> Result<TokenResponse, String>
```

---

### R2 — `OAuth2Client::authorization_url` Discards the OAuth State (High)

**File:** `crates/fraiseql-auth/src/oauth.rs:240–254`

**Problem:**

The `state` parameter generated in `authorization_url` is included in the redirect URL
to the authorization server, but is never returned to the caller. The caller has no
way to know what `state` value was sent, and therefore cannot verify it when the
authorization server returns in the callback.

```rust
pub fn authorization_url(&self, redirect_uri: &str) -> Result<String, String> {
    let state = uuid::Uuid::new_v4().to_string();  // generated...

    let url = format!(
        "...&state={}",
        urlencoding::encode(&state),  // included in URL...
    );

    Ok(url)  // ← state not returned; caller cannot verify callback
}
```

**Impact:** Any redirect from a legitimate authorization server back to the application
is accepted, regardless of whether the application initiated the request. An attacker can
craft a callback URL with a forged authorization code and bypass state verification.

**Fix:** Change the return type to expose the state:

```rust
pub struct AuthorizationRequest {
    pub url:   String,
    pub state: String,
    pub pkce:  Option<PKCEChallenge>,  // from R1
}

pub fn authorization_url(&self, redirect_uri: &str) -> Result<AuthorizationRequest, String> {
    let state = uuid::Uuid::new_v4().to_string();
    // ...
    Ok(AuthorizationRequest { url, state, pkce })
}
```

The caller is then responsible for storing `state` (e.g., in an encrypted session cookie)
and verifying it against the `state` parameter received in the callback.

**Note:** R1 and R2 should be fixed in the same commit since both require a return-type
change to `authorization_url`. Fixing R2 alone without R1 (or vice versa) would require
two breaking changes; do both at once.

---

## Track S — fraiseql-wire Silent No-ops (Priority: Medium)

---

### S1 — `keepalive_idle` Is Stored but Never Applied to the TCP Socket (Medium)

**Files:**
- `crates/fraiseql-wire/src/connection/conn.rs:40, 130–198` (field + builder)
- `crates/fraiseql-wire/src/connection/transport.rs:78` (`TcpStream::connect`)

**Problem:**

`ConnectionConfig` has a documented `keepalive_idle: Option<Duration>` field with a
fluent builder method and example in the docs:

```rust
// conn.rs:185–194 (builder)
/// Set TCP keepalive idle interval
/// Default: None (no keepalive)
pub fn keepalive_idle(mut self, duration: Duration) -> Self {
    self.keepalive_idle = Some(duration);
    self
}
```

The field is stored in `ConnectionConfig` (confirmed at line 230: `keepalive_idle: self.keepalive_idle`),
but the transport layer creates the TCP stream via a bare `TcpStream::connect`:

```rust
// transport.rs:78
pub async fn connect_tcp(host: &str, port: u16) -> Result<Self> {
    let stream = TcpStream::connect((host, port)).await?;  // ← no socket options applied
    Ok(Transport::Tcp(TcpVariant::Plain(stream)))
}
```

There is no `socket2` dependency in `fraiseql-wire/Cargo.toml`, so `SO_KEEPALIVE` cannot
be set. The `startup` method, which receives `config`, also never reads `config.keepalive_idle`.

**Impact:** Users who configure keepalive via `ConnectionConfigBuilder::keepalive_idle()`
receive no keepalive probes. Long-idle connections to PostgreSQL will be silently dropped by
firewalls/NAT without the client detecting it.

**Fix:** Add `socket2` as a dependency and apply the option before the socket connects:

```toml
# fraiseql-wire/Cargo.toml
socket2 = { version = "0.5", features = ["all"] }
```

```rust
// transport.rs
pub async fn connect_tcp_with_options(
    host: &str,
    port: u16,
    keepalive_idle: Option<std::time::Duration>,
) -> Result<Self> {
    use socket2::{Domain, Protocol, Socket, Type, TcpKeepalive};

    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

    if let Some(idle) = keepalive_idle {
        let ka = TcpKeepalive::new().with_time(idle);
        socket.set_tcp_keepalive(&ka)?;
    }

    let addr: std::net::SocketAddr = tokio::net::lookup_host((host, port))
        .await?.next()
        .ok_or_else(|| Error::Config("DNS resolution failed".into()))?;

    socket.connect(&addr.into())?;
    socket.set_nonblocking(true)?;

    let stream = TcpStream::from_std(socket.into())?;
    Ok(Transport::Tcp(TcpVariant::Plain(stream)))
}
```

Alternatively, use `tokio::net::TcpSocket` which exposes `set_keepalive` on nightly
but is not stable. The `socket2` approach is the production-ready path.

**Acceptance:** A test that sets `keepalive_idle(Duration::from_secs(1))` and inspects
the socket option via `getsockopt(SO_KEEPALIVE)` must return `true`.

---

## Track T — Validator and SCRAM Implementation Gaps (Priority: Medium)

---

### T1 — `AsyncValidatorProvider::ChecksumValidation` Has No Concrete Implementation (Medium)

**File:** `crates/fraiseql-core/src/validation/async_validators.rs:46, 57`

**Problem:**

`AsyncValidatorProvider` has three variants. Two have corresponding structs implementing
`AsyncValidator`: `EmailFormatCheck` → `EmailFormatValidator`, `PhoneE164Check` →
`PhoneE164Validator`. The third, `ChecksumValidation`, has no concrete implementation:

```rust
pub enum AsyncValidatorProvider {
    EmailFormatCheck,    // → EmailFormatValidator (implemented)
    PhoneE164Check,      // → PhoneE164Validator (implemented)
    ChecksumValidation,  // → ??? (no struct, no implementation)
    Custom(String),      // → caller-supplied (external)
}
```

If code creates an `AsyncValidatorConfig` with `ChecksumValidation` as provider and
attempts to dispatch it, there is no handler. The type system does not prevent this
configuration from being created and serialized/deserialized (via `serde`).

The `checksum` module (`src/validation/checksum.rs`) contains `LuhnValidator` and
`Mod97Validator` that implement `Validator` (the synchronous trait), but neither
implements `AsyncValidator`.

**Impact:** Any runtime dispatch system that matches on `AsyncValidatorProvider` will
encounter an unhandled case. A `match provider { ChecksumValidation => ??? }` will
either need an `unreachable!()` or a runtime error.

**Fix:** Either:

a) Add `ChecksumAsyncValidator` that wraps `LuhnValidator`/`Mod97Validator` with
   an `async fn validate_async(value, field)` implementation, or

b) Remove `ChecksumValidation` from `AsyncValidatorProvider` and document that checksum
   validators are synchronous (use `ValidationRule::Luhn` / `ValidationRule::Mod97` directly).

Option (b) is simpler and more correct since checksum validation does not require
async I/O.

---

### T2 — SCRAM Username Not Escaped Per RFC 5802 §5.1 (Medium)

**File:** `crates/fraiseql-wire/src/auth/scram.rs:73–78, 110–111`

**Problem:**

RFC 5802 §5.1 requires that usernames containing `,` or `=` must be escaped:
- `,` → `=2C`
- `=` → `=3D`

The SCRAM client inserts the raw username directly into the message:

```rust
// scram.rs:78
format!("n,,n={},r={}", self.username, self.nonce)
```

A username such as `alice,role=admin` would produce:
```
n,,n=alice,role=admin,r=<nonce>
```

This breaks the SCRAM message format because `,` is the field separator. A PostgreSQL
server that follows RFC 5802 strictly would reject authentication for usernames containing
these characters. More subtly, the `client_final_bare` used in HMAC authentication message
construction would also be malformed:

```rust
// scram.rs:111
let client_first_bare = format!("n={},r={}", self.username, self.nonce);
```

**Impact:** Postgres usernames containing `,` or `=` cannot authenticate via this client.
While `=` and `,` are unusual in Postgres usernames, `=` appears in common patterns like
`service=account` or `user=readonly`.

**Fix:**

```rust
fn sasl_prep_username(username: &str) -> String {
    username.replace('=', "=3D").replace(',', "=2C")
}

pub fn client_first(&self) -> String {
    let escaped = sasl_prep_username(&self.username);
    format!("n,,n={},r={}", escaped, self.nonce)
}
```

Apply the same escaping in `client_final` at line 111.

Add a test:
```rust
#[test]
fn test_username_with_special_chars() {
    let client = ScramClient::new("user,role=admin".to_string(), "pass".to_string());
    let first = client.client_first();
    // username comma and equals must be escaped
    assert!(first.contains("n=user=2Crole=3Dadmin,"));
}
```

---

## Track U — W3C Spec Compliance (Priority: Low)

---

### U1 — `TraceContext::default()` Produces an Explicitly Invalid W3C traceparent (Low)

**File:** `crates/fraiseql-observers/src/tracing/propagation.rs:155–163`

**Problem:**

The `Default` implementation creates a context with all-zero trace and span IDs:

```rust
impl Default for TraceContext {
    fn default() -> Self {
        Self {
            trace_id:    "0".repeat(32),  // 32 zeros
            span_id:     "0".repeat(16),  // 16 zeros
            trace_flags: 0x00,
            trace_state: None,
        }
    }
}
```

The W3C Trace Context specification (§2.2.3) explicitly states:

> "Implementations MUST ignore the `traceparent` header if the `trace-id` has all-zero bytes."

Similarly for `parent-id` (span-id):
> "Implementations MUST ignore the `traceparent` header if the `parent-id` has all-zero bytes."

When `TraceContext::default()` is used and its `to_traceparent_header()` is called, any
downstream service that follows the spec will discard the trace context entirely, breaking
distributed trace continuity.

**Impact:** Low — this only affects "no context" situations. But any code that creates a
default context and passes it through is silently violating the spec.

**Fix:** Either:

a) Remove the `Default` implementation entirely and require explicit construction via
   `TraceContext::new(...)` or a `root()` constructor that generates valid random IDs.

b) Change `default()` to generate a new random root context:
```rust
impl Default for TraceContext {
    fn default() -> Self {
        Self::new_root()
    }
}

impl TraceContext {
    /// Create a new root trace context with random IDs.
    pub fn new_root() -> Self {
        let uuid_bytes = *uuid::Uuid::new_v4().as_bytes();
        let trace_id = format!("{:032x}", u128::from_be_bytes(*uuid_bytes));
        // Generate a second UUID for span_id
        let span_bytes = *uuid::Uuid::new_v4().as_bytes();
        let span_id = format!("{:016x}",
            u64::from_be_bytes([
                span_bytes[0], span_bytes[1], span_bytes[2], span_bytes[3],
                span_bytes[4], span_bytes[5], span_bytes[6], span_bytes[7],
            ])
        );
        Self {
            trace_id,
            span_id,
            trace_flags: 0x01,  // sampled by default
            trace_state: None,
        }
    }
}
```

c) If `Default` is used only as a "null object" (the current intent), rename it to make
   the invalidity explicit:
```rust
pub const INVALID_TRACE_CONTEXT: TraceContext = TraceContext {
    trace_id:    /* 32 zeros at compile time */
    ...
};
```

---

## Implementation Priority

| ID | Finding | Severity | File | Effort |
|----|---------|----------|------|--------|
| Q1 | Arrow OptimizedView filter/order_by SQL injection | Critical | `flight_server/convert.rs` | Medium |
| Q2 | Arrow BulkExport table/filter SQL injection | Critical | `flight_server/service.rs` | Small |
| Q3 | BatchedQueries executes arbitrary client SQL | Critical | `flight_server/service.rs` | Large (architectural) |
| R1 | OAuth2 PKCE flag silently ignored | High | `fraiseql-auth/src/oauth.rs` | Small |
| R2 | OAuth2 state discarded — CSRF mitigation broken | High | `fraiseql-auth/src/oauth.rs` | Small |
| S1 | `keepalive_idle` not applied to TCP socket | Medium | `fraiseql-wire` | Medium |
| T1 | `ChecksumValidation` enum variant with no implementation | Medium | `fraiseql-core/src/validation/` | Small |
| T2 | SCRAM username not escaped per RFC 5802 | Medium | `fraiseql-wire/src/auth/scram.rs` | Small |
| U1 | `TraceContext::default()` produces W3C-invalid all-zero IDs | Low | `fraiseql-observers/src/tracing/` | Small |

**Recommended order:** Q2 → Q1 → R1+R2 (one commit) → T2 → T1 → S1 → Q3 → U1.

Q3 requires an API change and should be discussed with the team before implementation.
Q2 is a one-line fix (add `quote_identifier`) and should be the first commit.
R1 and R2 both require a return-type change to `authorization_url`; fix them together.
