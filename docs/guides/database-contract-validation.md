# Validating the database contract (`--against-db`)

FraiseQL compiles your schema to SQL calls that the server makes at runtime, but
two things are only mirrored *by hand* between the compiled schema and your
PostgreSQL functions:

1. The **server↔database mutation contract** — which function the server calls
   for each mutation, with which arguments, and which columns it decodes back.
2. **Internal PL/pgSQL calls** — one of your functions calling another.

PostgreSQL does not validate either at `CREATE FUNCTION`/`compile` time (it
defers PL/pgSQL body analysis to runtime), so drift surfaces as an opaque 500
the first time the affected path runs. Two CLI checks catch this at CI time
instead, against a live database, without booting a server.

> Both checks are PostgreSQL-only and read-only. Neither invokes a mutation, so
> they have no database side effects.

## `validate --against-db` — the mutation contract (#397)

```bash
fraiseql-cli validate --against-db "$DATABASE_URL" schema.compiled.json
```

For each database-backed mutation it verifies, against `pg_proc` and the
function's return type:

**Call binding**

- `sql_source` resolves to **exactly one** function (catches *function does not
  exist* and *function is not unique*).
- The function's input arity equals what the runtime sends: the positional
  arguments (flat arguments, the flattened fields of a single `input` object, or
  — on the update path — a single `jsonb` payload) **plus** the trailing
  injected (`@inject`) parameters.
- On the update path, the first parameter is `jsonb`.
- The trailing parameter names match the inject keys, in order (the server binds
  inject values positionally, so a reordering silently mis-binds them).

**Response shape**

- The result row carries `succeeded` and `state_changed` (both `boolean`) — the
  columns the server *requires* to decode `MutationResponse`.
- Any optional `MutationResponse` columns it declares (`error_class`,
  `http_status`, `entity`, …) have compatible types. `error_class` may be `text`
  or a project enum.

Error-severity findings fail the command (exit code 1) so it gates CI; warnings
(e.g. an advisory inject-name mismatch, or a function whose response shape can't
be introspected) do not. Add `--json` for a machine-readable report.

**Out of scope.** The *behavioural* response invariants (`succeeded ⇒
error_class IS NULL`, `¬succeeded ⇒ error_class IS NOT NULL ∧ ¬state_changed`,
`http_status ∈ 100..=599`) are properties of the function's runtime output and
are only observable by invoking the mutation — which would have side effects.
This static check verifies the *shape*, not the *behaviour*.

## `doctor --against-db` — internal-call resolution (#409)

```bash
fraiseql-cli doctor --against-db "$DATABASE_URL" --schemas app,core
```

This runs a PL/pgSQL **body-resolution** pass: for each managed function in the
listed schemas, it resolves every call inside the body against the live catalog
and reports unresolved internal calls (e.g.
`function helper.error_detail_not_found(unknown, uuid, unknown) does not exist`)
as failed doctor checks. This is the *internal-call* analogue of
`validate --against-db`, which only sees a mutation's server-facing signature.

The pass uses the [`plpgsql_check`](https://github.com/okbob/plpgsql_check)
extension. When it is **not installed** (the common case on managed Postgres),
the pass is **skipped with a warning** and an install hint rather than failing —
so `doctor` stays usable everywhere. `--schemas` defaults to `public`.

## Suggested CI usage

```bash
# Fail the build on any server↔database mutation-contract drift.
fraiseql-cli validate --against-db "$DATABASE_URL" schema.compiled.json

# Surface broken internal callers (where plpgsql_check is available).
fraiseql-cli doctor --against-db "$DATABASE_URL" --schemas app,core
```
