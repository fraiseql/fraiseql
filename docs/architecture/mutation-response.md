# `app.mutation_response`

Contributor reference for the typed PostgreSQL composite that every FraiseQL
mutation function emits.

Historical design record: [ADR-0013](../adr/0013-mutation-response-v2-schema.md)
(describes the original motivation for moving from string-status to typed columns).

---

## Design principles

1. **Orthogonal columns for orthogonal concerns.** Operation outcome, state
   change, error class, and error detail do not share a column.
2. **Typed classification, not string parsing.** `error_class` is a first-class
   PG enum. The Rust runtime reads it, never parses a prefix.
3. **Builder-enforced invariants.** The `succeeded × state_changed × error_class`
   truth table is checked inside `core.build_mutation_response`. Do not
   bypass the builder.
4. **Cross-database portability.** PG uses a native enum; MySQL / SQLite /
   SQL Server use `TEXT + CHECK` with the same value set. The Rust parser is
   unified across adapters.

---

## DDL

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
enforced by the shipped builders `fraiseql.mutation_ok` / `fraiseql.mutation_err`
(installed by `fraiseql setup`):

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

Idempotent calls are `succeeded=true, state_changed=false, entity={row}`.
Idempotent deletes with no matching row are `succeeded=true, state_changed=false`.
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

## Rust struct

```rust
/// Typed `app.mutation_response` row.
///
/// Fields map 1:1 to the PostgreSQL composite columns.
pub struct MutationResponse {
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
```

`http_status` is validated on ingest: out-of-range values become a
`FraiseQLError::Validation` with the column name and observed value.

Extra columns in the row (e.g. from older DB functions) are silently ignored
by the `serde` deserializer.

---

## Cascade (the typed cascade surface)

A mutation opts into the graphql-cascade surface with `cascade=True`
(`@fraiseql.type(crud=True, cascade=True)` or `@fraiseql.mutation(cascade=True)`).
The compiler then rewrites the mutation to return a **payload wrapper**
`<Name>Payload { entity, cascade, updatedFields }` — cascade lives on the payload,
not on the entity, so normalized client caches never store a `cascade` key against
an entity. A non-cascade mutation is unchanged and never surfaces cascade.

> **Eligibility: every cascade entity must expose `id: ID!`.** Cascade entities
> ride the `CascadeUpdates` envelope as the `CascadeNode` interface (`id: ID!`, per
> the graphql-cascade spec), auto-implemented on every queryable entity so it is
> selectable via an inline fragment. An entity whose GraphQL `id` is `UUID`/`Int`
> or absent cannot satisfy that interface, so `fraiseql compile` **fails fast** with
> one aggregated error naming each offending type — rather than emitting a schema
> the runtime would reject. Expose the identifier as GraphQL `ID` (FraiseQL
> serializes a UUID surrogate key as `ID`) to make a type cascade-eligible, or drop
> `cascade` from the mutations that return it. The identical `id: ID!` contract
> applies to Relay `Node` for `relay = true` types.

The function fills the `cascade` column with the spec-nested shape (see the
graphql-cascade spec): `{ updated: [{__typename, id, operation, entity}],
deleted: [{__typename, id, deletedAt}], invalidations: [...], metadata: {...} }`.
Author it with the shipped builders (installed by `fraiseql setup`):

```sql
v_cascade := fraiseql.build_cascade(
    p_updated := jsonb_build_array(
        fraiseql.cascade_entity('Post', v_post_id, 'CREATED', 'v_post'),
        fraiseql.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
    ),
    p_deleted := jsonb_build_array(
        fraiseql.deleted_entity('Comment', v_comment_id)
    )
);
```

At runtime FraiseQL projects each cascade entity to camelCase and runs the
field-level authorizer (#423) on it, exactly like a queried entity, and enforces
the response limits (`RuntimeConfig.cascade_limits`: max affected entities → the
cascade is truncated with `metadata.truncated`, max response size → rejected).

### Row-visibility boundary (RLS)

FraiseQL enforces **field-level** authorization on every cascade entity, but it
does **not** re-check row visibility on the wire — just as it does not for a
queried entity. Row visibility comes from RLS: `cascade_entity` reads each entity
from its **RLS-protected view** (`v_*`) on the mutation function's own connection,
whose session variables are pinned (the #329 fix), so a row the caller cannot see
is not returned by the view and never rides in the cascade — symmetric with a
query. The residual risk is an author bypassing that paved path — reading a base
table (`tb_*`) directly, or using `SECURITY DEFINER` — which no runtime check can
catch. **Always assemble cascade entities from the RLS views**, never base tables.

> **The view must be `security_invoker = true`** (PostgreSQL 15+, FraiseQL's
> standard view convention). A *default* view runs with the view owner's
> privileges and silently bypasses the caller's RLS — a cross-tenant leak the
> runtime cannot catch. `security_invoker` runs the view as the querying role, so
> the base-table policy applies. This is verified by the `cascade_rls_conformance`
> 2-tenant integration test: with a `security_invoker` view, tenant B's rows never
> ride in tenant A's cascade; with a default view, they leak.

---

## See also

[`change-log-contract.md`](./change-log-contract.md) — the Change Spine outbox
the executor writes in-transaction from this row (`entity_id` → `object_id`,
`entity` → `object_data`, `updated_fields`, `cascade`).
