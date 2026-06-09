# Change-Log Contract — `core.tb_entity_change_log`

Contributor reference for the framework-owned change-log table: the **first step
of the Change Spine** (an app-mediated, transactional change-capture
foundation). FraiseQL ships the table, owns its schema, and — by default — the
mutation executor writes one row per state-changing mutation **inside the
mutation's own transaction**, so the change record is atomic with the change.

The column set is the **superset** of what perf-observability (#392) needs and
what the Change Spine envelope needs, so the table ships once and is never
re-migrated for either consumer.

---

## Design principles

1. **One owned contract, shipped once.** The superset of perf + envelope columns
   lives in a single framework migration. New consumers read existing columns;
   they do not add migrations.
2. **Atomic, in-transaction outbox.** The row is INSERTed on the same
   connection/transaction as the mutation function, before commit — exactly-once
   with the state change, never a lossy dual write.
3. **Two producer conformance levels.** The FraiseQL executor is one producer;
   any writer of contract-conforming rows (an ETL job, a sister service) is a
   first-class producer too.
4. **Additive, idempotent migration.** `CREATE TABLE IF NOT EXISTS` +
   `ALTER … ADD COLUMN IF NOT EXISTS`. It never drops or renames a column. The
   one thing it *cannot* do is retype a pre-existing column — see
   [Upgrade & limitations](#upgrade--limitations).
5. **Single source of truth for the column set.** The typed contract
   [`fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT`] is what the
   migration DDL is checked against *and* what the `fraiseql doctor` drift check
   compares a live table against.

---

## The contract — superset column set

Placement: schema `core` (matches the existing reader and the
`03_add_nats_transport.sql` migration). DDL:
`crates/fraiseql-observers/migrations/08_create_entity_change_log_contract.sql`.

| Column | Type (PG) | `udt_name` | Source of need | Populated |
|---|---|---|---|---|
| `pk_entity_change_log` | `BIGINT GENERATED ALWAYS AS IDENTITY PK` | `int8` | reader cursor | always |
| `id` | `UUID NOT NULL DEFAULT gen_random_uuid()` | `uuid` | reader dedup / NATS id | always |
| `tenant_id` | `UUID` | `uuid` | RLS/JWT partition stamp (Trinity public id) | executor |
| `fk_customer_org` | `BIGINT` | `int8` | internal join FK (Trinity `fk_{entity}`) | app / producer |
| `fk_contact` | `BIGINT` | `int8` | actor join FK | app / producer |
| `object_type` | `TEXT NOT NULL` | `text` | perf #392 + reader | executor |
| `modification_type` | `TEXT NOT NULL` | `text` | perf #392 + reader (INSERT/UPDATE/DELETE/CUSTOM) | executor |
| `object_id` | `UUID` | `uuid` | perf #392 + reader | executor |
| `object_data` | `JSONB` | `jsonb` | entity payload | executor |
| `updated_fields` | `TEXT[]` | `_text` | envelope (`MutationResponse.updated_fields`) | executor |
| `cascade` | `JSONB` | `jsonb` | envelope (graphql-cascade) | executor |
| `duration_ms` | `INTEGER` | `int4` | **perf #392** (slowest-mutation ordering) | executor (PG) |
| `started_at` | `TIMESTAMPTZ` | `timestamptz` | **perf #392** (duration basis) | executor (PG) |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | `timestamptz` | perf + reader | always |
| `commit_time` | `TIMESTAMPTZ` | `timestamptz` | envelope (durable ordering) | executor |
| `seq` | `BIGINT` | `int8` | envelope (monotonic order; dedup on `(object_type, seq)`) | sequence default |
| `actor_type` | `TEXT` | `text` | #390 actor model | column now, value later |
| `acting_for` | `BIGINT` | `int8` | #390 acting-on-behalf-of | column now, value later |
| `schema_version` | `TEXT` | `text` | #377/#378 replay-correctness | column now, value later |
| `trace_id` | `TEXT` | `text` | perf #392 + #375 W3C trace | column now, value later |
| `trace_context` | `JSONB` | `jsonb` | #375 full W3C `traceparent`/`tracestate` | column now, value later |
| `change_status` | `TEXT` | `text` | reader | app / producer |
| `extra_metadata` | `JSONB` | `jsonb` | reader + the `duration_calc_version` marker | executor |
| `nats_published_at` | `TIMESTAMPTZ` | `timestamptz` | NATS bridge | bridge |
| `nats_event_id` | `UUID` | `uuid` | NATS dedup | bridge |

Indexes shipped: `idx_entity_log_duration (duration_ms DESC)` (slowest-mutation
forensics), `idx_entity_log_type (object_type)`, `idx_entity_log_created
(created_at)`, `idx_entity_log_tenant_seq (tenant_id, seq)`,
`idx_entity_log_type_seq (object_type, seq)`.

Read surface: the `core.v_entity_change_log` view exposes `duration_ms` + the
envelope columns top-level (indexed `WHERE`/`ORDER BY` for #392) and every
GraphQL field inside a `data` JSONB (keeps the #149 `entity_change_logs` query
stable).

---

## `tenant_id` vs `fk_customer_org` — complementary, not a rename

These are two distinct identifiers under the Trinity pattern. Never collapse
them:

- **`fk_customer_org`** — internal `BIGINT` foreign key, the Trinity `fk_{entity}`
  join slot. Kept as-is.
- **`tenant_id`** — the RLS/JWT partition key carried in the JWT →
  `SecurityContext`, a **UUID** (the public-facing identifier). Stamped
  *explicitly* at write time so out-of-session consumers (the change-log poller,
  the NATS bridge) re-authorize fan-out without reconstructing tenant identity
  from connection state — tenant identity is not portable across isolation
  models (PG `search_path` / MySQL current-DB / MSSQL `SESSION_CONTEXT`).

The executor parses `SecurityContext.tenant_id` with `Uuid::parse_str(…).ok()`;
a non-UUID tenant leaves the column `NULL` and **never** aborts the mutation —
the row is a log stamp, not the change itself.

So the contract is **purely additive**: `tenant_id` is added *alongside*
`fk_customer_org`, not in place of it.

---

## `seq` — a global sequence default

`seq` is fed by a plain global `SEQUENCE` set as the column `DEFAULT`
(`core.seq_entity_change_log`), not an executor-only counter. Any `INSERT` that
omits `seq` gets a monotonic value — the FraiseQL executor **and** cooperative
external producers alike. Durable ordering uses `seq`; dedup is on
`(object_type, seq)`.

---

## `duration_ms` — the canonical computation

`duration_ms` is **full wall-clock milliseconds** from `started_at` to the write,
on a single clock. The canonical expression lives in
[`fraiseql_db::changelog::duration_ms_sql`]:

```sql
(EXTRACT(EPOCH FROM (clock_timestamp() - current_setting('fraiseql.started_at')::timestamptz)) * 1000)::INTEGER
```

- **Both endpoints are on the DB clock** (`clock_timestamp()`). `started_at` is
  set transaction-locally via `set_config('fraiseql.started_at',
  clock_timestamp()::text, true)` before the mutation function runs, so there is
  no app↔DB clock skew. (`fraiseql_db::changelog::STARTED_AT_VAR` /
  `CLOCK_TIMESTAMP_DIRECTIVE`.)
- **`EXTRACT(EPOCH …)`, never `EXTRACT(MILLISECONDS …)`.** The
  `MILLISECONDS` form returns only *seconds-within-the-minute × 1000*, so it
  under-reports any interval ≥ 1 minute (`00:01:30.250` → `30250`, not `90250`).

### Data-quality marker (#392)

Each framework-written row stamps
`extra_metadata->>'duration_calc_version'` with
[`fraiseql_db::changelog::DURATION_CALC_VERSION`] = **`2`**: the
wall-clock-correct, single-DB-clock computation above. Legacy app-written rows
carry no marker (or `1`). #392's `null-rate` / forensics tooling uses the marker
to **refuse to mix incomparable rows** — pre-fix `EXTRACT(MILLISECONDS)` rows and
post-fix epoch rows must not be averaged together.

---

## Producer conformance levels

The contract owns the *schema + semantics*; the executor is one producer, and
external producers writing conforming rows are a first-class on-ramp to the
Spine.

### Full — the FraiseQL executor

Writes every request-scoped column: identity + change columns, `duration_ms`,
`started_at`, `commit_time`, `tenant_id`, `updated_fields`, `cascade`, and the
`duration_calc_version` marker. The write is the in-transaction `MATERIALIZED`
CTE in the PostgreSQL adapter
(`crates/fraiseql-db/src/postgres/adapter/database.rs`), prepared once and cached
(`prepare_cached`) so the per-mutation cost is dominated by index maintenance,
not statement re-parse.

### Cooperative external producer (ETL / jobs / sister services)

Supplies the identity + change columns it can know by value — `object_type`,
`modification_type`, `object_id`, `tenant_id`, `object_data`,
`updated_fields`, `cascade` — and lets the table's `seq` default fire. The
portable, fully-parameterized INSERT for non-PostgreSQL dialects is
[`fraiseql_db::changelog::build_changelog_insert_sql`] over
[`fraiseql_db::changelog::CHANGELOG_PORTABLE_INSERT_COLUMNS`].

For these rows **`duration_ms` and `started_at` are legitimately `NULL`** —
there is no FraiseQL request context to measure. This is expected, not drift;
#392's `null-rate` subcommand exists to measure exactly this population.

---

## Opt-out

The write is on by default. Two levels, AND-composed by the runner:

- **Global** — `RuntimeConfig.changelog_enabled`, resolved from
  `FRAISEQL_CHANGELOG_ENABLED` (env) → `[changelog] write_enabled` (compiled
  schema, default `true`) → `true`.
- **Per-mutation** — `MutationDefinition.changelog` (compiled, serde-default
  `true`), authored as `@fraiseql.mutation(changelog=False)` (Python) or
  `@Mutation({ changelog: false })` (TypeScript). The decorators validate the
  value is a boolean and emit the key only when set, so a schema authored
  without it keeps logging.

`changelog_enabled && mutation_def.changelog` decides whether the row is written
for a given mutation.

---

## Ownership & migration (fix-forward)

FraiseQL **owns** `core.tb_entity_change_log`. A downstream app that previously
created its own version and INSERTed into it from mutation functions upgrades to
the framework-owned contract — a documented **breaking** change:

1. **The migration brings the table to the contract.** Additive + idempotent;
   `id`/`created_at` backfill from defaults, every other added column is
   nullable. It never drops or renames.
2. **The executor owns the write** (no compat flag). Apps **remove their
   hand-rolled inserts**. Cooperative external producers writing conforming rows
   remain first-class — a different, supported pattern, not a double write.
3. **The drift check (`fraiseql doctor`) reports reconciliation work** — see
   below.

### Upgrade & limitations

The migration is **additive**, so `ADD COLUMN IF NOT EXISTS` **no-ops on a column
that already exists** and therefore **cannot retype or re-null it**. The
real-world hazard: a legacy app table with `object_id TEXT NOT NULL` is *not*
converted to the contract's `object_id UUID` (this bit the #149 change-log e2e).
The fix is a manual one-off:

```sql
ALTER TABLE core.tb_entity_change_log
    ALTER COLUMN object_id TYPE UUID USING object_id::uuid;
```

### Drift check (#380)

`fraiseql doctor --against-db postgres://…` reports drift between the live table
and the shipped contract, sourced from the same
`ENTITY_CHANGE_LOG_CONTRACT`:

- **Missing** contract column → warning; `fraiseql migrate up` adds it.
- **Type mismatch** on a pre-existing column → failure; the additive migration
  cannot fix it (e.g. a legacy `object_id text`) — `ALTER … TYPE` manually.
- **Extra** non-contract column → warning; left untouched (app-specific columns
  are safe to keep).

---

## This is the Change Spine first step — consumer map

- **#392 perf-observability** — the first consumer. Reads `duration_ms` via
  `v_entity_change_log`; the `duration_calc_version` marker gates pre/post-fix
  mixing.
- **#382 CDC broker fan-out** — drains this executor-written outbox
  (`FOR UPDATE SKIP LOCKED`); no WAL needed.
- **#374 multi-DB parity** — the outbox is a plain INSERT → portable across
  PG/MySQL/SQLite/MSSQL.
- **#366 WAL-CDC** — demoted to an opt-in PG-only producer behind this same
  envelope; the Kafka firehose is ceded to Debezium.
- **Future consumers** (columns shipped now, populated later): #390 audit-actor
  (`actor_type`/`acting_for`), #377/#378 replay (`schema_version`), #375
  OpenTelemetry (`trace_id`/`trace_context`).

---

## Open follow-ups

Tracked, not yet built:

- **Live MySQL / SQL Server adapter outbox wiring + per-DB tests.** The portable
  builder ([`build_changelog_insert_sql`]) and the `09_*`/`10_*` DDL ship; the
  MySQL (sqlx) and MSSQL (tiberius) adapters still delegate to the no-op default
  (no outbox row written), so there is no regression — but no row either. Wiring
  them needs the two drivers' in-transaction call semantics (MySQL's
  `CALL`-in-txn caveat) and per-DB test infrastructure.
- **Poller projection widening.** The change-log poller
  (`crates/fraiseql-observers/src/listener/change_log.rs`) could surface
  `duration_ms` / `tenant_id` / `seq` top-level for richer fan-out filtering.

---

## Anchor paths

- Contract DDL: `crates/fraiseql-observers/migrations/08_create_entity_change_log_contract.sql`
  (MySQL `09_*`, MSSQL `10_*`).
- Column set (single source of truth): `fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT`
  (+ `entity_change_log_contract_sql()`).
- `duration_ms` + markers: `fraiseql_db::changelog` (`duration_ms_sql`,
  `STARTED_AT_VAR`, `DURATION_CALC_VERSION`).
- Portable INSERT: `fraiseql_db::changelog::{build_changelog_insert_sql, CHANGELOG_PORTABLE_INSERT_COLUMNS}`.
- Executor in-txn write: `crates/fraiseql-db/src/postgres/adapter/database.rs`.
- Reader / poller: `crates/fraiseql-observers/src/listener/change_log.rs`.
- Drift check: `crates/fraiseql-cli/src/commands/doctor.rs` (`fraiseql doctor --against-db`).
- Related: [`mutation-response.md`](./mutation-response.md) — the row the executor parses to fill the envelope.
