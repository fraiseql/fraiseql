# Pagination

FraiseQL serves two families of paginated read, and they answer different
questions. This page states what each guarantees, what it costs, and — for the
offset family — how to change the order its pages are cut in.

## The guarantee

> **Every paginated read has a total order.**

A `LIMIT`/`OFFSET` page is a slice of a *sequence*. A relation is not a sequence:
with no `ORDER BY`, or with one that leaves rows tied, PostgreSQL may return the
same rows in a different order for each page — so a client walking `?offset=`
sees some rows twice and never sees others, under `200`, with no error anywhere.

The order is decided by the **compiler**, per query, and recorded in the compiled
schema. Two fields carry it:

| Family | Field | Set when |
|---|---|---|
| Relay connections (`first`/`after`) | `relay_cursor_column` | `relay = true` |
| Offset pages (`limit`/`offset`) | `pagination_order` | the query paginates |

## Offset pagination

### What you get by default

A list query whose `auto_params` enable `limit` or `offset` is ordered by the
**entity identity** — `id`, the invariant every FraiseQL type carries
([ADR-0017](../adr/0017-entity-identity-contract.md)).

* Compiled **without** `--database`, the order is `data->>'id'` — correct for every
  type, and the most expensive of the available spellings.
* Compiled **with** `--database`, the compiler introspects the relation and uses a
  native column instead: `pk_<type>` when the relation exposes it, else `id`.

The cost is worth stating plainly. Measured against PostgreSQL 16, 200 000 rows,
a deep-offset page, relative to the same read with no ordering at all:

| ordering | planner cost | plan | vs unordered |
|---|---|---|---|
| none | 4 079 | Seq Scan | — |
| `data->>'id'` | 31 241 | parallel sort | **7.7×** |
| native `id` | 16 959 | Index Scan | **4.2×** |
| `pk_<type>` | 6 780 | Index Scan | **1.7×** |

That 4.6× spread between the three is why the decision is made where the schema
is visible: the only column a request-time rule can reach unaided is the worst
one. Compiling with `--database` is what turns 7.7× into 1.7×.

A client's own `sort` is never replaced. The identity is *appended*, so it breaks
only the ties the client's keys left — which is also what makes a non-unique sort
(`?sort=status`) safe to paginate.

### Declaring the order

```python
@fraiseql.query(sql_source="v_invoice", pagination_order="pk_invoice")
def invoices() -> list[Invoice]: ...
```

Two spellings, and absence is the third:

* **a column name** — order pages by this column. It must be **unique** over
  `sql_source`; a non-unique one leaves the order partial, which is the defect
  this exists to remove. Compiling with `--database` checks that the column
  exists (a missing one fails the compile); nothing in the catalog reports
  uniqueness for a view, so that half is taken on trust.
* **`"none"`** — emit no default ordering. See below.
* **absent** — the compiler derives the identity, which is what almost every
  query wants.

`"none"` is therefore not usable as a column name. A relation column actually
called `none` must be ordered by inside the view.

### Authoring it from an SDK

`pagination_order` is a **conformance construct** (`query_pagination_order`), so every
official SDK is held to it: the fixture authors all three states, the real compiler
compiles the result, and the compiled value is compared. Before that it reached the
compiled schema through every compile path with nothing holding an SDK to it, and exactly
one SDK carried it — by passing unknown keys through verbatim (#1305).

| SDK | spelling |
|---|---|
| Python | `@fraiseql.query(pagination_order="created_at")` |
| TypeScript | `registerQuery(…, { paginationOrder: "created_at" })` |
| Go | `.PaginationOrder("created_at")` |
| PHP | `->paginationOrder('created_at')` |
| Java | `.paginationOrder("created_at")` |
| C# | `.PaginationOrder("created_at")` |
| F# | `\|> QueryBuilder.paginationOrder "created_at"` |
| Elixir | `fraiseql_query :invoices, pagination_order: "created_at"` |
| Ruby | `schema.query :invoices, pagination_order: "created_at"` |
| Dart | `schema.query('invoices', paginationOrder: 'created_at')` |
| Rust | not authorable — the SDK is field-level-RBAC focused and ships no query builder at all, a gap declared in `conformance/manifest.json` |

Omitting the call is the third state, and it is not the same as passing an empty string:
an SDK that defaults the key to `""` or to a guessed column is overriding a decision the
author deliberately left to the compiler. That is what the fixture's third query exists to
catch.

### The opt-out, and when you need it

A view may carry its own `ORDER BY` — and that is FraiseQL's own documented
remedy for a query with `order_by = false`. A default page ordering *replaces*
that order rather than adding to it, so a self-ordering view needs the opt-out:

```python
@fraiseql.query(sql_source="v_feed_ranked", pagination_order="none")
def feed() -> list[FeedItem]: ...
```

It is **declared rather than inferred** because the compiler cannot read a view's
body. Under `--database` it does read it, and a view containing `ORDER BY` whose
query has not declared either way raises a compile warning naming this remedy —
advisory, because `pg_get_viewdef` cannot tell a top-level `ORDER BY` from one
inside a window frame or a subquery, where a page ordering is entirely correct.

Declaring `"none"` also silences the compile warning about non-deterministic
pages: the author answered the question it asks.

### Deployment posture

```toml
[query_defaults]
pagination_order = "identity"   # default | "refuse" | "allow"
```

| posture | derives | `pagination_order = "none"` |
|---|---|---|
| `identity` | the entity identity | permitted |
| `refuse` | the entity identity | **compile error** |
| `allow` | nothing | permitted (and redundant) |

`refuse` is for a deployment in which every paginated read has a total order and
there are no exceptions, including the self-ordering-view one. `allow` restores
the pre-2.15.0 behaviour — and the defect with it, which is why the compile warns
for every query it leaves unordered.

All three are resolved at compile time. There is no runtime switch: the compiled
`pagination_order` is the answer, and a second switch elsewhere could disagree
with it.

### Where it applies

The ordering is applied to a read that is **actually paged** — one carrying
`limit` or `offset`. A read with neither returns the whole filtered relation in
one answer; it has no second page to overlap with, and sorting it would be a cost
with no beneficiary.

Over REST this distinction is invisible: `resolve_pagination` fills an absent
`?limit=` with `default_page_size`, so **every REST list route is a page** and is
ordered. Over GraphQL, a query that passes neither `limit` nor `offset` is not.

Exports (`Accept: application/x-ndjson`, `text/csv`, XLSX) read the whole filtered
relation rather than a page, and are unaffected.

### Deep offset

An `OFFSET n` page reads `n` rows to throw them away, whatever it is ordered by and
whatever index it walks. Measured on PostgreSQL 18, 200 000 rows, a page of 20 ordered by
the primary key:

| page | rows read | buffers |
|---|---|---|
| `OFFSET 0` | 20 | 4 |
| `OFFSET 10 000` | 10 020 | 164 |
| `OFFSET 100 000` | 100 020 | 1 610 |
| `OFFSET 199 980` | 200 000 | 3 216 |
| keyset, `WHERE pk > 199 980` | 20 | 7 |

A client walking a large list by offset pays for every page before the one it reads. A
[relay connection](#relay-connections) pages by cursor instead: unordered, it reads only its
page at any depth, so it is the path for deep walks (declare the query `relay = true` and page
it with `first`/`after`). Under an `orderBy`, a cursor page costs what the ordering costs to
walk to (see [Paging under an `orderBy`](#paging-under-an-orderby)).

An operator can refuse deep offset pages outright:

```toml
[validation]
max_offset = 10000   # unset by default: no ceiling
```

An offset beyond it is refused before any statement, with a message naming the relay
connection. It holds every offset a client chooses: GraphQL `offset:`, REST `?offset=`, gRPC
and MCP reads, aggregate and window `offset`, and `@stream`. A stream pages its list in
batches at increasing offsets: one whose own `limit` takes its last batch past the ceiling is
refused before it opens, and one with no `limit` ends, with the refusal as its last payload,
when a batch would start past it. `FRAISEQL_MAX_OFFSET` overrides the compiled value (`0` or
`none` lifts it). The setting belongs in `fraiseql.toml`: a server configuration's
`[validation] max_offset` is refused at boot, since a hot reload would drop it.

## Sort keys

A query with `orderBy` (a list or a relay connection) takes a list of
`{ field, direction }` items, and `field` is an enum of exactly the keys it accepts:

```graphql
enum ItemOrderByField { id name rank label }
input ItemOrderByInput { field: ItemOrderByField!, direction: SortDirection = ASC }
```

The keys are the type's fields that order meaningfully (not a relation, a list, a `JSON`
document or a vector, whose order would be that of their serialized text), plus the
native columns the query reads with `--database`, named by their key. The engine accepts
exactly these on every transport (GraphQL, REST `?sort=`, gRPC), and the enum lists exactly
these: one set, so the schema never advertises a key the engine refuses.

When every query returning an entity accepts the same keys, they share
`{Entity}OrderByField` and `{Entity}OrderByInput`. When they differ (one reads a native
column the others do not), each query has its own, named after it
(`ItemsByRankOrderByField`), so no query is offered a sibling's key. A query whose type has
no sortable key publishes no `orderBy`.

## Relay connections

A `relay = true` query is paginated by keyset on `relay_cursor_column`
(`pk_<snake_case(return_type)>`), which the compiler derives from the return type
and validates against the live relation under `--database`.

Keyset paging resumes from the last row's sort key, so its ordering ends with the
cursor column and nothing is appended after it. This is the durable answer to deep
pagination — an `OFFSET` of 200 000 still reads 200 000 rows to discard them,
whatever it is ordered by — and a client walking a large relation should prefer it.

A connection takes the same `where` and `orderBy` a list does, published with the same
types (`{Entity}WhereInput`, `[{Entity}OrderByInput!]`), when the query enables them.

### Paging under an `orderBy`

A connection with an `orderBy` is ordered by its keys, then by the cursor column, which
makes the order total. A page after (or before) a cursor resumes past the cursor row's
key values, key by key in each key's direction, then past its cursor column. NULLs sit
where PostgreSQL puts them: last for `ASC`, first for `DESC`. Each key compares as the
`ORDER BY` sorts it: by its native column and type, by its typed JSON value, under the
request locale's collation, and by a localized field's label.

Such a cursor carries the row's key values and a fingerprint of the ordering, so it
resumes only the ordering it came from. Under another `orderBy`, another direction,
another locale, or none, it is refused with a message saying to request the first page
again. An unordered connection's cursor is the cursor column alone.

The resume predicate is spelled out key by key (an `OR` per key), which is what lets keys
mix directions and hold NULLs. PostgreSQL cannot seek an index with it, though: at best it
walks an index that matches the ordering from the start and filters. So a page deep into a
large connection under an `orderBy` costs about what an offset page that deep costs.

**When a page seeks instead (#1533).** When every key is a native column PostgreSQL proves
`NOT NULL`, and every key is ascending, the predicate is one row comparison,
`(key, cursor_column) > ($1, $2)`, which PostgreSQL seeks an index on `(key, cursor_column)`
with. Measured on PostgreSQL 18 (60 000 rows, a page 55 000 deep): the expanded form removed
55 001 rows by filter over 55 364 buffers; the row comparison read 13. The conditions, and
why each is required:

* **A native column**: a key read from the document (`data->>'key'`) can be absent, so it can
  be NULL. A key is native when the query reads it as a column, which `compile --database`
  records for the query's arguments and inject parameters that name a column.
* **Proven `NOT NULL`**: `compile --database` reads the catalog. Only a base relation proves
  it: PostgreSQL 18 reports every column of a **view** nullable, even over a `NOT NULL`
  column, so a view-backed connection keeps the expanded form. Back the connection with a
  table (a `tv_` table) to seek. A NULL in a row comparison would drop its row from every page
  silently, which is why a column nothing proves is never compared as a row.
* **Ascending**: the cursor column always sorts ascending, and one row comparison reads every
  term in one direction.

Any other ordering keeps the expanded form, and is exact. An unordered connection always
seeks on its cursor column.

A relay connection never accepts `limit`/`offset`, and never carries a
`pagination_order`.

### Refetching a node

`node(id:)` takes the `id` an object returned, its UUID, so `node(id: x.id)` gives `x` back:

```graphql
{ node(id: "6f1c…") { ... on User { id name } } }
```

The id's type is found among the `relay` types, each probed with the scoping its own query
applies (`requires_role`, `requires_actor`, the authorizer, RLS, `inject_params`). An id
the caller may not read under any of them is `null`. Some schemas expose one entity as two
relay types, such as `User` and `UserCard` over the same table. An id both readable types
hold is refused rather than guessed: pass `base64("UserCard:6f1c…")` to name the type. That
typed form is always accepted.

## See also

* [ADR-0017 — entity identity contract](../adr/0017-entity-identity-contract.md)
* [`docs/authoring.md`](../authoring.md) — the decorator surface
