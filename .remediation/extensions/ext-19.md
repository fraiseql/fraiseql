# FraiseQL Remediation Plan — Extension XIX
## Observer Subsystem Correctness & SDK Contract Bugs

**Assessment date**: 2026-03-05
**Scope**: `fraiseql-observers` crate + Python authoring SDK
**Priority**: LOW–MEDIUM (operational correctness issues; no security impact)

---

### GG1 — Signed/unsigned checkpoint cast silently accepts negative values [LOW]

**File**: `crates/fraiseql-observers/src/listener/coordinator.rs:104–121`

```rust
/// Update listener checkpoint
pub fn update_checkpoint(&self, listener_id: &str, checkpoint: i64) -> Result<()> {
    // …
    handle.checkpoint.store(checkpoint as u64, Ordering::SeqCst);  // line 109
    Ok(())
}

/// (inside check_listener_health)
let checkpoint = handle.checkpoint.load(Ordering::SeqCst) as i64;  // line 121
```

The `checkpoint` field is an `AtomicU64` but the public API accepts `i64`. Negative
values (e.g., `-1`, a common sentinel) are silently stored as huge `u64` values
(`i64::MAX + 1` through `u64::MAX`) and round-trip correctly back to `i64` via
bit-reinterpretation. While the bits survive the trip, the design is confusing and
the API contract is wrong: checkpoint sequence numbers are intrinsically non-negative.

**Impact**: `FailoverEvent::checkpoint` (line 22) is declared `i64`. If a caller passes
`-1` to `update_checkpoint`, the `FailoverEvent` will surface `-1` in `last_checkpoint`,
which is then used to drive recovery position — an incorrect starting point.

**Fix**: Either:
- Change `AtomicU64` → `AtomicI64` throughout (preferred, matches the public contract),
- Or validate at the boundary:
  ```rust
  if checkpoint < 0 {
      return Err(ObserverError::InvalidConfig {
          message: format!("checkpoint must be non-negative, got {checkpoint}"),
      });
  }
  handle.checkpoint.store(checkpoint as u64, Ordering::SeqCst);
  ```

---

### GG2 — `stop_health_monitor` is a no-op; spawned monitor task leaks forever [LOW]

**File**: `crates/fraiseql-observers/src/listener/failover.rs:115–145`

```rust
pub async fn start_health_monitor(&self) -> mpsc::Receiver<FailoverEvent> {
    let (tx, rx) = mpsc::channel(100);
    let manager = self.clone();

    tokio::spawn(async move {
        loop {                                    // ← no exit condition
            tokio::time::sleep(…).await;
            if let Ok(failed_listeners) = manager.detect_failures().await {
                for failed_id in failed_listeners {
                    if let Ok(event) = manager.trigger_failover(&failed_id).await {
                        let _ = tx.send(event).await;  // ← error silently ignored
                    }
                }
            }
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

**Two problems**:

1. **`stop_health_monitor` does nothing.** It takes `&self` and has an empty body.
   The comment claims "receiver will be dropped" but the method cannot drop the receiver
   — it was returned to the caller by `start_health_monitor`. If the caller drops `rx`,
   the channel closes, but…

2. **The spawned task ignores the closed-channel error.** When `rx` is dropped,
   `tx.send(event).await` returns `Err(SendError(…))`. Because of `let _ = …`, the
   error is discarded and the loop continues. The task checks for failures, attempts to
   trigger failovers, and fires into a closed channel — indefinitely. There is no path
   that causes the task to exit except process termination.

**Impact**: Each call to `start_health_monitor` leaks a tokio task. In long-running
servers that reinitialise the observer subsystem (e.g., on configuration reload), tasks
accumulate. Each task also holds an `Arc<MultiListenerCoordinator>` keeping the
coordinator alive beyond its intended scope.

**Fix**: Return a `JoinHandle` and use a cancellation token:

```rust
use tokio_util::sync::CancellationToken;

pub async fn start_health_monitor(
    &self,
) -> (mpsc::Receiver<FailoverEvent>, CancellationToken) {
    let (tx, rx) = mpsc::channel(100);
    let token = CancellationToken::new();
    let manager = self.clone();
    let child_token = token.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = child_token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_millis(
                        manager.health_check_interval_ms)) => {}
            }
            if let Ok(failed_listeners) = manager.detect_failures().await {
                for failed_id in failed_listeners {
                    if let Ok(event) = manager.trigger_failover(&failed_id).await {
                        if tx.send(event).await.is_err() {
                            return; // receiver dropped — exit cleanly
                        }
                    }
                }
            }
        }
    });

    (rx, token)
}
```

Callers call `token.cancel()` to stop the monitor. This is a breaking API change to
`start_health_monitor` but `stop_health_monitor` was already a lie and needs to be
removed or reimplemented regardless.

---

### GG3 — `traceparent` parser accepts extra fields; stores them as `trace_state` [LOW]

**File**: `crates/fraiseql-observers/src/tracing/propagation.rs:86–117`

```rust
pub fn from_traceparent_header(header: &str) -> Option<Self> {
    let parts: Vec<&str> = header.split('-').collect();

    if parts.len() < 4 {   // ← accepts len > 4
        return None;
    }
    // …
    Some(Self {
        trace_id,
        span_id,
        trace_flags,
        trace_state: parts.get(4).map(|s| s.to_string()),  // ← wrong
    })
}
```

**Two spec violations** in this parser:

1. **Version "00" traceparent must have exactly 4 fields.** The W3C Trace Context spec
   (§3.2.2) states that an implementation receiving version "00" with more than 4 fields
   MUST treat it as invalid. The current code accepts any length ≥ 4.

2. **`trace_state` is not a component of `traceparent`.** The fifth dash-separated part
   of `traceparent` (if any) has no defined meaning under version "00"; it is
   **not** the `tracestate` header. The `tracestate` value is a separate HTTP header.
   Reading it from parts[4] is incorrect. In practice, `from_headers` (line 121–130)
   overwrites this with the real `tracestate` header, so the bug is masked when
   `from_headers` is used. However, `from_traceparent_header` is public and callers
   that invoke it directly receive incorrect `trace_state` values.

   *Note*: previous extensions flagged all-zero trace/span IDs in `Default` and W3C
   trace flag parsing. This finding (extra-field acceptance and misidentified trace_state
   source) is distinct and not covered there.

**Fix**:

```rust
pub fn from_traceparent_header(header: &str) -> Option<Self> {
    let parts: Vec<&str> = header.split('-').collect();

    // W3C spec §3.2: version "00" requires exactly 4 fields
    if parts.len() != 4 {
        return None;
    }
    let version = parts[0];
    if version != "00" {
        return None;
    }
    let trace_id = parts[1].to_string();
    let span_id  = parts[2].to_string();

    if trace_id.len() != 32 || !trace_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if span_id.len() != 16 || !span_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let trace_flags = u8::from_str_radix(parts[3], 16).ok()?;

    Some(Self {
        trace_id,
        span_id,
        trace_flags,
        trace_state: None,  // populated by from_headers() from the tracestate header
    })
}
```

---

### GG4 — Python SDK raises `TypeError` for a *value* constraint violation [LOW]

**File**: `sdks/official/fraiseql-python/src/fraiseql/decorators.py:334–341`

```python
# cache_ttl_seconds validation — fail fast at authoring time
if (ttl := cfg.get("cache_ttl_seconds")) is not None:
    if not isinstance(ttl, int) or ttl < 0:
        msg = (
            f"@fraiseql.query cache_ttl_seconds= on {f.__name__!r} must be a "
            f"non-negative integer (got {ttl!r})."
        )
        raise TypeError(msg)   # ← wrong exception type
```

The condition `ttl < 0` is a *value* constraint (the type is correct — it is an `int`),
not a *type* constraint. Python convention (and the built-in numeric types) uses
`ValueError` for out-of-range values and `TypeError` for wrong-type arguments.

Callers catching `ValueError` on schema validation (as is standard practice) will silently
miss this error when `cache_ttl_seconds=-1` is passed.

The same pattern occurs at mutation decorator line 504 (same validation block).

**Fix**:

```python
if not isinstance(ttl, int):
    raise TypeError(
        f"@fraiseql.query cache_ttl_seconds= on {f.__name__!r} must be an int "
        f"(got {type(ttl).__name__!r})."
    )
if ttl < 0:
    raise ValueError(
        f"@fraiseql.query cache_ttl_seconds= on {f.__name__!r} must be "
        f"non-negative (got {ttl!r})."
    )
```

Apply the same split to the mutation decorator.

---

### Summary

| ID | Severity | File | Lines | Issue |
|---|---|---|---|---|
| GG1 | LOW | `coordinator.rs` | 104–121 | `i64→u64` cast; negative checkpoints accepted silently |
| GG2 | LOW | `failover.rs` | 115–145 | Health monitor task never exits; `stop_health_monitor` is a no-op |
| GG3 | LOW | `propagation.rs` | 86–117 | traceparent parser accepts >4 fields; stores non-existent tracestate |
| GG4 | LOW | `decorators.py` | 334–341, 504 | `TypeError` raised for value constraint; should be `ValueError` |

**None of the above overlap with Extensions I–XVII.**
