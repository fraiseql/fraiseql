# Python Arrow Flight Client

Pulls GraphQL results out of a FraiseQL server as Arrow record batches over Flight
(gRPC): no JSON on the wire, no row-by-row deserialization. What comes back is a
`pyarrow.Table`, which hands straight to pandas, Polars, DuckDB or a Parquet file.

Siblings: [`../r`](../r) (R) and [`../rust/flight_client`](../rust/flight_client)
(Rust).

## Install

```bash
pip install -r requirements.txt
```

`pyarrow-core` is not enough — Flight lives in the full `pyarrow` wheel.

## Authenticate

Authentication is **not optional**, and this is the part the other two clients in
this tree get wrong ([#1200](https://github.com/fraiseql/fraiseql/issues/1200)). The
server's `do_get` validates a session token *before* it decodes the ticket, so a
call without one is refused whatever it asks for.

```bash
export FRAISEQL_FLIGHT_TOKEN='<a JWT from your identity provider>'
```

The client exchanges that for a short-lived session token over the Flight
`handshake` RPC, then sends `authorization: Bearer <session>` on every call. If your
deployment mints session tokens some other way, set
`FRAISEQL_FLIGHT_SESSION_TOKEN` and the handshake is skipped.

## Use

```bash
# A query, printed
python3 fraiseql_client.py query '{ users { id name email } }'

# With variables
python3 fraiseql_client.py query 'query($id: ID!) { user(id: $id) { name } }' \
    --variables '{"id": "58b923dc-1eff-4fb1-8357-ed9d7a30babd"}'

# Straight to Parquet, CSV or Arrow IPC — the extension picks the format
python3 fraiseql_client.py query '{ posts { title authorName } }' --output posts.parquet

# A pre-compiled Arrow view, if the deployment registers any
python3 fraiseql_client.py view va_orders --limit 100000
```

`--host` / `--port` (or `FRAISEQL_FLIGHT_HOST` / `FRAISEQL_FLIGHT_PORT`) point it
somewhere other than `localhost:50051`.

As a library:

```python
from fraiseql_client import FraiseQLFlightClient

client = FraiseQLFlightClient.connect("localhost", 50051, token=os.environ["FRAISEQL_FLIGHT_TOKEN"])
table = client.query("{ users { id name } }")

import polars as pl
df = pl.from_arrow(table)          # zero copy
```

## What the server actually returns

A **list** query becomes real Arrow columns, one per selected field:

```
$ python3 fraiseql_client.py query '{ users { id name email } }'
                 email                                    id           name
0    alice@example.com  58b923dc-…-ed9d7a30babd  Alice Johnson
```

A **single-object** query becomes one `result` column holding the JSON document.
The Flight server derives a columnar schema from result *rows*; with one object
there are none to derive from.

```
$ python3 fraiseql_client.py query 'query($id: ID!) { user(id: $id) { name } }' --variables '…'
                                       result
0  {"data":{"user":{"name":"Alice Johnson"}}}
```

Select a list if you want columns.

## What this client does not expose

Two Flight ticket types exist that a stock server will not serve, so there are no
subcommands for them:

- **`ObserverEvents`** — `Status::unimplemented`. Read historical events through the
  GraphQL API instead. (The R and Rust clients in this tree both still offer it.)
- **`BulkExport`** — fail-closed behind an operator allow-list
  (`with_bulk_export_tables`); a stock server answers `permission_denied`, and it
  applies no per-user row filtering when enabled.

## Errors

The client prints the server's gRPC status and exits non-zero. Two worth
recognising:

| what you see | what it means |
|---|---|
| `unauthenticated` | no session token, or it expired — session tokens are short-lived, so re-handshake |
| `not found` | the view name is not in the server's Arrow schema registry |

A malformed GraphQL query currently arrives as `internal` rather than
`invalid argument` — [#1201](https://github.com/fraiseql/fraiseql/issues/1201). Do
not build a retry policy that trusts that status yet.
