# AuthoringIR Format Reference

This document is a **practical guide for code generator authors** building tools
that emit `schema.json` (the AuthoringIR format). For the complete field
reference, see [architecture/intermediate-schema.md](architecture/intermediate-schema.md).

## Overview

The AuthoringIR is a JSON file (`schema.json`) that describes your GraphQL API.
Language SDKs (Python, TypeScript, Go, etc.) emit this file from decorator-based
definitions. The FraiseQL compiler reads it and produces `schema.compiled.json`
for the runtime server.

```
Your code  →  SDK  →  schema.json  →  fraiseql-cli compile  →  schema.compiled.json
```

---

## Type Format Rules

### Supported type names

| Type name    | GraphQL type   | Notes                          |
|--------------|----------------|--------------------------------|
| `"String"`   | `String`       |                                |
| `"Int"`      | `Int`          | 32-bit signed integer          |
| `"Float"`    | `Float`        | Double precision               |
| `"Boolean"`  | `Boolean`      |                                |
| `"ID"`       | `ID`           | UUID v4 in FraiseQL            |
| `"DateTime"` | `DateTime`     | ISO 8601                       |
| `"Date"`     | `Date`         | ISO 8601 date-only             |
| `"Time"`     | `Time`         | ISO 8601 time-only             |
| `"Json"`     | `JSON`         |                                |
| `"UUID"`     | `UUID`         |                                |
| `"Decimal"`  | `Decimal`      | Arbitrary-precision numeric    |
| `"Vector"`   | `[Float!]!`    | pgvector embedding — requires `vector_config` |
| `"BitVector"`| `String`       | pgvector binary vector — requires `vector_config` |
| `"HalfVector"` | `[Float!]!`  | pgvector `halfvec` — requires `vector_config` |
| `"SparseVector"` | `String`   | pgvector `sparsevec` — requires `vector_config` |

Any unrecognized type name (e.g. `"User"`, `"Post"`) is treated as a reference
to a GraphQL **object type** defined elsewhere in the schema.

### Vector fields (#386)

A `"Vector"` field **must** carry a `vector_config` object (and `vector_config`
on any other type is a compile error):

```json
{
  "name": "embedding",
  "type": "Vector",
  "nullable": false,
  "vector_config": {
    "dimensions": 1536,
    "index_type": "hnsw",
    "distance_metric": "cosine"
  }
}
```

`dimensions` is required (≥ 1); `index_type` (`"hnsw"` | `"ivf_flat"` | `"none"`,
default `"hnsw"`) and `distance_metric` (`"cosine"` | `"l2"` | `"inner_product"`,
default `"cosine"`) drive the emitted index DDL and the default metric of the
`nearest` query argument. The backing view must expose the vector as a native
`vector(N)` column named after the field (snake_case) — the JSONB `data` payload
does not carry embeddings.

### Distance fields (#959)

A `Float` field may declare `vector_distance`, naming the vector field whose
search distance it carries:

```json
{ "name": "similarity", "type": "Float", "nullable": false,
  "vector_distance": "embedding" }
```

It is projected from the `nearest` search's own distance expression, and is
refused on a query that did not run that search. See
[vector search](operations/vector-search.md).

### Binary vector fields (#959)

A `"BitVector"` field carries the same `vector_config`, where `dimensions`
counts **bits** and `distance_metric` is `"hamming"` or `"jaccard"` — the two
metrics pgvector defines over `bit` values:

```json
{
  "name": "fingerprint",
  "type": "BitVector",
  "nullable": false,
  "vector_config": {
    "dimensions": 768,
    "index_type": "hnsw",
    "distance_metric": "hamming"
  }
}
```

A metric of the wrong kind for the field type is a compile error in both
directions, as is `"ivf_flat"` with `"jaccard"` (pgvector ships
`bit_jaccard_ops` for `hnsw` only). The backing view exposes the field as a
native `bit(N)` column; in GraphQL it is a `String` of `0`/`1` characters. See
[vector search](operations/vector-search.md).

### Half-precision and sparse vector fields (#959)

`HalfVector` and `SparseVector` carry the same `vector_config` and the same
float metrics as `Vector`; what differs is the column behind them and, for
sparse vectors, how a value is written:

```json
{ "name": "compact", "type": "HalfVector", "nullable": true,
  "vector_config": { "dimensions": 1536, "index_type": "hnsw", "distance_metric": "l2" } },
{ "name": "terms", "type": "SparseVector", "nullable": true,
  "vector_config": { "dimensions": 30000, "index_type": "none", "distance_metric": "cosine" } }
```

`HalfVector` is a `halfvec(N)` column with the same `[Float!]!` GraphQL surface
as `Vector`. `SparseVector` is a `sparsevec(N)` column and a GraphQL `String`,
in pgvector's own `{1:0.5,7:0.25}/1000` text form — index/value pairs and the
dimension count. `index_type = "ivf_flat"` on a sparse vector is a compile
error: pgvector ships no `sparsevec_*` operator class for it. See
[vector search](operations/vector-search.md).

### SDK support

All eleven official SDKs author `vector_config` and `vector_distance`, and the
cross-SDK conformance suite's `vector_fields` construct holds them to it. Each
SDK writes `index_type` and `distance_metric` into the emitted document even
when the author leaves them at their default, so the IR states which index and
which metric the column will get. Which *combinations* pgvector defines is the
compiler's to know, not an SDK's.

### Nullability is separate from the type

Nullability is **always** controlled by the `nullable` field, **not** by
appending `!` to the type name:

```json
{
  "name": "id",
  "type": "ID",
  "nullable": false
}
```

**Do not use `"ID!"`**. While the compiler will accept it (stripping the `!`
and emitting a warning), the canonical format omits the non-null marker entirely.

### Common mistakes

| Wrong                          | Correct                       | Why                                  |
|-------------------------------|-------------------------------|--------------------------------------|
| `"type": "ID!"`               | `"type": "ID"`                | `!` is redundant; use `nullable`     |
| `"type": "String!"`           | `"type": "String"`            | Same                                 |
| `"type": "[String!]!"`        | `"type": "String"` + list     | Lists are expressed via `returns_list` on queries, not in the type string |
| `"field_type": "String"`      | `"type": "String"`            | JSON key is `"type"`, not `"field_type"` |
| `"arg_type": "Int"`           | `"type": "Int"`               | JSON key is `"type"`, not `"arg_type"` |
| `"default_value": 42`         | `"default": 42`               | JSON key is `"default"`, not `"default_value"` |

### Rich scalars

The compiler recognizes 49+ built-in rich scalar types (case-insensitive matching):

- **Contact**: `Email`, `PhoneNumber`, `URL`, `DomainName`, `Hostname`
- **Location**: `PostalCode`, `Latitude`, `Longitude`, `Coordinates`, `Timezone`
- **Financial**: `IBAN`, `CUSIP`, `CurrencyCode`, `Money`, `StockSymbol`
- **Identifiers**: `Slug`, `SemanticVersion`, `APIKey`, `VIN`
- **Networking**: `IPAddress`, `IPv4`, `IPv6`, `MACAddress`, `CIDR`
- **Content**: `Markdown`, `HTML`, `Cron`, `Regex`, `Color`

Rich scalar names are recognized as valid field types (they compile to string
storage). They do **not** carry type-specific filter operators: the
`<RichType>WhereInput` surface was removed in v2.15 because the runtime WHERE
parser never served it (#869) — filtering uses the standard operator set.

---

## Key JSON Serialization Rules

Several fields use `#[serde(rename = ...)]` — the **JSON key differs from the
Rust field name**:

| Rust field                    | JSON key   | Context                        |
|-------------------------------|------------|--------------------------------|
| `field_type`                  | `"type"`   | `IntermediateField`, `IntermediateInputField` |
| `arg_type`                    | `"type"`   | `IntermediateArgument`         |
| `type_condition`              | `"on"`     | `IntermediateFragment`         |
| `where_clause`                | `"where"`  | `IntermediateAutoParams`       |

---

## Minimal Valid `schema.json`

The smallest schema that compiles successfully:

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "id",    "type": "ID",     "nullable": false },
        { "name": "email", "type": "String",  "nullable": false }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "sql_source": "v_users"
    }
  ]
}
```

## Query with Arguments

```json
{
  "name": "user",
  "return_type": "User",
  "returns_list": false,
  "nullable": true,
  "sql_source": "v_users",
  "arguments": [
    {
      "name": "id",
      "type": "ID",
      "nullable": false
    }
  ]
}
```

## List Query with a Total Count

A bare `[T]` list has nowhere to hang a total, so an offset-paginated client cannot compute
a page count. `"count": true` emits a sibling query — `<name>Count(where): Int!` — that
answers it (#938):

```json
{
  "name": "users",
  "return_type": "User",
  "returns_list": true,
  "sql_source": "v_users",
  "count": true
}
```

compiles to both `users(where, orderBy, limit, offset): [User!]!` and
`usersCount(where): Int!`.

The count reflects the **whole filtered set**, independent of `limit`/`offset` — that is
the point of it — so the sibling takes `where` and no pagination arguments. It is otherwise
derived from the list definition, inheriting the same `sql_source`, `inject`,
`requires_role` and declared arguments: a count answers "how many rows match?" without
returning a row, so one that dropped the tenant filter would disclose another tenant's row
total while appearing to leak nothing.

Opt-in per query, because the extra `SELECT COUNT(*)` scans the full filtered set and is
wasted on any list not rendered with page numbers. Refused at compile time when
`returns_list` is false, when `sql_source` is absent, and when `relay` is true — a Relay
connection already exposes `totalCount` over the same rows.

## Mutation with Cache Invalidation

```json
{
  "name": "createUser",
  "return_type": "User",
  "operation": "create",
  "sql_source": "fn_create_user",
  "arguments": [
    { "name": "email", "type": "String", "nullable": false },
    { "name": "name",  "type": "String", "nullable": true  }
  ],
  "invalidates_views": ["v_users"]
}
```

## Enum Type

```json
{
  "name": "OrderStatus",
  "values": [
    { "name": "PENDING" },
    { "name": "SHIPPED", "description": "Package dispatched" },
    { "name": "CANCELLED", "deprecated": { "reason": "Use REFUNDED" } }
  ]
}
```

## Relationships (#1266)

A type may declare relationships the REST transport embeds — `?select=orders(id,total)`,
`?select=orders.count`, `?orders.status=paid`. They are also advertised per type in the
served OpenAPI document and emitted by the client generator as `relationships.{ts,rs,go,py}`.

```json
{
  "name": "User",
  "sql_source": "v_user",
  "fields": [{ "name": "id", "type": "ID", "nullable": false }],
  "relationships": [
    {
      "name": "orders",
      "target_type": "Order",
      "cardinality": "OneToMany",
      "foreign_key": "fk_user",
      "referenced_key": "id"
    }
  ]
}
```

`foreign_key` and `referenced_key` are SQL **column** names, not the declared field names.
Under the default `camelCase` naming convention the column `fk_user` is published as the
field `fkUser`, and the compiler resolves one to the other — so a generator that emits the
declared name here produces a schema no embed can follow.

Which side each key is read from swaps with the cardinality:

| `cardinality` | read off the **declaring** type's row | filtered on the **target** row |
|---|---|---|
| `OneToMany` | `referenced_key` | `foreign_key` |
| `ManyToOne` | `foreign_key` | `referenced_key` |
| `OneToOne` | `foreign_key` | `referenced_key` |

`fraiseql compile` **refuses** a relationship no embed could follow, rather than emitting
one that resolves to `[]` or `null` under a 200: an undeclared `target_type`, a join column
naming no field of the side it is read from, a `target_type` returned by no **list** query,
an empty key, or one name declared twice on a type. The compiled schema is checked again at
load, so a hand-edited artifact cannot carry one either.

## Server-Injected Parameters

Use `inject_params` to pass server-side context (e.g. JWT claims) as SQL parameters
without exposing them as GraphQL arguments:

```json
{
  "name": "myOrders",
  "return_type": "Order",
  "returns_list": true,
  "sql_source": "v_orders",
  "inject_params": {
    "org_id": "jwt:org_id"
  }
}
```

The value may also be given in the expanded form the SDKs emit, which is what
`fraiseql compile` writes back out:

```json
"inject_params": {
  "org_id": { "source": "jwt", "claim": "org_id" }
}
```

This key is the **same name the compiled schema uses**. It was previously `inject`
here and `inject_params` there, and three SDKs copied the compiled name into their
intermediate output — where it bound to an empty map, compiling queries with no
tenant predicate and reporting success. The compiler now refuses a schema that uses
the old `inject` key rather than silently ignoring it.

## Subscription

```json
{
  "name": "onUserCreated",
  "return_type": "User",
  "topic": "user.created"
}
```

---

## Python SDK Output Format

The Python SDK decorators emit `schema.json` automatically. Here's how
decorator code maps to JSON:

```python
@fraiseql.type
class User:
    """A platform user."""
    id: int           # → { "name": "id",   "type": "Int",    "nullable": false }
    email: str        # → { "name": "email", "type": "String", "nullable": false }
    name: str | None  # → { "name": "name",  "type": "String", "nullable": true  }

@fraiseql.query(sql_source="v_users")
def users() -> list[User]:
    ...

@fraiseql.query(sql_source="v_users")
def user(id: int) -> User | None:
    ...

@fraiseql.mutation(sql_source="fn_create_user", invalidates=["v_users"])
def create_user(email: str, name: str | None = None) -> User:
    ...
```

---

## Validation & Error Messages

The compiler validates the schema and produces clear error messages. Common errors:

| Error message | Cause | Fix |
|---------------|-------|-----|
| `unknown type 'ID!'` | `!` in type string (before v2.2) | Remove `!`, use `"nullable": false` |
| `Query 'X' references unknown type 'Y'` | `return_type` doesn't match any `types[].name` | Check spelling, add the type definition |
| `Failed to convert type 'X'` | Invalid field in type definition | Check field types are valid |

---

## See Also

- [architecture/intermediate-schema.md](architecture/intermediate-schema.md) — complete field reference
- [architecture/overview.md](architecture/overview.md) — end-to-end architecture
