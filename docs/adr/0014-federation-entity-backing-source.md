# ADR-0014: Federation `_entities` Backing-Source Resolution

## Status: Proposed — pending GraphQL-federation-expert review

This ADR documents the design used to resolve **owner-split `extends` entities** in
FraiseQL subgraphs (issue #507), the alternatives considered, and the open questions we
would like a federation expert to validate. It is written to be self-contained for a
reviewer without prior FraiseQL context.

Related: ADR-0001 (three-layer architecture), `docs/architecture/federation.md`,
issues #504 / #506 / #507.

---

## Context

### What FraiseQL is

FraiseQL is a **compiled GraphQL-to-SQL engine**. Schemas are authored in Python/TS
(authoring layer), compiled to a `CompiledSchema` JSON by a Rust CLI (compilation
layer), and executed by a Rust server that generates SQL at runtime (runtime layer).
There is no per-request resolver code — the server turns a GraphQL operation into a SQL
statement using the compiled schema.

### Federation and `_entities`

In Apollo Federation v2 a single entity type (e.g. `User`) is split across subgraphs:
one subgraph **owns** it, others **extend** it. The gateway resolves an entity by
fanning out, then merging the partial results on the entity's `@key`:

```
                    ┌─────────────────────────┐
   client ───────▶  │   Apollo Router         │
                    └───────────┬─────────────┘
                       fan-out  │  merge on @key(id)
            ┌──────────────────┼──────────────────┐
            ▼                                       ▼
   ┌──────────────────┐                  ┌──────────────────┐
   │  Subgraph A      │                  │  Subgraph B      │
   │  type User @key  │                  │  extend type     │
   │    id            │                  │    User @key     │
   │    name          │                  │    id @external  │
   │                  │                  │    reviewcount   │
   └──────────────────┘                  └──────────────────┘
       OWNS User                            EXTENDS User
```

Each subgraph implements the federation-standard `_entities(representations: [_Any!]!)`
query. Given `[{ __typename: "User", id: "user-1" }]`, Subgraph B must return the
fields **it** contributes (`reviewcount`) for that key.

### What "resolving `_entities`" means in FraiseQL (the SQL it must emit)

To answer an `_entities` request, FraiseQL generates:

```
   SELECT  <field projections>          ◀── (b) HOW are fields stored?
   FROM    <relation>                   ◀── (a) WHICH table/view?
   WHERE   <key> IN ($1, $2, …)
```

So per entity type it needs a **backing source** = the pair:

```rust
// crates/fraiseql-federation/src/types.rs
pub struct EntitySource {
    pub relation: String,             // e.g. "v_user", "user", "app.v_org"
    pub jsonb_column: Option<String>, // Some("data") → jsonb mode; None → flat mode
}
```

FraiseQL supports two field-storage shapes:

```
  jsonb-"data" view (the FraiseQL standard)      flat-column table/view
  ┌───────────────────────────────────┐         ┌──────────────────────────┐
  │ data (jsonb)                       │         │ id    │ reviewcount │ …  │
  │ {"id":"user-1","name":"Alice"}     │         │ user-1│     42      │    │
  └───────────────────────────────────┘         └──────────────────────────┘
  field → data->'is_active' (typed),             field → bare column
  key   → data->>'id'                            key   → id::text IN (…)  [PG]
```

### The gap this ADR closes

Where the backing source comes from differs by entity kind:

```
  OWNED entity (Subgraph A)                EXTENDS entity (Subgraph B)
  ─────────────────────────                ───────────────────────────
  Has a root query:                        Has NO root query in this subgraph.
    query user(id): User                   It only contributes fields to a User
        sql_source = "v_user"              owned elsewhere.
        jsonb_column = "data"
                                           ⇒ nothing to read the relation off of.
  ⇒ source rides on the QUERY.                                       ◀── THE GAP
```

History:

| Issue | Fixed | Left |
|-------|-------|------|
| **#504** | Bug report: resolver guessed `FROM lower(typename)` (`user`, `organization`) — usually a non-existent relation → query errored → gateway swallowed it to `null`. | everything |
| **#506** | **Owned** entities: source `(relation, jsonb_column)` from the backing **query** (`queries[]`, keyed by `return_type`). | **Extends** entities: no query ⇒ map stayed empty for them ⇒ still fell back to `lower(typename)` ⇒ still `null`. |
| **#507** (this ADR) | **Extends** entities: source the relation from a **type-level `sql_source`**. | SDK must emit it (out-of-repo). |

---

## Decision

### 1. The extends relation lives on `TypeDefinition.sql_source` (the `types[]` array)

The authoring SDK emits a type-level `sql_source` (and, implicitly, a `jsonb_column`)
for an owner-split `extend type` entity. The compiler carries it to
`CompiledSchema.types[].sql_source`; previously this field was always written empty.

### 2. A single builder unifies both entity kinds (query-first, type-fallback)

```rust
// crates/fraiseql-core/src/schema/compiled/schema_domain.rs  (feature = "federation")
CompiledSchema::entity_sources() -> HashMap<typename, EntitySource>
  (1) for each query with sql_source:                       ┐ OWNED — first-wins
        map[q.return_type] = (q.sql_source, q.jsonb_column) ┘ per return_type
  (2) for each type with non-empty sql_source NOT already mapped:  ┐ EXTENDS (#507)
        map[t.name] = (t.sql_source, t.jsonb_column)               ┘ fills gaps only
```

A query-sourced relation **always wins**; the type-level value only fills the gap an
extends entity leaves. (Rationale for query-first: a type can be read by *multiple*
queries with *different* views, so the relation is not a 1:1 property of the type for
owned entities — the query is the more precise source.)

The `_entities` resolver consumes this map; absent an entry it still falls back to
`lower(typename)` (legacy / unit-test behaviour).

### 3. jsonb vs flat uses one convention, shared with the query path

`jsonb_column` non-empty → jsonb projection (`<col>->'<snake(field)>'`, typed,
camelCase→snake); empty → flat (bare columns, key cast `id::text` on PostgreSQL). The
compiler **defaults an extends entity's `jsonb_column` to `"data"`** (the FraiseQL
standard), symmetric with how the query path defaults a query's `jsonb_column` to
`"data"`. A flat-column extends entity is therefore the one authored with an explicit
empty `jsonb_column`.

`relation` is quoted defensively at SQL-build time (`quote_relation`): split on `.`,
each segment validated as a safe SQL identifier and double-quoted, so `app.v_org`
becomes `"app"."v_org"`. The authored value must be the **bare** identifier (`user`),
not pre-quoted (`"user"`).

---

## Alternatives considered

### Candidate 2 — put the relation on `FederationEntity` (the `federation` config block)

Tempting: federation-specific, co-located with `@key`/`extends`, and a fresh
`Option<String>` field avoids reusing the (historically vestigial) `TypeDefinition`
fields.

**Rejected** because of a merge trap in the compile pipeline. The SDK `schema.json` and
`fraiseql.toml` are merged before conversion. The merger **overwrites** the federation
config from TOML whenever `[federation].enabled`, and the TOML-side entity struct is
`#[serde(deny_unknown_fields)]` with only `{name, key_fields}`:

```
   merger.rs:625
   if toml_schema.federation.enabled {
       merged["federation_config"] = toml_schema.federation.to_compiled();  // OVERWRITES
   }                                                                        // maps only
                                                                            // {name,key_fields}
```

```
            SDK emits federation block            TOML [federation].enabled = true
            with sql_source on entity
                      │                                   │
                      ▼                                   ▼
   Candidate 2:   present in merged ───────────▶  OVERWRITTEN, sql_source LOST  ✗
   (federation.entities[])

   Candidate 1:   present in merged.types[] ────▶  untouched (TOML never           ✓
   (types[])                                       writes types[])
```

`types[]` is a separate top-level key the TOML merger never rewrites, so a type-level
`sql_source` survives **both** federation-config paths. Candidate 2's value would be
silently dropped the moment a user enables `[federation]` in TOML — the same
silent-drop class that caused #495 and #504. This was the deciding factor.

### Candidate 3 — every `TypeDefinition` owns its relation (make queries pure references)

Unify owned + extends by populating `types[].sql_source` for **all** types and having
queries reference the type. **Rejected:** a type can be returned by multiple queries
with different views (`v_user`, `v_user_admin`), so `relation` is not a 1:1 type
property for owned entities. Query-first + type-fallback models reality; Candidate 3
would be wrong in the multi-view case and is a large, risky refactor of the query path.

---

## Consequences

**Positive**
- Extends entities resolve correctly (flat and jsonb), completing #504/#506.
- Robust against the TOML federation-merge overwrite (the decisive property).
- Reuses the existing query-path jsonb/flat convention — one mental model.
- Owned entities unaffected (query always wins); compiled output for non-extends types
  is byte-identical (type `sql_source`/`jsonb_column` stay empty).

**Negative / trade-offs**
- Reactivates two previously-dead reads of `TypeDefinition`:
  - the CLI optimizer's projection-hint heuristic keys on a non-empty `jsonb_column`
    (extends types now have `"data"`). Mitigated: extends stubs have few fields, the
    heuristic needs >10, and any hint is inert for a type with no local query.
  - the runtime mutation cache-invalidation path reads `type.sql_source`. Inert for
    extends entities (not mutated in the extending subgraph) — or correct if they were.
- The "empty `jsonb_column` = flat" convention is subtle: `TypeDefinition.jsonb_column`
  has a serde default of `"data"`, so a hand-authored compiled-schema fixture that
  *omits* the field gets jsonb mode; a flat extends entity must set it to `""` explicitly.
- The SDK (out-of-repo) must emit the type-level `sql_source`; until then extends
  entities are unresolved (graceful: `null`, as before).

---

## Open questions for the federation expert

1. **Model sanity.** Is it correct for an *extending* subgraph (which does not own the
   type) to resolve `_entities` by reading its **own** local relation keyed by the
   entity `@key`? Our model: each subgraph stores the fields it contributes in a local
   table/view keyed by the shared key, and `_entities` reads that. Is this the intended
   federation contract, or is there a case where the extending subgraph should resolve
   differently (e.g. delegate, or require a join)?

2. **`@requires` / `@provides`.** A contributed field may `@requires` external fields,
   which the gateway passes inside the representation. Our backing-relation read keys
   only on the entity `@key` and ignores other representation fields. Is that a
   correctness gap for `@requires`-bearing fields, and should the backing source model
   account for it?

3. **Composite & multiple `@key`s.** The backing source is per-type (one relation);
   key shape is handled entirely in the `WHERE … IN` clause. Is anything about
   composite keys or multiple `@key` directives incompatible with a single
   per-type backing relation?

4. **Flat-vs-jsonb default.** We default an extends entity's storage shape to the
   jsonb-`data` view (FraiseQL's norm) and require explicit opt-out for flat columns.
   Is defaulting (vs requiring the SDK to state the shape explicitly) reasonable?

5. **Owner-side resolvability.** When an entity's `@key` is marked non-resolvable in a
   subgraph, should `entity_sources` deliberately *omit* it so `_entities` cannot
   resolve it locally? Currently resolvability is enforced elsewhere; should the backing
   source respect it too?

6. **Multi-subgraph / shareable.** Any concern when the same type is `@shareable` and
   resolvable in more than one subgraph, each with its own backing relation?
