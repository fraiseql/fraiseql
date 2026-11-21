# gRPC Transport (Preview)

The `grpc` capability answers gRPC calls from the compiled schema's queries and
mutations, over a service described by a protobuf descriptor set you supply. Every call
runs on the same executor, with the same row policies and field rules, as a GraphQL
request.

**Preview.** It is behind the `grpc` Cargo feature, which no published artifact
compiles in, and **no server entry point mounts it**: neither the `fraiseql-server`
binary nor `fraiseql run` serves gRPC, even when it is compiled in and enabled, and
`/health` reports the enabled capability `started: false` rather than a transport
nobody can reach. An
embedder can mount the service itself (below). See the
[feature compatibility matrix](../features-compatibility-matrix.md) for what Preview
and Source mean.

## Configuration

```toml
# fraiseql.toml
[grpc]
enabled = true
descriptor_path = "proto/service.binpb"   # required when enabled
reflection = true                         # the default
stream_batch_size = 500                   # the default
```

| Key | Default | Meaning |
|-----|---------|---------|
| `enabled` | `false` | Writes `grpc_config` into the compiled schema |
| `descriptor_path` | none | A `FileDescriptorSet` (`protoc --descriptor_set_out`); an empty path is refused at compile time |
| `reflection` | `true` | The service keeps the descriptor bytes for a reflection service |
| `stream_batch_size` | `500` | Rows per batch on a server-streaming call; `0` is refused at compile time |
| `include_types` / `exclude_types` | `[]` | Accepted, but not read: they do not restrict the service today ([#1790](https://github.com/fraiseql/fraiseql/issues/1790)) |

A server built without the `grpc` feature refuses a compiled schema whose `grpc_config`
is enabled, at boot, naming the feature.

## How calls map to operations

The first service in the descriptor set is served. Each of its methods is matched to an
operation by name:

| Method | Operation |
|--------|-----------|
| `Get<Type>` | the single-item query, e.g. `GetUser` → `user` |
| `List<Types>` | the list query, e.g. `ListUsers` → `users`; a server-streaming method streams it in `stream_batch_size` batches, from one SQL statement and one snapshot |
| anything else | the mutation of the same name, first letter lowered: `CreateUser` → `createUser` |

A method with no matching operation is skipped and answers `UNIMPLEMENTED`; so is a
function-backed query. Column values are encoded into the response message by type:
text, numbers and booleans directly; temporal values, JSON and UUIDs as strings; a
`NULL` leaves the field unset.

## The row view

A read selects from `vr_<sql_source>`, a view the project writes with one column per
scalar field, named as the field. Each cell is read by its column's own PostgreSQL type:
a `timestamptz` is written with its offset (`2024-01-15T10:00:00+00:00`), a `timestamp`
without one (`2024-01-15T10:00:00`), a `date` as `2024-01-15`, and a `text` column as it
is stored.

A `DateTime`, `LocalDateTime`, `Date` or `Time` field is a proto `string` carrying its
scalar's format, as GraphQL and REST serve it (see
[temporal values](../features/temporal-values.md)). A value outside that format refuses the
read with `INTERNAL`, naming the field and the kind of value stored, in the words GraphQL
uses, on a unary and a streaming call alike. It is never rewritten: a `DateTime` stored
without an offset is not read in any time zone. Give each such column the type its scalar
reads: `timestamptz` for a `DateTime`, `timestamp` for a `LocalDateTime`, or the document's
text (`data->>'at'`). A cast in the view such as `(data->>'at')::timestamptz` reads an
offset-less text in the session's time zone before the server sees it; `compile --database`
checks the document view the row view reads from.

An `ID` field is a proto `string` too, read as text whatever its column holds: a string is
served as stored, an integer as its decimal text (`7` as `"7"`), as GraphQL serves them, and
any other kind (a fraction, a Boolean) refuses the read in GraphQL's words. A `UUID` field is
a `string` carrying the UUID's text.

The server refuses to start with a descriptor declaring one of those fields as anything but
a single `string`, naming the field and what to declare. A `google.protobuf.Timestamp` is an
instant, which a `LocalDateTime`, a `Date` or a `Time` is not; an integer field cannot carry
an `ID` that is not one.

A value of a kind its proto field cannot carry (a number at a `string`, a string at an
`int32`) refuses the read with `INTERNAL`, naming the field and its declared kind, never the
value.

A `Decimal` column carries the exact digits the database stored, scale included (`1.50`).
Declare its field as a `string`, or as a decimal message with a `string value` field, the
`google.type.Decimal` shape:

```proto
message Decimal { string value = 1; }
message Order { Decimal total = 1; }
```

A `Decimal` declared as a `double`, a `float` or an integer would lose its scale and any
digit past the seventeenth, so the server refuses to start with that descriptor, naming the
field.

A `NUMERIC` can also hold `NaN`, `Infinity` and `-Infinity`. A `string` field carries them
as that text. A decimal message has no form for them, as `google.type.Decimal` defines none,
so a read that meets one is refused with an error naming the field and the value, on a
unary and a streaming call alike
([#1819](https://github.com/fraiseql/fraiseql/issues/1819)). Declare the field `string` to
receive them.

## Authentication and errors

A call carries `authorization: Bearer <jwt>` metadata, checked by the server's OIDC
validator (`[auth]`). A missing or invalid token answers `UNAUTHENTICATED`; a revoked
one is refused when the embedder passes the server's revocation store. Identity
enrichment failures answer `PERMISSION_DENIED` or `UNAVAILABLE`, and the per-user (or,
unauthenticated, per-IP) rate limit answers `RESOURCE_EXHAUSTED`.

Errors map to status codes as follows, with sanitized messages:

| Engine error | gRPC status |
|--------------|-------------|
| validation | `INVALID_ARGUMENT` |
| unsupported | `UNIMPLEMENTED` |
| authorization | `PERMISSION_DENIED` |
| service unavailable | `UNAVAILABLE` |
| cost, response size, storage quota | `RESOURCE_EXHAUSTED` |
| anything else | `INTERNAL` |

## Mounting it as an embedder

`fraiseql_server::routes::grpc::build_grpc_service` returns the tonic service (and the
reflection descriptor bytes) for a compiled schema and an executor, or `None` when
gRPC is not enabled. Two things are the embedder's to get right:

- **Revocation.** Pass the store the HTTP transports check; with `None`, a revoked token
  keeps working over gRPC until it expires.
- **Tenancy.** The service answers every caller from the one executor it is given; it
  does not dispatch tenants ([#1550](https://github.com/fraiseql/fraiseql/issues/1550)).
  Do not mount it over the default executor for tenant traffic.

## See also

- The `grpc` capability fixture, `crates/fraiseql-server/tests/capabilities/grpc/`: a
  descriptor set and schema that enable gRPC, and the `/health` report that it is
  configured and not started.
- [GraphQL complexity limits](../security/graphql-complexity-limits.md), which apply to
  gRPC reads as to GraphQL.
