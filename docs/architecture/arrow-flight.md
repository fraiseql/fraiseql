# Arrow Flight

The `arrow_flight` capability serves query results as Apache Arrow record batches over
the [Arrow Flight](https://arrow.apache.org/docs/format/Flight.html) gRPC protocol, for
clients that want columns rather than JSON: dataframes, notebooks, warehouses. It also
accepts uploads into tables you allow-list.

It is behind the `arrow` Cargo feature, which the `-full` and `-platform` release
artifacts compile in, and runs on PostgreSQL. The `fraiseql-server` binary and
`fraiseql run` start it. `serve_on_listener` does not: an embedder that hands the server
its listener gets no port it did not choose, so it gets no Flight listener either.

## Configuration

```toml
# server.toml
flight_bind_addr = "127.0.0.1:50051"   # the default: loopback only
flight_views = ["va_public_metrics"]   # the default is none
flight_upload_tables = []              # the default: Upload disabled

[auth]
issuer = "https://id.example.com/"     # Flight trusts the server's OIDC validator
audience = "analytics"
```

| Key | Default | Meaning |
|-----|---------|---------|
| `flight_bind_addr` | `127.0.0.1:50051` | Address of the Flight listener. Binding another interface is an explicit choice. `--flight-bind-addr` and `FRAISEQL_FLIGHT_BIND_ADDR` override it; a malformed value stops startup. |
| `flight_views` | `[]` | Views an `OptimizedView` ticket may read. Each must be a plain identifier, or boot fails. Each is typed from one of its rows at boot, so a view that is empty or unreadable then is not served (a warning says so). |
| `flight_upload_tables` | `[]` | Tables a client may `DoPut` into. Empty disables uploads. |

The handshake also needs `FLIGHT_SESSION_SECRET` in the environment, generated with
`openssl rand -hex 32`. It signs the short-lived session token a client uses after the
handshake. Without it, the server starts, logs a warning, and fails every handshake.

### What a view and an upload bypass

Read this before naming anything in either list:

- **`flight_views`**: an `OptimizedView` ticket reads the view **whole**, with no row
  policy, field rule or authorizer
  ([#716](https://github.com/fraiseql/fraiseql/issues/716)). Every principal that
  completes the handshake can read every row and column of every view named here. Name
  only views whose entire contents all of them may read.
- **`flight_upload_tables`**: an upload is a client-directed `INSERT` that does not go
  through the mutation pipeline, so no operation authorizer or field rule applies.
  - Name only tables every authenticated Flight client may write.
  - The rows and their `core.tb_entity_change_log` entries are written in one
    transaction, so subscribers and CDC see an upload like any other write.
  - The server refuses to boot with uploads enabled alongside `cache_enabled = true`:
    an upload invalidates no cached read.
  - It also refuses uploads alongside `[tenancy.runtime] enabled`: an upload writes
    through the server-wide pool, not a tenant's
    ([#1550](https://github.com/fraiseql/fraiseql/issues/1550)).

## Authentication

1. The client sends `Handshake` with the payload `Bearer <jwt>`. The token is checked by
   the server's OIDC validator, built from `[auth]`: issuer, audience and the issuer's
   published keys. The HS256 bearer mode of the HTTP transports does not apply to
   Flight.
2. The handshake answers with a session token, signed with `FLIGHT_SESSION_SECRET` and
   valid for five minutes. Every later call carries it as
   `authorization: Bearer <session token>`.

With no `[auth]`, the server logs that Flight runs without authentication, and the
handshake still refuses every client: there is no unauthenticated Flight access.
Identity enrichment applies to Flight principals as it does to HTTP ones.

## Calls and tickets

A ticket is a JSON object tagged by `type`:

| Ticket | Serves |
|--------|--------|
| `{"type":"GraphQLQuery","query":"…","variables":…}` | A GraphQL query's result, executed through the same policy as `/graphql`: tenant dispatch, trusted documents, per-tenant quotas |
| `{"type":"OptimizedView","view":"…"}` | A view named in `flight_views` (above) |
| `{"type":"BatchedQueries",…}`, `{"type":"BulkExport",…}`, `{"type":"ObserverEvents",…}` | Several queries, a table export, and observer events |

- **`DoGet`** streams a ticket's batches. By default 50 streams run at once; the next
  one answers `RESOURCE_EXHAUSTED`.
- **`GetFlightInfo`** and **`GetSchema`** take the ticket in the descriptor's path.
  `PollFlightInfo` answers at once, complete.
- **`DoPut`** uploads into an allow-listed table. Any other table answers
  `PERMISSION_DENIED`.
- **`DoExchange`** carries queries and uploads.
- **`DoAction`** offers `HealthCheck`, plus `ClearCache` and `RefreshSchemaRegistry`,
  which need the `admin` scope.
- **`ListFlights`** does not list the declared `flight_views` today
  ([#1789](https://github.com/fraiseql/fraiseql/issues/1789)). A client has to know the
  view's name.

How a column is typed depends on where its rows come from.

A `GraphQLQuery` ticket's result is typed from its JSON values:

- A string maps to `Utf8`, and so does an array, an object, or a column that is null in
  every row.
- A whole number maps to `Int64`, and any other number to `Float64`.
- A boolean maps to `Boolean`.

A GraphQL `Decimal` travels as a string of its digits, so it arrives as `Utf8`, exact.

`OptimizedView`, `BatchedQueries` and `BulkExport` read rows straight from PostgreSQL,
and a `numeric` column is typed by what PostgreSQL says it is, never by a value
([#1766](https://github.com/fraiseql/fraiseql/issues/1766)):

- `numeric(p, s)` with `p` at most 38 and `s` from 0 to `p` maps to `Decimal128(p, s)`.
- Any other `numeric` maps to `Utf8`, holding the digits PostgreSQL printed. That covers
  an unconstrained `numeric`, a wider one, one with a negative scale, and a column computed
  in a view (`amount * 2`), which PostgreSQL reports as unconstrained.
- No `numeric` maps to `Float64`.

`Decimal128` has no `NaN` or infinity, so a `numeric(p, s)` that holds `NaN` refuses the
read, naming the column and the value. A `Utf8` column carries `NaN`, `Infinity` and
`-Infinity` as that text.

The precision and scale come with the statement's own result description, for a view's
columns as for a table's, so no catalog is queried. The schema `GetSchema` and
`GetFlightInfo` advertise for a view is typed the same way. Every other column is typed
from its values, as a `GraphQLQuery` result is.

A schema an embedder registers for a view with `SchemaRegistry::register` must type each
of the view's `numeric` columns as `Decimal128` or `Utf8`. A float or an integer would round
the digits, so `DoGet` refuses the view, naming the column and the type to register.

## See also

- [Feature compatibility matrix](../features-compatibility-matrix.md)
- [Enriched identity](enriched-identity-rls.md)
- `crates/fraiseql-arrow/README.md`: the library, for embedders
- The `arrow_flight` capability fixture,
  `crates/fraiseql-server/tests/capabilities/arrow_flight/`. Through the server binary,
  an OIDC-authenticated client completes the handshake and reads a declared view with
  `DoGet`.
