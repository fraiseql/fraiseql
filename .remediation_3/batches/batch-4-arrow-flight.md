# Batch 4 — Arrow Flight Completeness

## Problem

### Production path exposure

`service.rs:545` calls `execute_placeholder_query(view, limit)` on the `None`
branch of the database adapter option:

```rust
// flight_server/service.rs:545
let batches = match &self.db_adapter {
    Some(adapter) => adapter.query(sql).await?,
    None => execute_placeholder_query(view, limit),  // ← live service can reach this
};
```

If a production deployment misconfigures the server without a database adapter
(e.g., a bad feature flag combination), it silently returns placeholder data
instead of failing with an error. The function is not behind any `#[cfg(test)]`
or feature gate.

### Undocumented stubs

Three `Status::unimplemented()` returns exist in `handlers.rs` with messages
that give no context to the caller:

| Location | Current message |
|----------|----------------|
| `handlers.rs:226` | `"BulkExport not implemented yet"` |
| `handlers.rs:1087` | `"BulkExport not supported"` (inconsistent wording) |
| `handlers.rs:1130` | `"PollFlightInfo not implemented yet"` |

These messages do not name the version that will implement the feature, link to
any issue, or suggest a workaround. A client developer hitting this at runtime
has no path forward.

---

## Fix Plan

### AF-1 — Guard `execute_placeholder_query` from production

**Option A (recommended)**: Gate behind `#[cfg(any(test, feature = "testing"))]`.

In `crates/fraiseql-arrow/src/flight_server/convert.rs`:

```rust
#[cfg(any(test, feature = "testing"))]
pub(crate) fn execute_placeholder_query(
    view: &str,
    limit: usize,
) -> Vec<RecordBatch> {
    // existing implementation
}
```

In `crates/fraiseql-arrow/src/flight_server/service.rs`, replace the `None`
branch:

```rust
let batches = match &self.db_adapter {
    Some(adapter) => adapter.query(sql).await?,
    None => {
        #[cfg(any(test, feature = "testing"))]
        { execute_placeholder_query(view, limit) }
        #[cfg(not(any(test, feature = "testing")))]
        {
            return Err(Status::failed_precondition(
                "Arrow Flight server started without a database adapter. \
                 Configure a database adapter or enable the `testing` feature."
            ));
        }
    }
};
```

Add compile-time assertion to `lib.rs`:

```rust
// Ensure placeholder query is never available in release builds without explicit opt-in
#[cfg(all(not(test), not(feature = "testing"), feature = "placeholder-query-available"))]
compile_error!("placeholder-query-available must not be enabled in production builds");
```

### AF-2 & AF-3 — BulkExport stubs

**Decision point**: Implement or explicitly defer?

- If BulkExport is on the v2.1.0 roadmap: implement it.
- If BulkExport is not planned: replace the stub with a structured error that
  names the feature and directs users to the issue tracker.

**If deferring** (replace both stubs at `handlers.rs:226` and `handlers.rs:1087`):

```rust
return Err(Status::unimplemented(
    "BulkExport is not yet implemented in FraiseQL Arrow Flight. \
     Track progress at https://github.com/fraiseql/fraiseql/issues/XXX. \
     Use the standard do_get path with a query ticket as a workaround."
));
```

Both messages must be identical (currently they differ: `"not implemented yet"`
vs `"not supported"`).

**If implementing**: BulkExport requires:
1. A `BulkExportTicket` deserializer (table name, filter, column list)
2. A streaming query against the database adapter
3. Conversion of result rows to `RecordBatch` via existing `encode_json_to_arrow_batch`
4. Streaming the batches back as `FlightData`

### AF-4 — PollFlightInfo stub

`PollFlightInfo` is part of the Flight SQL 1.2 specification. It supports
long-running query polling. Replace `handlers.rs:1130` with:

```rust
// PollFlightInfo is part of Arrow Flight SQL 1.2. FraiseQL targets
// Arrow Flight SQL 1.1 for v2.x. This endpoint will be implemented in v3.0.
return Err(Status::unimplemented(
    "PollFlightInfo requires Arrow Flight SQL 1.2, which FraiseQL targets for v3.0. \
     Use synchronous do_get for queries expected to complete within the timeout."
));
```

### AF-5 — Integration tests

New file `crates/fraiseql-arrow/tests/flight_integration.rs`:

```rust
//! Integration tests for the Arrow Flight server.
//!
//! These tests use an in-process Flight server backed by a mock database
//! adapter and do not require a live database.

use fraiseql_arrow::flight_server::FraiseQLFlightServer;
use fraiseql_arrow::test_helpers::MockArrowAdapter;
use arrow_flight::flight_service_client::FlightServiceClient;
use tonic::transport::Channel;

async fn start_test_server() -> (FlightServiceClient<Channel>, impl Drop) {
    // Start in-process gRPC server with MockArrowAdapter
    todo!("implement test server setup")
}

#[tokio::test]
async fn test_do_get_returns_record_batch_for_valid_ticket() {
    let (mut client, _server) = start_test_server().await;
    // ... test do_get with a valid query ticket
}

#[tokio::test]
async fn test_do_get_returns_not_found_for_unknown_view() {
    let (mut client, _server) = start_test_server().await;
    // ... test do_get with an unknown view name
}

#[tokio::test]
async fn test_list_flights_returns_registered_views() {
    let (mut client, _server) = start_test_server().await;
    // ... test list_flights enumerates available views
}

#[tokio::test]
async fn test_bulk_export_returns_unimplemented() {
    let (mut client, _server) = start_test_server().await;
    // ... test that BulkExport returns a structured unimplemented error
}

#[tokio::test]
async fn test_no_database_adapter_returns_failed_precondition() {
    // Start server with no database adapter (not in testing feature mode)
    // Verify do_get returns FailedPrecondition, not placeholder data
    todo!()
}
```

---

## Verification

- [ ] `grep -r "execute_placeholder_query" crates/fraiseql-arrow/src/ | grep -v "#\[cfg"` returns nothing (no unguarded references in production code)
- [ ] BulkExport stub messages are identical at both call sites
- [ ] PollFlightInfo message names the target version
- [ ] `cargo build -p fraiseql-arrow` (no `testing` feature) succeeds and does not include placeholder code in the binary
- [ ] `cargo nextest run -p fraiseql-arrow --test flight_integration` passes
- [ ] `cargo clippy -p fraiseql-arrow --all-targets -- -D warnings` clean
