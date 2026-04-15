# `app.mutation_response` v2

Contributor reference for the typed, versioned PostgreSQL composite that every
FraiseQL mutation function emits.

Authoritative design record: [ADR-0013](../adr/0013-mutation-response-v2-schema.md).
This document is the contributor-facing companion: DDL, semantics, mapping
tables, migration contract, and the call-site inventory that sizes the
cross-crate sweep.

---

## Design principles

1. **Orthogonal columns for orthogonal concerns.** Operation outcome, state
   change, error class, and error detail do not share a column.
2. **Typed classification, not string parsing.** `error_class` is a first-class
   PG enum. The Rust runtime reads it, never parses a prefix.
3. **Versioned shape.** `schema_version` is the first column. The composite
   evolves by bumping the version, not by silently changing column semantics.
4. **Builder-enforced invariants.** The `succeeded × state_changed × error_class`
   truth table is checked inside `core.build_mutation_response_v2`. Do not
   bypass the builder.
5. **Cross-database portability.** PG uses a native enum; MySQL / SQLite /
   SQL Server use `TEXT + CHECK` with the same value set. The Rust parser is
   unified across adapters.

---

## v2 DDL

```sql
CREATE TYPE app.mutation_error_class AS ENUM (
    'validation',
    'conflict',
    'not_found',
    'unauthorized',
    'forbidden',
    'internal',
    'transaction_failed',
    'timeout',
    'rate_limited',
    'service_unavailable'
);

CREATE TYPE app.mutation_response AS (
    schema_version  SMALLINT,                    -- always 2 for this shape
    succeeded       BOOLEAN,                     -- terminal outcome
    state_changed   BOOLEAN,                     -- did DB state actually change
    error_class     app.mutation_error_class,    -- NULL iff succeeded
    status_detail   TEXT,                        -- human-readable subtype
    http_status     SMALLINT,                    -- 100..=599
    message         TEXT,
    entity_id       UUID,
    entity_type     TEXT,
    entity          JSONB,                       -- always populated, incl. noops
    updated_fields  TEXT[],
    cascade         JSONB,
    error_detail    JSONB,                       -- structured error payload
    metadata        JSONB                        -- observability only
);
```

PG composite types do not support `CHECK` directly. The invariant below is
enforced by `core.build_mutation_response_v2`:

```sql
-- Enforced in the builder, not in DDL
(succeeded AND error_class IS NULL)
OR (NOT succeeded AND error_class IS NOT NULL AND NOT state_changed)
```

For non-PG adapters the same rule lives in a `CHECK` on the table/view that
emits the row.

---

## Column-by-column semantics

| Column           | Type                        | Meaning |
|------------------|-----------------------------|---------|
| `schema_version` | `SMALLINT`                  | `2` for this shape. Written every row; Rust parser dispatches on it. |
| `succeeded`      | `BOOLEAN`                   | Terminal outcome. `true` = operation completed (including noop). |
| `state_changed`  | `BOOLEAN`                   | `true` iff the database actually changed. Independent of `succeeded`. |
| `error_class`    | `app.mutation_error_class`  | `NULL` iff `succeeded`. Drives cascade code 1:1. |
| `status_detail`  | `TEXT`                      | Free-text subtype (e.g. `"duplicate_email"`, `"stale_revision"`). Not parsed. |
| `http_status`    | `SMALLINT`                  | 100..=599. First-class, not derived. Validated on ingest. |
| `message`        | `TEXT`                      | Human-readable summary. Safe to show to end users. |
| `entity_id`      | `UUID`                      | Primary key of the affected entity. Present for updates/deletes. |
| `entity_type`    | `TEXT`                      | GraphQL type name (e.g. `"User"`). Used by cache invalidation. |
| `entity`         | `JSONB`                     | Full entity payload. Populated even for noops (current row). |
| `updated_fields` | `TEXT[]`                    | GraphQL field names that changed. Empty on noop. |
| `cascade`        | `JSONB`                     | Cascade operations (see `graphql-cascade` spec). |
| `error_detail`   | `JSONB`                     | Structured error payload only (field, constraint, severity). |
| `metadata`       | `JSONB`                     | Observability only (trace IDs, timings, audit extras). |

`error_detail` and `metadata` are never merged. Consumers probe one or the
other. `entity` is never used as an error payload carrier.

---

## Semantics table

| `succeeded` | `state_changed` | `error_class` | meaning                                   |
|-------------|-----------------|---------------|-------------------------------------------|
| `true`      | `true`          | `NULL`        | create / update / delete applied          |
| `true`      | `false`         | `NULL`        | noop (idempotent call, state unchanged)   |
| `false`     | `false`         | non-null      | error — `error_class` drives cascade code |
| `false`     | `true`          | non-null      | **illegal** — rejected by the builder     |

Partial success is a separate pattern from "failed with state change." Per the
cascade spec, partial success is `succeeded=true + state_changed=true` with
non-critical entries in `error_detail`. A row with `succeeded=false` must not
have changed state — if it did, the mutation function has a transaction-
boundary bug that the response shape is not the right place to paper over.

### Noop

`noop:no_changes` in v1 is `succeeded=true, state_changed=false, entity={row}`
in v2. Idempotent deletes — which v1 could not cleanly express — are
`succeeded=true, state_changed=false` on a DELETE with no matching row.
Callers that only want "did anything happen" read `state_changed`; callers
that want current state read `entity`.

---

## `mutation_error_class` enum values

| Value                 | When to use |
|-----------------------|-------------|
| `validation`          | Input failed schema / business-rule validation. |
| `conflict`            | Uniqueness, optimistic-concurrency, or state conflict. |
| `not_found`           | Target entity does not exist (or caller cannot see it). |
| `unauthorized`        | Caller is unauthenticated. |
| `forbidden`           | Caller is authenticated but lacks permission. |
| `internal`            | Unhandled server-side failure. Do not leak details. |
| `transaction_failed`  | Transaction was rolled back (serialization, deadlock, explicit). |
| `timeout`             | Operation exceeded a deadline. |
| `rate_limited`        | Caller exceeded quota. |
| `service_unavailable` | Downstream dependency unreachable. |

### Extension policy

Adding a value requires:
1. ADR amendment to ADR-0013 recording the new value and its HTTP default.
2. `ALTER TYPE app.mutation_error_class ADD VALUE '<name>'` in a migration.
3. New arm in Rust `MutationErrorClass` + `CascadeErrorCode` mapping.
4. New arm in all SDK clients that project the classification.

Removing a value is an `ALTER TYPE ... RENAME VALUE` + full migration. Treat
the enum as append-only unless a release boundary makes full migration cheap.

---

## `MutationErrorClass` → `CascadeErrorCode` mapping

1:1. No fallbacks, no HTTP-code tiebreakers.

| `MutationErrorClass`  | `CascadeErrorCode`    |
|-----------------------|-----------------------|
| `Validation`          | `VALIDATION_ERROR`    |
| `Conflict`            | `CONFLICT`            |
| `NotFound`            | `NOT_FOUND`           |
| `Unauthorized`        | `UNAUTHORIZED`        |
| `Forbidden`           | `FORBIDDEN`           |
| `Internal`            | `INTERNAL_ERROR`      |
| `TransactionFailed`   | `TRANSACTION_FAILED`  |
| `Timeout`             | `TIMEOUT`             |
| `RateLimited`         | `RATE_LIMITED`        |
| `ServiceUnavailable`  | `SERVICE_UNAVAILABLE` |

### Default `http_status` per class

When a mutation function does not supply `http_status`, the builder applies:

| `error_class`         | Default |
|-----------------------|---------|
| `validation`          | 422 |
| `conflict`            | 409 |
| `not_found`           | 404 |
| `unauthorized`        | 401 |
| `forbidden`           | 403 |
| `internal`            | 500 |
| `transaction_failed`  | 500 |
| `timeout`             | 504 |
| `rate_limited`        | 429 |
| `service_unavailable` | 503 |

Success rows default to `200` (or `201` for creates, applied by the builder
via the `operation` parameter passed in).

---

## Rust struct (target shape for Phase 01)

```rust
/// v2 mutation response — typed, versioned.
///
/// Fields map 1:1 to `app.mutation_response` v2 columns.
pub struct MutationResponseV2 {
    pub schema_version: u16,
    pub succeeded:      bool,
    pub state_changed:  bool,
    pub error_class:    Option<MutationErrorClass>,
    pub status_detail:  Option<String>,
    pub http_status:    Option<i16>,  // matches PG SMALLINT; validated 100..=599
    pub message:        Option<String>,
    pub entity_id:      Option<uuid::Uuid>,
    pub entity_type:    Option<String>,
    pub entity:         serde_json::Value,
    pub updated_fields: Vec<String>,
    pub cascade:        serde_json::Value,
    pub error_detail:   serde_json::Value,
    pub metadata:       serde_json::Value,
}

pub enum MutationErrorClass {
    Validation,
    Conflict,
    NotFound,
    Unauthorized,
    Forbidden,
    Internal,
    TransactionFailed,
    Timeout,
    RateLimited,
    ServiceUnavailable,
}
```

`http_status` is validated on ingest: out-of-range values become a
`FraiseQLError::Validation` with the column name and observed value.

---

## Migration contract

### Coexistence strategy: parallel types

During the transition, v1 and v2 coexist as two separate composite types:

- `app.mutation_response_v1` — frozen v1 shape, used by not-yet-migrated functions.
- `app.mutation_response`    — new v2 shape, used by migrated functions.

`ALTER TYPE ... ADD ATTRIBUTE` was considered and rejected: v1 row semantics
do not translate column-by-column to v2 (the `status TEXT` prefix conflates
concerns that v2 splits), so mutating the existing type in place would leave
rows in an ambiguous middle state.

### Rust-side dispatch

`parse_mutation_row` becomes a version dispatcher:

```rust
pub fn parse_mutation_row(row: &Row) -> Result<MutationOutcome> {
    match row.get::<i16>("schema_version") {
        Ok(2) => parse_v2(row).map(Into::into),
        _     => parse_v1(row),   // existing prefix parser
    }
}
```

`MutationOutcome` (the existing seam in `runtime/executor/mutation.rs`) is
preserved so Phase 01 does not churn every consumer. v2-specific richness
(HTTP status, structured error detail, updated fields) is threaded through
extended fields on `MutationOutcome` variants.

### Retirement

Phase 04 removes the v1 path once all emitters are v2. The dispatcher
collapses to `parse_v2` only. `schema_version` stays — it signals to future
readers that this composite is principled about evolution, and it costs two
bytes per row.

### Legacy-mutation-v1 feature flag

Phase 04 ships in two releases:
1. First release: v1 parser is gated behind a `legacy-mutation-v1` Cargo
   feature (default: **on**). Downstream consumers still on v1 can opt out
   of the feature to prove they have migrated.
2. Following release: the feature and the v1 parser are deleted.

Do not collapse the two releases. The gap is the only early-warning signal
we have for out-of-tree v1 consumers.

---

## Call sites (blast-radius inventory)

Output of the Phase 00 grep-inventory. This sizes Phase 05.

### Parser core (Phase 01)

- `crates/fraiseql-core/src/runtime/mutation_result.rs` — `MutationOutcome`,
  `parse_mutation_row`, `is_error_status`, `populate_error_fields`. Becomes
  the version dispatcher.
- `crates/fraiseql-core/src/runtime/executor/mutation.rs:11,341,351,368,452,491`
  — primary consumer of `MutationOutcome`. Success/error arms project into
  GraphQL response.

No structural coupling in federation / cache / wire: the inventory found zero
uses of `MutationOutcome` or `parse_mutation_row` in
`crates/fraiseql-federation/`, `crates/fraiseql-core/src/cache/`, or
`crates/fraiseql-wire/`. Phase 01's `MutationOutcome`-as-seam design is
sufficient; no pre-Phase-01 escalation needed.

### Downstream consumers (Phase 05 sweep — FraiseQL-internal only)

**Server routes:**
- `crates/fraiseql-server/src/routes/grpc/handler.rs:391,431,446` —
  `encode_mutation_response` (protobuf encoder; post-parser; needs typed-
  outcome threading only if v2 fields surface on the wire).
- `crates/fraiseql-server/src/routes/grpc/mod.rs:333` — call-site.
- `crates/fraiseql-server/src/routes/rest/response.rs:413,415` — extracts
  entity from parsed outcome. No raw-composite coupling.
- `crates/fraiseql-server/src/routes/rest/handler.rs:1339` — raw-form
  pass-through for REST.
- `crates/fraiseql-server/src/routes/rest/bulk.rs` — consumes
  `mutation_result`.

**DB trait doc comments:**
- `crates/fraiseql-db/src/traits.rs:601,689` — references to
  `app.mutation_response` column set.
- `crates/fraiseql-db/src/dialect/capability.rs:16,36,62` —
  capability docs referencing mutation_response.

**Codegen:**
- `crates/fraiseql-cli/src/codegen/proto_gen.rs:133,197,199,240,483,737,742`
  — generates the protobuf `MutationResponse` message (the GraphQL-type /
  wire message, distinct from the PG composite; naming collision only).

**Integration tests (fixture rows):**
- `crates/fraiseql-server/tests/rest_transport_e2e_test.rs:29,32,43`
- `crates/fraiseql-server/tests/apq_mutation_e2e_test.rs:10,46`
- `crates/fraiseql-server/tests/grpc_transport_e2e_test.rs:203,204,205,252,269,739,773,774,775,817,818`
  (protobuf `MutationResponse`, not the PG composite)
- `crates/fraiseql-core/tests/pipeline_mutation_error_type_test.rs:6,194`
  (#294 regression on `is_error_status`)
- `crates/fraiseql-core/tests/mutation_typename_integration.rs:133`
- `crates/fraiseql-core/tests/mutation_nullability.rs:114,115`
- `crates/fraiseql-core/tests/federation/mutation_response.rs` (wire-shape
  tests; not the PG composite)
- `crates/fraiseql-core/tests/federation_mutation_http.rs:175`
- `crates/fraiseql-federation/src/mutation_http_client.rs:417,449` (HTTP
  response size; no structural coupling to v1 shape)

**Examples (SQL fixtures):**
- `examples/blog_api/` — 1 file
- `examples/cascade-create-post/` — 1 file
- `examples/ecommerce_api/` — ~22 files (types + functions)
- `examples/mutation-patterns/` — ~6 files
- `examples/real_time_chat/` — 1 file

**Docs:**
- `docs/adr/0013-mutation-response-v2-schema.md` (this initiative's ADR)
- `docs/database-compatibility.md`
- `docs/features/mutation-timing.md`

### Out of scope for this sweep

External consumers outside this repository (including any private
reference implementation). They migrate on their own schedule. The
`schema_version`-gated dispatcher in Phase 01 plus the
`legacy-mutation-v1` feature flag in Phase 04 give them a graceful path.

### Non-PG adapter parity

MySQL / SQLite / SQL Server continue to emit v1 until a follow-up
initiative lands the `TEXT + CHECK` equivalents. The unified Rust parser
reads typed columns on PG and falls back to v1 parsing on non-PG adapters
until that follow-up ships. Tracking issue to be opened at Phase 00
sign-off (see below).

---

## Pointer: printoptim reference implementation

The canonical PG helpers that emit v2 rows live in a private reference
repository (`core.build_mutation_response_v2`, `core.log_and_return_mutation_v2`,
`core.error_detail_*_v2`). FraiseQL's compiler and runtime do not depend on
them; Phases 02–03 of the plan describe that repository's migration for
context but are out of scope for this codebase.

---

## Open sign-off items for Phase 00

- [ ] Architecture doc (this file) + ADR-0013 reviewed by the user
- [ ] DDL compiled against a scratch PG instance
- [ ] Builder validation function demonstrably rejects the illegal combination
- [ ] Tracking issue opened for non-PG adapter parity (MySQL / SQLite / SQL Server)
- [ ] User sign-off before Phase 01 begins
