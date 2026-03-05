# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension III

*Written 2026-03-05. Fourth assessor's findings.*
*Extends the three preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings verified against HEAD (latest commit: `140eea10c`).*
*Modified files confirmed: `async_validators.rs`, `validation/mod.rs`, `coordinator.rs`,*
*`failover.rs`, `propagation.rs`, `in_memory.rs`.*

---

## Executive Summary

The three previous assessors covered documentation accuracy, authentication bypass, stub
modules (backup, syslog), SQL injection in window queries, a frozen clock, and duplicate
metrics structs. This pass found a different stratum:

| Category | Count | Severity |
|---|---|---|
| Observer subsystem design flaws | 7 | High |
| Feature theater (new stubs) | 6 | High |
| API surface hygiene | 3 | Medium |

The most serious finding is the observer subsystem: the failover manager's health check
threshold field is stored and documented but never consulted, listener checkpoints silently
corrupt negative values, and the `Connecting` state has no recovery path — meaning a
connection failure in that state requires full restart.

---

## Track L — Observer Subsystem Design Flaws (Priority: High)

These issues are in the recently modified files (`coordinator.rs`, `failover.rs`) and
are new since the previous assessors wrote their reports.

---

### L1 — `stop_health_monitor` Is a No-op (High)

**File:** `crates/fraiseql-observers/src/listener/failover.rs:142–147`

**Problem:**

```rust
pub async fn start_health_monitor(&self) -> mpsc::Receiver<FailoverEvent> {
    let (tx, rx) = mpsc::channel(100);
    let manager = self.clone();

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(manager.health_check_interval_ms)).await;
            // ... detect and trigger failover
        }
    });

    rx
}

/// Stop health monitoring (by dropping receiver)
pub const fn stop_health_monitor(&self) {
    // Receiver will be dropped, causing channel to close
    // and health monitor task to end
}
```

`stop_health_monitor` is a `const fn` that does nothing. The comment explains a mechanism
that does not work: the receiver is returned to and owned by the *caller*, not by
`FailoverManager`. When the caller drops the receiver, the `tx.send(event).await` in the
spawned task receives `SendError` and the `let _ = tx.send(...)` silently ignores it —
**the task loops forever** regardless. There is no cancellation token, no `JoinHandle`,
no way to stop the background task.

**Consequence:** Calling `start_health_monitor()` without saving the returned receiver and
waiting for it forever leaks a task per call. In tests, this can accumulate indefinitely.

**Fix:**

Return a `tokio::task::JoinHandle` and a `CancellationToken` (from `tokio-util`):

```rust
use tokio_util::sync::CancellationToken;

pub fn start_health_monitor(&self) -> (mpsc::Receiver<FailoverEvent>, CancellationToken) {
    let (tx, rx) = mpsc::channel(100);
    let cancel = CancellationToken::new();
    let manager = self.clone();
    let cancel_child = cancel.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_child.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_millis(manager.health_check_interval_ms)) => {
                    // ... detect and trigger failover
                }
            }
        }
    });

    (rx, cancel)
}
```

Or, more simply:

```rust
pub fn start_health_monitor(&self) -> (mpsc::Receiver<FailoverEvent>, tokio::task::JoinHandle<()>) {
```

And update `stop_health_monitor` to take the handle:

```rust
pub fn stop_health_monitor(handle: tokio::task::JoinHandle<()>) {
    handle.abort();
}
```

**Acceptance:**
- `start_health_monitor()` returns a value that allows cancellation
- Calling stop causes the spawned task to exit
- No `FailoverManager` test leaks background tasks

---

### L2 — `failover_threshold_ms` Is Stored but Never Consulted (High)

**File:** `crates/fraiseql-observers/src/listener/failover.rs:31`
**File:** `crates/fraiseql-observers/src/listener/coordinator.rs:125`

**Problem:**

`FailoverManager` stores a configurable `failover_threshold_ms` (default: 60,000ms = 60s).
The user-facing documentation (`with_intervals`) implies this controls when a listener is
considered failed. But the actual health check is delegated to `coordinator.check_listener_health()`,
which hardcodes its own threshold:

```rust
// coordinator.rs:125 — never reads failover_threshold_ms
let is_healthy = state == ListenerState::Running && last_heartbeat.elapsed().as_secs() < 60;
```

The 60-second threshold is baked into the coordinator and is completely separate from
(and never informed by) the `failover_threshold_ms` field.

**Consequence:** A user calling:

```rust
FailoverManager::with_intervals(coordinator, 1000, 5000) // check every 1s, fail at 5s
```

...will find that listeners are still only considered failed after 60 seconds of silence,
regardless of the configured 5-second threshold.

**Fix — Option A (recommended):** Thread `failover_threshold_ms` into the health check:

```rust
// coordinator.rs: add threshold parameter to check_listener_health
pub async fn check_listener_health_with_threshold(
    &self,
    threshold_ms: u64,
) -> Result<Vec<ListenerHealth>> {
    // ...
    let is_healthy = state == ListenerState::Running
        && last_heartbeat.elapsed().as_millis() < u128::from(threshold_ms);
    // ...
}
```

And call it from `FailoverManager::detect_failures`:

```rust
let health = self.coordinator
    .check_listener_health_with_threshold(self.failover_threshold_ms)
    .await?;
```

**Fix — Option B:** Remove `failover_threshold_ms` from `FailoverManager` and document
that the coordinator controls health policy.

**Acceptance:**
- `FailoverManager::with_intervals(coordinator, 1000, 5000)` marks listeners unhealthy
  after 5 seconds of missed heartbeat (not 60)
- `failover_threshold_ms()` accessor returns the value that actually governs detection

---

### L3 — `elect_leader` Claims "Deterministic" on a Non-Ordered Map (Medium)

**File:** `crates/fraiseql-observers/src/listener/coordinator.rs:162–165`

**Problem:**

```rust
// Select first healthy listener (deterministic)
let new_leader = healthy[0].listener_id.clone();
```

`healthy` is collected from a `DashMap` iterator. `DashMap` provides no ordering guarantee
— the iteration order is hash-map-dependent and can vary between Rust versions, platforms,
and even between runs on the same machine due to hash randomization.

Calling `healthy[0]` returns "the first listener in whatever order DashMap happened to
iterate" — this is non-deterministic. In a high-availability setup, two coordinator
instances could elect different leaders, breaking the single-leader invariant.

**Fix:**

Sort `healthy` by `listener_id` (or registration timestamp) before selecting:

```rust
let mut healthy: Vec<_> = health.iter().filter(|h| h.is_healthy).collect();
healthy.sort_by(|a, b| a.listener_id.cmp(&b.listener_id)); // deterministic

if healthy.is_empty() { /* ... */ }
let new_leader = healthy[0].listener_id.clone();
```

**Acceptance:** `elect_leader()` doc comment says "by lowest listener_id" (or another
stable criterion), and the sort is present in the implementation.

---

### L4 — `update_checkpoint` Silently Corrupts Negative `i64` Values (High)

**File:** `crates/fraiseql-observers/src/listener/coordinator.rs:104–112`

**Problem:**

Checkpoints are declared as `i64` in the public API but stored internally as `AtomicU64`:

```rust
pub fn update_checkpoint(&self, listener_id: &str, checkpoint: i64) -> Result<()> {
    // ...
    handle.checkpoint.store(checkpoint as u64, Ordering::SeqCst); // ← lossy cast
    Ok(())
}

// In check_listener_health:
let checkpoint = handle.checkpoint.load(Ordering::SeqCst) as i64; // ← wrap-around read
```

If `checkpoint` is `-1` (a sentinel for "not started" in many PostgreSQL-based systems),
the cast `(-1i64) as u64 = u64::MAX`. When read back: `u64::MAX as i64 = -1`. This
roundtrips correctly only for negative values that happen to wrap-around exactly.

For `checkpoint = -2`: `(-2i64) as u64 = u64::MAX - 1`. Read back as i64: `-2`. Still OK.

However, for checkpoints above `i64::MAX` (which PostgreSQL can return as `int8` values
above `9_223_372_036_854_775_807`), the cast would lose the high bit. More critically,
the `AtomicU64` does not match the type contract — storing `i64::MIN` as `u64` produces
`9_223_372_036_854_775_808`, and comparisons elsewhere in the system expecting i64
semantics would produce wrong ordering.

**Fix:**

Use `AtomicI64` or store the raw bits with documented reasoning:

```rust
use std::sync::atomic::AtomicI64;

pub checkpoint: Arc<AtomicI64>,

// In update_checkpoint:
handle.checkpoint.store(checkpoint, Ordering::SeqCst);

// In check_listener_health:
let checkpoint = handle.checkpoint.load(Ordering::SeqCst);
```

If the intent is to avoid negative checkpoints, document that explicitly and `debug_assert`:

```rust
debug_assert!(checkpoint >= 0, "Checkpoint must be non-negative; got {checkpoint}");
```

**Acceptance:**
- `ListenerHandle::checkpoint` is `Arc<AtomicI64>` (or the type contract is clearly documented)
- Storing `-1` and reading it back via `check_listener_health` returns `-1`

---

### L5 — `Connecting` State Has No Recovery Path (High)

**File:** `crates/fraiseql-observers/src/listener/state.rs:134–152`

**Problem:**

The state machine transition matrix:

```
Initializing → Connecting
Initializing → Stopped
Connecting   → Running
Connecting   → Stopped      ← only exit from Connecting failure
Running      → Recovering
Running      → Stopped
Recovering   → Running
Recovering   → Stopped
```

`Connecting` cannot transition to `Recovering`. If a listener fails to establish its
database connection during the `Connecting` state, the only valid transition is to
`Stopped` — requiring a full listener restart to retry.

By contrast, `Running` can transition to `Recovering` (then back to `Running`). This
asymmetry means:

1. A transient connection failure during initial setup causes immediate termination
2. The same transient failure after initial connection allows transparent recovery

For high-availability scenarios (e.g., brief network partition during startup), this
forces hard restarts where a retry would suffice.

The module doc comment says:
> Tracks listener states: Initializing → Connecting → Running → Recovering

This implies `Connecting` is a peer of `Running` at the same level of stability, but
only `Running` has recovery support.

**Fix:**

Add the missing transition:

```rust
// Connection flow — with recovery
| (ListenerState::Connecting, ListenerState::Running)
| (ListenerState::Connecting, ListenerState::Recovering) // ← add this
| (ListenerState::Connecting, ListenerState::Stopped)
```

**Acceptance:**
- `state_machine.transition(Connecting → Recovering)` returns `Ok(())`
- Test: connect, fail (Connecting → Recovering → Connecting → Running) succeeds
- `max_recovery_attempts` is enforced from `Connecting` state as well as `Running`

---

### L6 — Non-Atomic State Transitions in `ListenerStateMachine` (Medium)

**File:** `crates/fraiseql-observers/src/listener/state.rs:45–49`

**Problem:**

The state machine uses three separate `Arc<Mutex<...>>` fields:

```rust
current_state:     Arc<Mutex<ListenerState>>,
state_change_time: Arc<Mutex<Instant>>,
recovery_attempts: Arc<Mutex<u32>>,
```

The `transition()` method acquires these locks in sequence. If two concurrent callers
call `transition()` simultaneously, an observer between the two lock acquisitions can
see `current_state` updated to `Running` but `recovery_attempts` still at the
pre-transition value (3), or `state_change_time` not yet updated.

In the failover use case, `check_listener_health` may see `state == Running` (from
`current_state`) while `recovery_attempts` still reflects the previous recovery session.

**Fix — Option A (recommended):** Combine all mutable state into a single `Arc<Mutex<StateMachineInner>>`:

```rust
struct StateMachineInner {
    state:              ListenerState,
    state_change_time:  Instant,
    recovery_attempts:  u32,
}

pub struct ListenerStateMachine {
    inner:                 Arc<Mutex<StateMachineInner>>,
    listener_id:           String,
    max_recovery_attempts: u32,
}
```

**Fix — Option B:** Document that snapshot consistency is not guaranteed and callers
must account for stale reads.

**Acceptance:** Either the state transitions are atomic across all three fields, or the
documentation explicitly states the consistency model.

---

## Track M — Feature Theater (New Stubs, Priority: High)

These are in addition to the stubs identified in Tracks F and J of the previous plans.

---

### M1 — Jaeger Exporter Never Sends HTTP (High)

**File:** `crates/fraiseql-observers/src/tracing/exporter.rs:205–225`

**Problem:**

`export_spans` buffers spans and triggers export when the batch is full. The export
function:

```rust
fn export_spans(config: &JaegerConfig, spans: Vec<JaegerSpan>) -> Result<()> {
    // In production, this would make actual HTTP request to Jaeger
    // For now, this is a placeholder that validates configuration

    for span in &spans {
        tracing::trace!(
            trace_id = %span.trace_id,
            operation = %span.operation_name,
            "Exported span to Jaeger"
        );
    }

    Ok(())
}
```

No HTTP request is made. No `reqwest` or `hyper` call exists in this file.
`record_span()` is a public API function — users calling it to send spans to Jaeger
will find their traces silently discarded.

The `JaegerConfig::endpoint` and `JaegerConfig::max_batch_size` fields are defined but
only the endpoint appears in a log line — neither controls actual network behavior.

**Fix:**

Implement with `reqwest`:

```rust
async fn export_spans(config: &JaegerConfig, spans: Vec<JaegerSpan>) -> Result<()> {
    if spans.is_empty() { return Ok(()); }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| Error::Tracing(format!("HTTP client error: {e}")))?;

    // Jaeger HTTP Collector API: POST /api/traces
    let payload = serde_json::json!({ "spans": spans });
    let resp = client.post(&config.endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|e| Error::Tracing(format!("Jaeger send failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(Error::Tracing(format!("Jaeger rejected spans: {}", resp.status())));
    }

    Ok(())
}
```

If implementation is deferred, label the feature as preview and do not export
`record_span` and `flush_spans` from the public API.

**Acceptance:**
- Either `export_spans` makes a real HTTP POST to the configured Jaeger endpoint, or
- The module is labeled `// Preview — not yet implemented` and removed from public re-exports

---

### M2 — Global `JAEGER_EXPORTER` Static Breaks Test Isolation (Medium)

**File:** `crates/fraiseql-observers/src/tracing/exporter.rs:104`

**Problem:**

```rust
static JAEGER_EXPORTER: Arc<Mutex<Option<JaegerExporter>>> = Arc::new(Mutex::new(None));
```

This is a process-wide singleton. Consequences:

1. Tests that call `init_jaeger_exporter()` contaminate other tests in the same process
   (the global is never reset between tests).
2. Two tests calling `init_jaeger_exporter()` concurrently will race — the second overwrites
   the first without warning.
3. In production, only one Jaeger configuration can exist per process. Applications with
   multiple `Server` instances (e.g., in integration tests) cannot use independent Jaeger
   configurations.

The test setup `fn init_test_exporter()` does not reset the global after the test completes,
silently breaking test ordering.

**Fix:**

Replace the global with an instance-held field on whatever struct orchestrates tracing
(the `Server` builder, `ObserverRuntime`, or similar). Pass the exporter through
dependency injection rather than a static.

If a global is necessary for tracing infrastructure, use `once_cell::sync::OnceCell`
and document the single-init-per-process constraint explicitly.

**Acceptance:**
- Tests calling `init_jaeger_exporter` do not affect each other's behavior
- `cargo nextest run --test-threads 4` passes consistently for the exporter tests

---

### M3 — `export_sdl_handler` and `export_json_handler` Return Hardcoded Examples (High)

**File:** `crates/fraiseql-server/src/routes/api/schema.rs:41–75`

**Problem:**

These two registered HTTP handlers return hardcoded example schemas, not the actual
compiled schema loaded by the server:

```rust
pub async fn export_sdl_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,  // ← _state intentionally ignored
) -> Result<Response, ApiError> {
    let schema_sdl = generate_example_sdl();  // ← hardcoded example
    Ok((StatusCode::OK, schema_sdl).into_response())
}
```

A user accessing `GET /api/schema/sdl` will receive a hardcoded example schema for a
`User` type that may have no relation to their actual schema. Since these routes are
registered in the production router, this is observable by any client.

The `state` parameter name uses leading underscore (`_state`) to silence the "unused
variable" warning — this is a code smell indicating intentional non-use of production data.

**Fix:**

Connect to the actual schema:

```rust
pub async fn export_sdl_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Response, ApiError> {
    let sdl = state.executor.schema().to_sdl()
        .map_err(|e| ApiError::internal(format!("SDL generation failed: {e}")))?;
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], sdl).into_response())
}
```

If `CompiledSchema` does not yet expose `to_sdl()`, either implement it or remove these
routes from the router until they can return real data.

**Acceptance:**
- `GET /api/schema/sdl` returns the SDL of the schema actually loaded by the server
- `_state` → `state` (the pattern of ignoring AppState is removed)
- `generate_example_sdl()` and `generate_example_json_schema()` are deleted

---

### M4 — `federation_health_handler` Always Returns Healthy with Empty Subgraphs (High)

**File:** `crates/fraiseql-server/src/routes/health.rs:131–143`

**Problem:**

```rust
pub async fn federation_health_handler() -> impl IntoResponse {
    let response = FederationHealthResponse {
        status:    "healthy".to_string(),
        subgraphs: vec![],                    // ← always empty
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let status_code = StatusCode::OK;          // ← always 200
    (status_code, Json(response))
}
```

This handler:
- Is registered in the production router
- Always returns HTTP 200 with `status: "healthy"`
- Never queries any subgraph
- Returns an empty subgraph list regardless of federation configuration

An operator monitoring `GET /health/federation` to detect subgraph failures will never
see a failure. This is a silent incorrect health signal.

**Fix (short-term):** Return HTTP 501 Not Implemented:

```rust
pub async fn federation_health_handler() -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({
        "error": "federation_health_not_implemented",
        "message": "Federation health check is not yet available. \
                    Check individual subgraph health endpoints directly."
    })))
}
```

**Fix (long-term):** Inject `SubgraphHealthChecker` into `AppState` and call it.

**Acceptance:**
- The handler never returns HTTP 200 with an empty subgraph list
- Either returns 501 (honest) or queries real subgraph health

---

### M5 — Observer Tenant/User Extraction Always Returns `None` (High)

**File:** `crates/fraiseql-observers/src/handlers.rs:419–480`

**Problem:**

Two functions in the observer handler are responsible for extracting tenant context and
user attribution for every observer mutation:

```rust
fn extract_customer_org_from_headers() -> Option<i64> {
    // ...
    None  // ← always None, every call site
}

fn extract_user_id_from_headers() -> Option<&'static str> {
    // ...
    None  // ← always None, every call site
}
```

These are called at lines 31, 117, 119, 158, 160 — every entity mutation handler in the
observer system. The result is:
- `customer_org: None` on every observer write → multi-tenancy RLS cannot apply
- `created_by: None` on every observer event → audit trail attribution is lost

This is the observer-layer analog of the E1 bug (GET handler drops security context)
found by the second assessor, but affects all observer operations rather than one route.

Note that the functions receive no parameters — they cannot access the Axum request
context even if implemented, because they are not Axum extractors.

**Fix:**

Convert to Axum extractors that accept the security context already extracted by middleware:

```rust
fn extract_customer_org(security_context: Option<&SecurityContext>) -> Option<i64> {
    security_context.and_then(|ctx| ctx.tenant_id)
}

fn extract_user_id(security_context: Option<&SecurityContext>) -> Option<&str> {
    security_context.map(|ctx| ctx.user_id.as_str())
}
```

Pass `security_context` (from `OptionalSecurityContext`) into each handler that calls these.

**Acceptance:**
- Observer mutations record `customer_org` from the authenticated user's tenant
- Observer mutations record `created_by` / `updated_by` from the authenticated user
- `extract_customer_org_from_headers()` and `extract_user_id_from_headers()` are deleted

---

### M6 — Seven Empty Config Structs Published in Public API (Medium)

**File:** `crates/fraiseql-server/src/config/mod.rs:269–328`

**Problem:**

Seven public config structs have no fields and are explicitly marked as placeholders:

```rust
pub struct NotificationsConfig {
    // Placeholder for notification system
}
pub struct LoggingConfig {
    // Placeholder for advanced logging configuration
}
pub struct SearchConfig {
    // Placeholder for search indexing support
}
pub struct CacheConfig {
    // Placeholder for advanced caching strategies
}
pub struct QueueConfig {
    // Placeholder for job queue support
}
pub struct RealtimeConfig {
    // Placeholder for real-time updates support
}
pub struct CustomEndpointsConfig {
    // Placeholder for custom endpoint support
}
```

These are referenced in the top-level `FraiseQLConfig` struct:

```rust
pub notifications:      Option<NotificationsConfig>,
pub logging:            Option<LoggingConfig>,
pub search:             Option<SearchConfig>,
// ...
```

They appear in `cargo doc` output as part of the public API. Users who inspect these
structs and configure them (by providing the relevant TOML sections) will find their
configuration silently accepted and ignored — none of these fields affect server behavior.

**Fix:**

Either implement or remove:

1. Remove the seven placeholder structs from `FraiseQLConfig`
2. Remove the corresponding fields from TOML config
3. Add a CHANGELOG entry: "These configuration sections are reserved for future releases"
4. If some are coming soon, add `#[doc(hidden)]` and a `// Coming in vX.Y` comment

**Acceptance:**
- No empty placeholder struct is part of the documented public API
- Users configuring `logging`, `search`, etc. in TOML receive a deserialization warning
  that the section is unrecognized (Serde's `deny_unknown_fields`)

---

## Track N — API Surface Hygiene (Priority: Medium)

---

### N1 — `testing::mocks` Exported in Both Test and Non-Test Builds (Medium)

**File:** `crates/fraiseql-webhooks/src/lib.rs:36–41`

**Problem:**

```rust
// Re-export testing mocks for tests
#[cfg(test)]
pub use testing::mocks;
// Also export mocks for integration tests (tests/ directory)
#[cfg(not(test))]
pub use testing::mocks;
```

`#[cfg(test)]` covers unit tests in the same crate. `#[cfg(not(test))]` covers everything
else, including integration tests (`tests/`) *and production binaries*. Together, these
two attributes cover every possible compilation mode — `testing::mocks` is always exported.

This means:
- Library users who do `use fraiseql_webhooks::mocks::MockClock` in production code
  compile successfully with no warning
- The `MockClock` and `MockSignatureVerifier` types become part of the published crate's
  public API surface (they would appear on docs.rs)
- Downstream crates that accidentally ship mock implementations in production have no
  compile-time guard

**Fix:**

Use `#[cfg(any(test, feature = "testing"))]` with a dedicated `testing` feature flag:

```rust
// In Cargo.toml:
[features]
testing = []

// In lib.rs:
#[cfg(any(test, feature = "testing"))]
pub use testing::mocks;
```

Integration tests in `tests/` should enable the `testing` feature in `Cargo.toml`:

```toml
# fraiseql-webhooks/Cargo.toml
[dev-dependencies]
# ... other deps

[[test]]
name = "signature_integration"
required-features = ["testing"]
```

**Acceptance:**
- `pub use testing::mocks` is not present in a `--features ""` build
- Integration tests compile by explicitly opting into `--features testing`
- `cargo doc -p fraiseql-webhooks` does not show `MockClock` or `MockSignatureVerifier`

---

### N2 — `reqwest::Client::build().unwrap_or_default()` Silently Drops Timeout (Medium)

**Files:**
- `crates/fraiseql-server/src/subscriptions/webhook_lifecycle.rs:39–42`
- `crates/fraiseql-core/src/federation/http_resolver.rs:67–70`
- `crates/fraiseql-core/src/runtime/subscription/webhook.rs:159–162`

**Problem:**

Three locations build a `reqwest::Client` with a configured timeout, then silently
fall back to the default client (no timeout) if the build fails:

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_millis(config.timeout_ms))
    .build()
    .unwrap_or_default();  // ← drops timeout silently on failure
```

If `reqwest::Client::builder().timeout(...).build()` fails (which can happen if the
TLS stack fails to initialize), `unwrap_or_default()` returns a plain `reqwest::Client`
with no timeout. Subsequent HTTP requests made with this client can hang indefinitely —
a DoS vector in high-availability scenarios where the client is used to call external
webhooks or federation subgraphs.

Compare with the *correct* pattern already used in `fraiseql-secrets/src/secrets_manager/backends/vault.rs`:

```rust
let client = reqwest::Client::builder()
    .build()
    .map_err(|e| SecretsError::ConnectionError(format!("HTTP client error: {e}")))?;
```

**Fix:**

Propagate the error:

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_millis(config.timeout_ms))
    .build()
    .map_err(|e| SomeError::Configuration(format!("HTTP client init failed: {e}")))?;
```

**Acceptance:**
- Zero `.unwrap_or_default()` calls on `reqwest::Client::builder().build()` in non-test code
- `cargo clippy` with `clippy::unwrap_or_default` catches future regressions

---

## Interaction with Existing Plans

| This plan | Existing plan |
|---|---|
| L1 (stop_health_monitor no-op) | Related to G5 (silent swallowed config); different module |
| L2 (failover_threshold_ms unused) | Related to J1 (cache_list_queries no-op); same pattern |
| L3 (non-deterministic elect_leader) | New; not covered by previous plans |
| L4 (checkpoint i64→u64 corruption) | New; not covered by previous plans |
| L5 (Connecting→Recovering missing) | New; different from state machine issues in prior plans |
| L6 (non-atomic state transitions) | Thematically related to I3 (frozen clock correctness) |
| M1 (Jaeger exporter stub) | Extends F2 (syslog stub); same pattern, different crate |
| M2 (global Jaeger singleton) | Extends K1 (duplicate MetricsCollector); observability layer |
| M3 (schema handlers return examples) | Extends F1 (backup stubs); high-visibility stub |
| M4 (federation_health always OK) | Related to E2 (RBAC unauthenticated); correctness in infra |
| M5 (observer tenant/user always None) | Extends E1 (GET handler drops auth); same root cause |
| M6 (empty config structs) | Extends F3 (dead backup providers); feature theater |
| N1 (mocks always exported) | Extends G6 (unused imports); API hygiene |
| N2 (reqwest unwrap_or_default) | Extends B2 (webhooks unwraps); same pattern, 3 more locations |

---

## Execution Order

### Immediate (before next release)

1. **M5** — Observer tenant/user always None: observer multi-tenancy is completely broken,
   all mutations are unattributed (1 day; same fix as E1)
2. **L4** — Checkpoint i64→u64 corruption: potential data corruption in checkpoint recovery
   (2 hours)
3. **M4** — Federation health always OK: silent false health signal (1 hour — return 501)

### Week 1

4. **L1** — stop_health_monitor no-op: task leak per monitor start (2 hours)
5. **L2** — failover_threshold_ms unused: configurable field that has no effect (2 hours)
6. **M1** — Jaeger stub: either implement or remove from public API (4 hours)
7. **N2** — reqwest unwrap_or_default: propagate errors, prevent silent timeout loss (1 hour)

### Week 2

8. **L3** — Non-deterministic leader election: add sort (30 minutes)
9. **L5** — Connecting→Recovering missing: add state transition (1 hour + tests)
10. **M3** — Schema handlers: connect to actual schema or return 501 (2 hours)
11. **N1** — mocks always exported: add `testing` feature flag (2 hours)

### Week 3+

12. **L6** — Non-atomic state transitions: consolidate Mutex (4 hours)
13. **M2** — Jaeger global singleton: DI refactor (4 hours)
14. **M6** — Empty config structs: remove from public API (2 hours)

---

## Definition of Done (Extension III)

In addition to the original plan's and Extensions I/II's definitions:

1. `start_health_monitor()` returns a cancellation handle; calling stop terminates the task
2. `FailoverManager::with_intervals(coord, 1000, 5000)` marks listeners unhealthy after 5s
3. `elect_leader()` selects by a stable, documented criterion (e.g., lowest `listener_id`)
4. `update_checkpoint(-1)` then `check_listener_health()` returns `last_checkpoint == -1`
5. `transition(Connecting → Recovering)` returns `Ok(())`
6. `GET /api/schema/sdl` returns the loaded schema's SDL (not a hardcoded User type)
7. `GET /health/federation` returns 501 or real subgraph health (never "healthy" with `subgraphs: []`)
8. Observer mutations record `customer_org` and `created_by` from the authenticated user
9. `pub use testing::mocks` does not appear in `--features ""` builds of `fraiseql-webhooks`
10. Zero `.unwrap_or_default()` on `reqwest::Client::builder().build()` in non-test code
11. `cargo doc -p fraiseql-webhooks` does not show `MockClock` or `MockSignatureVerifier`
12. No empty placeholder struct (body = only a comment) is part of the documented public API
