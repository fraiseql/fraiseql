# ADR-0013: `app.mutation_response` v2 Composite Schema

## Status: Proposed

## Context

FraiseQL mutations return rows shaped like the `app.mutation_response` PostgreSQL composite. The v1 composite encodes the outcome of a mutation as a single `status TEXT` column using an open-ended prefix convention (`"new"`, `"updated"`, `"noop:no_changes"`, `"not_found:<entity>"`, `"conflict:<subtype>"`, `"validation:<subtype>"`, etc.). The Rust runtime parses that string to decide whether a row is a success or an error, and which error type to project.

This design has four compounding problems:

1. **String parsing as classification.** Every downstream consumer — the Rust parser, SDK clients, observability tooling — has to re-derive the error class from a stringly-typed field. The prefix list grows reactively; there is no compiler-enforced source of truth.

2. **Conflation of orthogonal concerns.** The `status` string encodes at least four orthogonal things: operation outcome (was it a create/update/delete), state change (did the DB actually change), error class (what went wrong), and error subtype (free-text detail). Bundling them into one column means every consumer must unbundle them.

3. **Error class is double-encoded.** Printoptim's `core.error_detail_*` templates carry `http_status` (404/409/422) in the JSONB `metadata` column. The status prefix also encodes the class (`conflict:*` vs `not_found:*`). These can disagree, and the Rust parser has no principled way to resolve them without a tiebreaker.

4. **No versioning.** Evolving the composite shape — adding a column, changing semantics — is a flag-day operation across every mutation function. There is no `schema_version` field, no side-by-side support, no graceful path.

An earlier iteration of this ADR (now superseded) proposed patching these problems in the Rust parser: close the prefix taxonomy with a typed enum, add an HTTP-code bridge, deprecate drift. That plan is internally coherent but leaves the data model untouched — the patches accumulate forever. The project's stated quality bar is **blueprint quality, whatever the cost**, which means fixing the data model is in scope.

## Decision

**Replace `app.mutation_response` v1 with a typed, versioned v2 composite. Migrate every consumer. Remove v1.**

### The v2 composite

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
    schema_version  SMALLINT,                    -- 2
    succeeded       BOOLEAN,                     -- terminal outcome
    state_changed   BOOLEAN,                     -- did DB state actually change
    error_class     app.mutation_error_class,    -- NULL iff succeeded
    status_detail   TEXT,                        -- human-readable subtype
    http_status     SMALLINT,                    -- first-class, 100–599
    message         TEXT,
    entity_id       UUID,
    entity_type     TEXT,
    entity          JSONB,                       -- always populated, including noops
    updated_fields  TEXT[],
    cascade         JSONB,
    error_detail    JSONB,                       -- structured error payload only
    metadata        JSONB                        -- observability only
);
```

### Semantics

| `succeeded` | `state_changed` | `error_class` | meaning                                 |
|-------------|-----------------|---------------|-----------------------------------------|
| `true`      | `true`          | `NULL`        | create / update / delete applied        |
| `true`      | `false`         | `NULL`        | noop (idempotent call, state unchanged) |
| `false`     | `false`         | non-null      | error — `error_class` drives cascade code |
| `false`     | `true`          | non-null      | **illegal** — enforced by validation    |

Enforced by a validation function called from `core.build_mutation_response`:

```sql
CHECK (
    (succeeded AND error_class IS NULL)
    OR (NOT succeeded AND error_class IS NOT NULL AND NOT state_changed)
)
```

### `MutationErrorClass` → `CascadeErrorCode`

One-to-one. No fallback tables, no HTTP-code tiebreakers, no prefix parsing:

| `MutationErrorClass`    | `CascadeErrorCode`    |
|-------------------------|-----------------------|
| `Validation`            | `VALIDATION_ERROR`    |
| `Conflict`              | `CONFLICT`            |
| `NotFound`              | `NOT_FOUND`           |
| `Unauthorized`          | `UNAUTHORIZED`        |
| `Forbidden`             | `FORBIDDEN`           |
| `Internal`              | `INTERNAL_ERROR`      |
| `TransactionFailed`     | `TRANSACTION_FAILED`  |
| `Timeout`               | `TIMEOUT`             |
| `RateLimited`           | `RATE_LIMITED`        |
| `ServiceUnavailable`    | `SERVICE_UNAVAILABLE` |

### Noop is a state-change flag, not a status

`noop:no_changes` (v1) becomes `succeeded=true, state_changed=false, entity={current row}` (v2). Idempotent deletes — which v1 had no clean way to express — become `succeeded=true, state_changed=false` on a DELETE mutation with no corresponding row. Callers that only care about "did anything happen" check `state_changed`; callers that want the current state read `entity`.

### Versioning

`schema_version SMALLINT` is the first column. v2 rows carry `2`. During migration, the Rust parser dispatches on this column and can parse both v1 and v2 rows. After v1 emitters are removed (Phase 04), the dispatcher collapses — but the column stays, signaling to future readers that the composite is principled about evolution.

### Migration strategy

1. Phase 00: design and spec (this ADR).
2. Phase 01: Rust v2 parser alongside v1; `parse_mutation_row` dispatches on `schema_version`.
3. Phase 02: printoptim adds v2 helpers (`build_mutation_response_v2`, `error_detail_*_v2`) beside v1.
4. Phase 03: printoptim migrates ~40 mutation functions to v2 in waves. Wave = PR. Rollback = revert.
5. Phase 04: remove v1 helpers, v1 composite, Rust v1 parser. Rename `_v2` suffix off everything.
6. Phase 05: FraiseQL downstream sweep (SDKs, examples, docs).
7. Phase 06: finalize — no scar tissue.

## Consequences

### Positive

- ✅ **Blueprint-quality data model.** Orthogonal concerns sit in orthogonal columns. No parsing, no classification, no tiebreakers.
- ✅ **`MutationErrorClass` is the cascade code.** 1:1 mapping, zero-effort wire-envelope emission when that work arrives.
- ✅ **Noop is a first-class flag.** Works for deletes, updates, and any future idempotent operation without extending the prefix vocabulary.
- ✅ **CHECK-enforced invariants.** Illegal states (partial-state errors, success-with-error-class) are rejected at the database layer, not papered over at the consumer.
- ✅ **Versioning.** Future v3 is a principled operation; v1→v2 proves the mechanism.
- ✅ **Eliminates JSONB soup.** `error_detail` (structured error payload) and `metadata` (observability) are separate. Consumers know where to look.
- ✅ **Cross-database portability story.** PG enum where native; `TEXT + CHECK` with the same value set on MySQL/SQLite/SQL Server. Rust parser is unified.

### Negative

- ⚠️ **Large migration.** ~40 printoptim functions, all helpers, templates, tests, fixtures, snapshots, SDK types. Mitigated by wave-based rollout, `schema_version` coexistence, and "revert the PR" as always-available rollback.
- ⚠️ **Cross-repo coordination.** FraiseQL and printoptim must ship the type changes in lockstep. Mitigated by the dispatcher pattern — v2 parser lands before any v2 emitters; v1 removal happens after all v1 emitters are gone.
- ⚠️ **One-time cost.** PG enum evolution is cheap for add (`ALTER TYPE ADD VALUE`) but expensive for removal. The error-class enum should be treated as stable; additions require ADR amendments.
- ⚠️ **Transitional complexity.** During Phases 02–03, both v1 and v2 emitters run. Documented as transitional; Phase 04 removes it.

## Alternatives Considered

### Alt 1: Taxonomy patch on v1 (the earlier version of this ADR)

Close the prefix set with a Rust enum, add an HTTP-code bridge, deprecate drift prefixes. Leaves the data model untouched.

**Rejected.** Internally coherent but aims at the wrong target. Patches accumulate; the root cause (`status TEXT`) persists. Fails the "blueprint quality" bar.

### Alt 2: Keep v1, add a typed sidecar column

Keep `status TEXT` for back-compat, add `error_class app.mutation_error_class` as a new column that shadows it. Consumers read the typed column; the string is ignored.

**Rejected.** Now every row carries two sources of truth that must agree. Same drift problem as the HTTP-code bridge — just in a different column. And you still have to migrate every function anyway to populate the new column.

### Alt 3: Drop the composite entirely, return JSONB

Functions return `JSONB` with an agreed shape; no composite type.

**Rejected.** Loses the strongest property of the current design: a single uniform type that one Rust parser consumes. JSONB "agreed shape" is exactly what we just escaped from at the status-string level. Drift at a higher level.

### Alt 4: Emit the cascade-spec wire shape directly from PG

PG functions return rows shaped like `{success, errors[], data, cascade}` — the cascade-spec envelope.

**Rejected for now.** Mixes two abstraction layers (storage convention and wire protocol). The composite should be the *storage* contract; the wire envelope is assembled by FraiseQL from the composite. This ADR clears the path for wire-envelope work; it does not pre-empt the design.

## Resolved Design Decisions

1. **Two separate types during migration.** `app.mutation_response_v1` (frozen) and `app.mutation_response` (new v2). `ALTER TYPE ... ADD ATTRIBUTE` was considered and rejected because v1 rows carry semantics that don't translate column-by-column to v2.
2. **`state_changed` is a boolean, not implied.** Considered deriving it from `updated_fields` non-empty; rejected because "no fields updated" and "no state change" are different concepts (e.g. a metadata-only update with no fact-field changes).
3. **`http_status` is a first-class column, not derived.** The `error_class` implies a default (422 for validation, 409 for conflict, 404 for not_found), but PG authors override per-case. Default is applied by `build_mutation_response` when `http_status` is not supplied.
4. **`error_detail` separated from `metadata`.** Error payload (field, constraint, severity) lives in `error_detail`. Observability (trace IDs, timestamps, audit extras) lives in `metadata`. Consumers probe one or the other, never both.
5. **`schema_version` is kept after v1 removal.** Signals the composite is principled about evolution. Costs 2 bytes per row.
6. **No `operation` column.** The mutation name ("createFoo") tells the caller what they asked for; re-emitting it in the response is redundant.
7. **Partial success uses `succeeded=true + error_detail`.** Matches the cascade spec's partial-success pattern (success with non-critical errors). The CHECK constraint forbids `succeeded=false + state_changed=true` specifically — a partial *failure* that leaves modified state is an error-handling bug at the function level, not a response shape.

## Implementation

See `.phases/mutation-response-v2/` for the 7-phase implementation plan:

- Phase 00: Design and Specification (this ADR + `docs/architecture/mutation-response.md`)
- Phase 01: Rust v2 parser with version dispatch
- Phase 02: Printoptim v2 helpers and templates
- Phase 03: Printoptim function migration in waves
- Phase 04: v1 removal
- Phase 05: FraiseQL downstream sweep
- Phase 06: Finalize

## References

- graphql-cascade specification, `specification/04_mutation_responses.md`
- Printoptim reference implementation: `../printoptim_backend/db/0_schema/03_functions/030_common/0302_mutation/`
- [ADR-0001: Three-layer architecture](0001-three-layer-architecture.md) — Rust runtime is the authoritative consumer of compiled schema + PG output
- `memory/project_graphql_cascade_spec.md` — FraiseQL's cascade implementation strategy
