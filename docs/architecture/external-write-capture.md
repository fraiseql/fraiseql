# External-Write Capture (#366)

How an **uncooperative external write** — a raw `INSERT INTO tb_post …` from
`psql`, a database migration, a background job, or a third-party tool — reaches
GraphQL subscribers, **without** double-emitting for writes that already flow
through FraiseQL's mutation executor.

This is the Change Spine's fallback producer. It complements, and never
duplicates, the app-mediated [in-transaction outbox](./change-log-contract.md):

```
                          core.tb_entity_change_log
                                    │
   GraphQL mutation ── executor outbox row (full envelope) ──┐
                                                             ├─► reader/poller ─► subscribers
   raw psql write ──── capture-trigger row (Debezium env.) ──┘
```

---

## The duplication problem, and the fix

A plain capture trigger on `tb_post` would fire for **every** write — including a
normal FraiseQL mutation, which already writes its own change-log row. That is a
double emit.

The fix is a **transaction-local marker**. FraiseQL's mutation executor runs each
mutation inside a transaction and, at the start of it, sets:

```sql
SET LOCAL fraiseql.cdc_mediated = 'on'   -- fraiseql_db::CDC_MEDIATED_VAR / CDC_MEDIATED_ON
```

(implemented as `set_config('fraiseql.cdc_mediated', 'on', true)` in the
PostgreSQL adapter — transaction-local, so it auto-resets on commit and is
invisible to other connections).

The shipped trigger function checks it **first** and stays silent when it is set:

```plpgsql
IF current_setting('fraiseql.cdc_mediated', true) = 'on' THEN
    RETURN NULL;          -- app-path write: the executor outbox already logged it
END IF;
-- …otherwise capture this external write…
```

Because the marker is **transaction-scoped**, it covers everything that
transaction does — 1 row or 1,000,000 rows, across any number of statements. So a
FraiseQL mutation whose function internally touches many rows still produces
exactly its **one** logical outbox row; the trigger is suppressed wholesale.

| Write path | Marker | Executor row | Trigger row | Total |
|---|---|---|---|---|
| FraiseQL mutation (`changelog` on) | `on` | 1 (full envelope) | suppressed | **1** |
| FraiseQL mutation (`changelog=false`) | `on` | 0 (opted out) | suppressed | **0** |
| External write (psql / migration) | unset | 0 | 1 (Debezium env.) | **1** |

No path can double-emit: the marker and the outbox write are set in the same
transaction, so whenever the executor writes a row the trigger is suppressed.

---

## What a captured row contains

A captured row is a first-class `core.tb_entity_change_log` row, so the existing
reader, poller, and NATS bridges fan it out unchanged:

- **`object_type`** — the **GraphQL type name** (e.g. `Post`), baked into the
  trigger at install time. The reader and the subscription matcher key on the type
  name, never the table name, so no `table → type` lookup is needed.
- **`modification_type`** — `INSERT` / `UPDATE` / `DELETE` (`TG_OP`).
- **`object_data`** — the **after-image** (`NEW`): the full post-mutation row as JSONB.
  The op code is taken from `modification_type`, not embedded in `object_data`. The reader
  still exposes `ChangeLogEntry::debezium_operation` / `after_values` / `before_values`.
- **`object_data_before`** — the **pre-image** (`OLD`), recorded only for tables that opt in
  via `@subscribable(pre_image=True)`; otherwise `NULL`.
- **`object_id`** — the row's public id column (default `id`), which **must be a
  UUID** (see below).
- **`tenant_id`** — the configured tenant column (default `tenant_id`) when present
  and UUID-shaped, else the cooperative session GUC `fraiseql.tenant_id`. Per-tenant
  subscription filtering applies unchanged.
- **Cooperative envelope** — `actor_type` / `acting_for` / `schema_version` are
  stamped from the matching `fraiseql.*` session GUCs when an external writer sets
  them, else `NULL` (degraded but valid).
- **`extra_metadata`** — `{"cdc_source": "fallback_trigger"}`, so a captured row is
  distinguishable from an executor-written one. `seq` / `id` / `created_at` fire
  from the table's own column defaults.

### Envelope completeness vs. the app path

An **app-path** mutation records the full Change-Spine envelope (actor, tenant,
trace, duration, cascade, schema version) on its single outbox row. A **captured
external** row records only what is knowable at the trigger: the changed row, the
operation, the type, and the tenant — plus any cooperative `fraiseql.*` GUCs the
writer chose to set. This is expected, not drift.

---

## Declaring and installing capture

Declare which physical table(s) back a subscribable type:

```python
@fraiseql.type(subscribable_tables=["tb_post"])
class Post:
    id: UUID
    title: str
```

The compiler aggregates these into the compiled schema (`subscribable`), and the
CLI emits a self-contained, idempotent install script (the capture function plus
the per-table triggers):

```bash
fraiseql generate-capture-triggers -s schema.compiled.json | psql "$DATABASE_URL"
# or: fraiseql generate-capture-triggers -s schema.compiled.json -o capture.sql
```

Each subscribable table gets **three** statement-level triggers (INSERT / UPDATE /
DELETE). The contract table `core.tb_entity_change_log` must already exist — install
the [change-log contract migration](./change-log-contract.md) first.

### Requirement: a UUID public-id column

The change-log reader decodes `object_id` as a **non-null `uuid`** over the whole
poll batch, so a single row with a NULL / non-UUID `object_id` would fail the
decode for the entire batch and **stall the poller permanently**. The capture
trigger therefore filters to rows whose public-id column is a valid UUID and
**skips** any others (it never writes a NULL `object_id`, and never aborts the
user's write). Consequently, a `@subscribable` table **must** expose a UUID `id`
column — a table without one is silently not captured rather than risking a stall.

---

## Bulk writes

The triggers are `AFTER … FOR EACH STATEMENT` with transition tables (`OLD TABLE` /
`NEW TABLE`), so a bulk statement is captured as a **single set-based INSERT**, not
one trigger invocation per row:

```sql
-- 200,000 rows → 200,000 change-log rows, captured by ONE trigger firing.
UPDATE tb_post SET archived = true WHERE created_at < '2020-01-01';
```

This yields one event per changed row (the correct CDC granularity) at the cost of
one statement-level invocation. Note the inherent asymmetry: a FraiseQL mutation
that changes N rows records **one** logical change-log row (the executor's view),
whereas an opaque external bulk write of N rows records **N** rows (the row-level
CDC view).

---

## Caveats

- **`session_replication_role = replica`.** These are ordinary (origin) triggers,
  so a session running with `session_replication_role = replica` (logical-replication
  apply, some bulk loaders) does **not** fire them — those writes are not captured.
  This is the documented opt-out for true bulk loads where capture is undesirable.
- **gRPC mutations.** The marker is set on the executor's session/outbox paths; a
  mutation issued over the gRPC transport that bypasses those paths is captured by
  the fallback trigger (with the degraded external envelope) rather than the rich
  executor outbox. There is no duplication — the gRPC path writes no outbox row of
  its own.
- **PostgreSQL only.** The marker is transaction-local PostgreSQL state and the
  capture function is PL/pgSQL. The MySQL / SQL Server contract migrations are
  unaffected.

---

## Anchor paths

- Marker constants: `fraiseql_db::{CDC_MEDIATED_VAR, CDC_MEDIATED_ON}`
  (`crates/fraiseql-db/src/changelog.rs`); set in the PostgreSQL adapter
  (`crates/fraiseql-db/src/postgres/adapter/database.rs`, `mark_cdc_mediated`).
- Trigger function: `crates/fraiseql-observers/migrations/11_create_change_log_capture_trigger.sql`
  (exposed as `fraiseql_observers::migrations::entity_change_log_capture_trigger_sql`).
- `@subscribable` threading: `IntermediateType.subscribable_tables` →
  `CompiledSchema.subscribable` (`fraiseql_core::schema::SubscribableEntity`).
- DDL generator: `fraiseql_core::schema::generate_capture_trigger_ddl`; CLI command
  `crates/fraiseql-cli/src/commands/generate_capture_triggers.rs`.
- Reader / poller: `crates/fraiseql-observers/src/listener/change_log.rs`.
- Related: [`change-log-contract.md`](./change-log-contract.md).
