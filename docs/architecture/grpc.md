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
text, numbers and booleans directly; timestamps, dates, JSON and UUIDs as strings; a
`NULL` leaves the field unset. A `Decimal` column is sent as a `double` today, which
loses its scale and any digit past the seventeenth
([#1765](https://github.com/fraiseql/fraiseql/issues/1765)).

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
