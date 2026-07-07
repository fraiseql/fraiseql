# ADR-0017: Entity Identity Contract (`id: ID!`)

## Status: Accepted

This ADR records the decision to give FraiseQL a single, enforced notion of entity
identity: **every queryable entity exposes a global `id: ID!`**, and the compiler
canonicalizes the Trinity external id (`id: UUID`) to `id: ID` to make that true by
construction.

Related: `docs/architecture/mutation-response.md` (cascade), `examples/_TEMPLATE`
(Trinity pattern), ADR-0014 (federation entity backing source), the graphql-cascade
spec (`../graphql-cascade/reference/cascade_base.graphql`).

---

## Context

Several subsystems independently need "the id of an entity":

- **graphql-cascade** — the `CascadeNode` interface (`id: ID!`); collateral entities
  ride `UpdatedEntity.entity: CascadeNode!`, selected via inline fragments.
- **Relay** — the `Node` interface (`id: ID!`) for `relay = true` types.
- **Apollo Federation** — `@key(fields: "id")` for entity resolution.
- **Cache normalization / cursor pagination** — keying by a stable id.

All four assume a uniformly-typed opaque identifier. FraiseQL, however, keeps `ID`
and `UUID` as **distinct scalars**, and entities were authored with heterogeneous id
types — `ID`, `UUID`, `Int`, or a non-`id` key (`examples/basic` uses `id: Int`).

The concrete failure that motivated this ADR: cascade synthesis (2.11) force-added
`implements CascadeNode` (`id: ID!`) to every queryable entity, then the compiled
schema failed its own validator with a swallowed "missing field 'id'" bail whenever
an entity's id was `UUID`/`Int`. Each id-consuming subsystem was independently
rediscovering the same heterogeneity — a missing invariant, surfacing as a cascade
bug.

The Trinity pattern (`examples/_TEMPLATE`) already settles what identity *is*:

```sql
pk_entity  INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- internal join key, never exposed
id         UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,    -- external, stable identity
identifier TEXT UNIQUE                                        -- optional human-readable business key
```

The external identity is `id UUID`. GraphQL's `ID` scalar is the spec's opaque,
stringified identifier — and a UUID serializes as the *same* JSON string.

---

## Decision

**1. Contract.** Every queryable entity exposes a global `id: ID!`. This is a
first-class FraiseQL invariant, consumed uniformly by cascade, Relay, federation,
and caching — not re-derived per subsystem.

**2. Canonicalize the Trinity id.** The compiler rewrites an authored `id: UUID` to
`id: ID` on every output object type (`converter::identity`), before the
interface-forcing passes. This is **wire-transparent** (a UUID and an ID serialize to
the same string) and makes every Trinity entity satisfy `Node` / `CascadeNode` /
`@key(fields: "id")` automatically. `ID` is also the canonical, valid federation key
type, so this is federation-friendly.

**3. Enforce the backstop.** An entity that opts into a Node-style interface
(cascade/relay) but cannot present `id: ID!` — a non-identity `id: Int`, an exotic
type, or no `id` field at all — fails `compile` fast with one aggregated, actionable
error naming each offending type and the remedy (`converter::interface_conformance`).
It never emits IR that the validator rejects with a swallowed bail.

**4. Legible diagnostics.** The CLI surfaces the full `anyhow` cause chain in human
and `--json` output, so any residual rejection is self-diagnosing.

### Rejected alternatives

- **Skip + warn** (drop non-conforming entities from cascades): silent degradation —
  antithetical to a correctness-first platform.
- **Union instead of interface** (`UpdatedEntity.entity` as a union of concrete
  types): solves cascade locally but grows unbounded with the schema, forces clients
  to enumerate every type to select `id`/`__typename`, diverges from the graphql-cascade
  spec FraiseQL publishes, and leaves Relay/federation with the same tension. A local
  optimum, not the invariant.
- **Canonicalize `Int`/`String` ids too**: `id: Int` → `ID` is a wire-visible change
  (number → string) and legitimizes exposing a serial pk as a global id, which the
  Trinity deliberately avoids. Left to the backstop so the author adopts a real
  identity (UUID) consciously. Only the wire-transparent `UUID` → `ID` is automatic.

---

## Consequences

- The reporter's Trinity schema (federation entities with `id: UUID` + cascade) now
  compiles with no per-entity edits — the id canonicalizes transparently.
- Cascade, Relay, and federation `@key` share one identity contract; a fix in one no
  longer leaves the others latently broken.
- `id: UUID` no longer appears in compiled output / introspection — it is `ID`. This
  is wire-compatible (same string), but a consumer that introspected `id: UUID`
  literally will see `ID`. Documented in the CHANGELOG.
- Entities with a non-identity `id` (`Int`, none) that use cascade/relay must expose
  `id: ID` (Trinity: a UUID surrogate). This is a clear compile error, not silent.
- **Authoring SDK (done for Python):** the `fraiseql` Python SDK now emits `id: ID`
  for an identity field authored as `id: str`/`id: UUID` (`types.extract_field_info`),
  so the compiled schema is honest at the source and the compiler canonicalization
  becomes a backstop for older/hand-authored schemas. Confirmed against a real
  64-entity schema whose SDK emitted `id: String` on every type.
- **SpecQL (a separate authoring source) already works via the backstop.** SpecQL
  maps a `uuid` column to GraphQL `UUID` (`gql_type_mapper`), and the Trinity id is
  `uuid`-typed — so it emits `id: UUID`, which the compiler's `UUID → ID`
  canonicalization handles. No SpecQL change is required for conformance; emitting
  `id: ID` directly for the identity column would be an optional honesty improvement.
- **Future (graphql-cascade#5):** if a genuine need for heterogeneous-identity
  cascades emerges, the union model can relax this contract without breaking anyone
  (error → working is non-breaking).
