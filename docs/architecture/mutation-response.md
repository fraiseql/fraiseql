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
4. **Native enum classification.** `error_class` is a native PostgreSQL enum,
   so an out-of-set value is rejected by the database itself, not by
   application-level parsing.

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
| `entity_type`    | `TEXT`                      | The GraphQL type the outcome is served as — on success the entity's type (e.g. `"User"`, also used by cache invalidation), on failure the declared error type (e.g. `"DuplicateEmailError"`). See *The type an outcome is served as*. |
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

### The type an outcome is served as

`entity_type` must name a type the outcome can be, derived from the mutation's return type:

* **Success**: a non-error member of the returned union, an implementor of the returned
  interface, or the returned type itself.
* **Failure**: an error (`is_error`) member of the returned union, an error implementor of
  the returned interface, or — when the mutation returns a plain object type — any error
  type the schema declares.

A stamp outside that set is a contract error, and the mutation's write is rolled back. A
`NULL` stamp is accepted where only one type is possible: `fraiseql.mutation_ok(entity)`
for a single success type, `fraiseql.mutation_err('conflict')` for a union with a single
error member (every `auto_error_union` result). Where there are several, the function must
say which one it produced:

```sql
RETURN QUERY SELECT * FROM fraiseql.mutation_err(
    'conflict', 'Email already registered', p_entity_type => 'DuplicateEmailError');
```

An unstamped failure of a mutation returning a plain object type has no member to choose
between; it is served untyped (`__typename` of the return type, plus `status`).

### A constraint the function violates

When the function raises an integrity-constraint violation (SQLSTATE class 23) instead of
returning a row, the write rolls back with its transaction. If the mutation returns a union
or interface with exactly one error member (every `auto_error_union` result), the violation
is served as that member, as an unstamped `mutation_err` would be, so a constraint does not
need a pre-check in the function to reach the typed error:

| SQLSTATE | `status` / `errorClass` | `httpStatus` | `message` |
|---|---|---|---|
| `23502` not-null, `23514` check | `validation` | 422 | `The request contains an invalid value` |
| any other `23xxx` (unique, exclusion, foreign key, …) | `conflict` | 409 | `The request conflicts with the current state of the data` |

The message is always that generic text; the server log records the database's error. The
member names the constraint in one `errors[]` entry, the shape a function's own
`mutation_err_entries` gives (#1531), so a client can act on it without the function
pre-checking:

```json
{ "__typename": "MutationError", "status": "conflict", "httpStatus": 409,
  "errors": [{ "code": 409, "identifier": "tb_user_email_live_key",
               "message": "The request conflicts with the current state of the data",
               "details": { "sqlstate": "23505" } }] }
```

`identifier` is the constraint's name: a unique **index**'s name for a partial unique index
(it has no `pg_constraint` row), and for a not-null violation, which names only its column,
the not-null constraint PostgreSQL 18 catalogues for that column (`<table>_<column>_not_null`
unless named). Name constraints in `snake_case` and the identifier is a translation key too.
The server key `mutation_constraint_metadata` sets how much is said:

| Value | Entry |
|---|---|
| `"identifier"` (default) | `identifier`, `code`, `message`, `details.sqlstate` |
| `"full"` | also `details.table` and `details.columns` (the constraint's key columns, from `pg_constraint` or `pg_index`; omitted for an expression index rather than guessed) |
| `"none"` | no entry; `status`, `httpStatus` and `message` only |

Never the database's `DETAIL` and never a value, at any setting: they carry the row. A
foreign-key violation (`23503`) is `conflict` / 409 in both directions: PostgreSQL 18 names the
**referencing** table and no column whether the parent is missing or still referenced, so the
direction is not recoverable from anything structured (its localized message is the only
difference, and `lc_messages` is superuser-only).

The member must declare a field to carry the entry: the synthesized `MutationError` has
`errors` (JSON). A declared error type adds `errors` the same way. REST does not reach this path
(it mounts no route for a union-returning mutation), MCP cannot call one (#1546), and gRPC's
`MutationResponse` carries `success`, `id` and `error` only.

A mutation with no error member, or with several, keeps the top-level
`CONSTRAINT_VIOLATION` error, since nothing says which member a violation is.

A literal stamp outside the set is caught before it ships: `fraiseql compile --database`,
`fraiseql validate --against-db` and `fraiseql doctor --against-db` read the function body
and fail on it ([database contract validation](../guides/database-contract-validation.md)).
At runtime every contract error is logged at `warn` with the mutation and its function, and
counted in `fraiseql_mutation_contract_errors_total`.

### Noop

Idempotent calls are `succeeded=true, state_changed=false, entity={row}`.
Idempotent deletes with no matching row are `succeeded=true, state_changed=false`.
Callers that only want "did anything happen" read `state_changed`; callers
that want current state read `entity`.

### Error entries: `error_detail.errors[]`

A failure that a client must explain to a person carries its reasons as a list of entries
under `error_detail.errors`. Each entry has this shape:

| Key          | Type       | Meaning |
|--------------|------------|---------|
| `code`       | `SMALLINT` | A numeric code for the reason (an HTTP-like status, or the application's own). |
| `identifier` | `TEXT`     | The key a client translates, e.g. `t('errors.' + identifier)`. Matches `^[a-z][a-z0-9_]*$`. |
| `message`    | `TEXT`     | A fallback text for when no translation exists. |
| `details`    | `JSONB`    | Optional: the values the translation interpolates (a field, a limit, an id). |

Build entries with `fraiseql.error_entry` and return them with `fraiseql.mutation_err_entries`
(both installed by `fraiseql setup`):

```sql
RETURN QUERY SELECT * FROM fraiseql.mutation_err_entries(
    'validation', 'The order cannot be placed',
    fraiseql.error_entry(422::smallint, format('%s_not_found', 'Order line'), 'No such line',
                         jsonb_build_object('line', p_line_id)),
    fraiseql.error_entry(422::smallint, 'quantityTooLow', 'Quantity too low'));
-- error_detail = {"errors": [
--   {"code": 422, "identifier": "order_line_not_found", "message": "No such line",
--    "details": {"line": "…"}},
--   {"code": 422, "identifier": "quantity_too_low", "message": "Quantity too low"}]}
```

`error_entry` normalises the identifier, so one built from a human label or a type name is
still a key: accents removed (`Événement` → `evenement`), camelCase split (`PaymentTerm` →
`payment_term`), and every run of other characters one `_` (`Order line` → `order_line`).
An identifier that normalises to nothing of that shape (empty, only punctuation, starting
with a digit) raises SQLSTATE `22023`, so the mistake surfaces in the function, not in a
client's missing translation. `fraiseql.error_identifier(text)` exposes the normalisation
alone. The helpers need a UTF8 database (for `normalize`), and no extension.

To find the failures that do not follow this shape, set the server key

```toml
mutation_error_shape_check = "warn"   # default "off"
```

Each failed mutation response whose `error_detail` carries no `errors` array (or an empty
one), or an entry whose `identifier` is not a key, is then logged at `warn` (with the
mutation, its function, and what is wrong) and counted in
`fraiseql_mutation_error_shape_violations_total`. The response itself is unchanged: the
check reports, it does not refuse.

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

> **Eligibility: every cascade entity has a global `id: ID!`** (the entity-identity
> contract, ADR-0017). Cascade entities ride the `CascadeUpdates` envelope as the
> `CascadeNode` interface (`id: ID!`, per the graphql-cascade spec), auto-implemented
> on every queryable entity so it is selectable via an inline fragment. The **Trinity
> external id `id: UUID` is canonicalized to `id: ID` at compile time** — wire-transparent,
> so the common case is cascade-eligible with no action. Only an entity whose `id` is
> a non-identity type (e.g. a serial `Int`) or absent cannot satisfy the interface:
> `fraiseql compile` then **fails fast** with one aggregated error naming each
> offending type, rather than emitting a schema the runtime would reject. Give such a
> type a UUID surrogate `id` (Trinity), or drop `cascade` from the mutations that
> return it. The identical contract applies to Relay `Node` for `relay = true` types.

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

> **Framework projections are not cascade entities (#665).** When the change-log GraphQL
> surface is exposed (`[changelog] expose = true`), FraiseQL injects two read-only
> projections — `EntityChangeLog` and `TransportCheckpoint`. They are the change-capture
> *mechanism*, not cascade-deliverable entities, so they are marked `internal` and excluded
> from cascade classification: they do **not** implement `CascadeNode` and are exempt from
> the `id: ID!` eligibility check above (`TransportCheckpoint` is keyed by `transport_name`
> and has no `id` by design, so the check would otherwise fail on a framework type the
> user never wrote). The runtime cascade path also rejects a payload entry naming an
> `internal` type, so a change-log row can never ride in a cascade. The two features
> compose freely — see [the change-log-over-GraphQL guide](../guides/changelog-graphql.md).

> **Embedded value objects are not cascade entities (#687).** A type with no independent
> identity — a `Money` amount, a `Dimensions` triple — is *embedded* under a parent entity,
> not a cache node of its own. Declare it `@fraiseql.type(embedded=True)`: it emits no
> `sql_source` and is exempt from the eligibility check above — it does **not** implement
> `CascadeNode` and is never enforced against `id: ID!`.
>
> ```python
> @fraiseql.type(embedded=True)
> class Money:
>     amount: int
>     currency: str
>
> @fraiseql.type(sql_source="v_order")
> class Order:
>     id: str
>     total: Money  # rides inside the Order payload; no id of its own
> ```
>
> This is **author-declared**, not inferred from the source. Without the marker, the SDK
> synthesizes a `v_money` source for every `@fraiseql.type`, which classifies `Money` as a
> cascade entity and fails the compile on a type that has no identity by design — the reason a
> value object under a `cascade` mutation could not compile before #687. The declaration is
> the fix, so `embedded=True` rejects an explicit `sql_source` (no backing view) and `cascade`
> (a value object cannot originate a cascade). Contrast the `internal` exemption above: that is
> set by the framework on projections it synthesizes; `embedded` is the *author's* declaration.

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

> **The view must be `security_invoker = true`** (FraiseQL's standard view
> convention). A *default* view runs with the view owner's
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
