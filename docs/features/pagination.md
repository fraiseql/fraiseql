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

## Relay connections

A `relay = true` query is paginated by keyset on `relay_cursor_column`
(`pk_<snake_case(return_type)>`), which the compiler derives from the return type
and validates against the live relation under `--database`.

Keyset paging resumes from the last row's sort key, so its ordering ends with the
cursor column and nothing is appended after it. This is the durable answer to deep
pagination — an `OFFSET` of 200 000 still reads 200 000 rows to discard them,
whatever it is ordered by — and a client walking a large relation should prefer it.

A relay connection never accepts `limit`/`offset`, and never carries a
`pagination_order`.

## See also

* [ADR-0017 — entity identity contract](../adr/0017-entity-identity-contract.md)
* [`docs/authoring.md`](../authoring.md) — the decorator surface
