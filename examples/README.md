# FraiseQL Examples

Every `cd` in this file lands somewhere that exists, and every example it names has
been run. Where something does not work, it says so and names the issue.

FraiseQL compiles a schema to SQL ahead of time. Python (or TypeScript) declares the
types, `fraiseql compile` turns them into `schema.compiled.json`, and the Rust
runtime executes queries against it. That shape is why most examples here have a
`schema.py`, a `sql/setup.sql` and no application server of their own.

## Start here

```bash
createdb fraiseql_example
psql -v ON_ERROR_STOP=1 -d fraiseql_example -f basic/sql/setup.sql
export DATABASE_URL=postgresql://localhost/fraiseql_example

cd basic-query && ./run.sh
```

`ON_ERROR_STOP=1` is not decoration: without it psql prints an error, keeps going,
and exits 0 on a half-loaded schema.

---

## Rust examples

Six runnable crates, in the order they are worth reading. Each is a workspace member,
so `cargo run` works from inside it; each also has a `run.sh` that compiles the
schema it reads first, because `schema.compiled.json` is a build artifact and is
gitignored.

| Example | Needs a database | Shows |
|---------|:----------------:|-------|
| [`basic-query/`](basic-query) | yes | load a compiled schema, connect, execute, print — four calls |
| [`error-handling/`](error-handling) | yes | the three places a query can fail, including the one that raises nothing |
| [`complex-queries/`](complex-queries) | yes | nesting, variables, filtering, ordering, two root fields in one operation |
| [`performance/`](performance) | yes | repeated timings, a phase trace that accounts for its own total, structured SQL logs |
| [`subscriptions/`](subscriptions) | no | the subscription manager end to end in one process |
| [`authentication/`](authentication) | no | mint a JWT, validate it, and five tokens that must be rejected |

```bash
cd examples/basic-query
./run.sh
```

`basic-query`, `error-handling` and `performance` read `examples/basic`;
`complex-queries` reads `examples/ecommerce`; `subscriptions` reads
`examples/streaming`. Load that example's `sql/setup.sql` first — each README says
which.

---

## Schema examples

Three schemas of increasing size. Each carries `schema.py` (authoring),
`schema.json` (generated), `sql/setup.sql` and a `queries/` directory of runnable
operations.

### [`basic/`](basic) — a blog

`User` and `Post`, one-to-many, denormalized author identity. 3 users, 4 posts.

### [`ecommerce/`](ecommerce) — a catalogue and its orders

`Category`, `Product`, `Customer`, `Order`, `OrderItem`, plus the `OrderStatus`
enum. Nested objects and a nested list, all built by the views, so
`order { items { product { name } } }` is still one SQL statement. 5 categories,
12 products, 5 customers, 7 orders.

### [`streaming/`](streaming) — events, messages, presence, metrics

Four types and the four subscriptions that push them.

Run any of their queries without a server:

```bash
cd examples/ecommerce
psql -v ON_ERROR_STOP=1 -d ecommerce_example -f sql/setup.sql
cargo run -p fraiseql-cli -- compile schema.json -o schema.compiled.json
DATABASE_URL=postgresql://localhost/ecommerce_example \
  cargo run -p fraiseql-cli -- query "$(cat queries/04-order-analysis.graphql)"
```

`fraiseql query` boots the compiled schema in-process, runs one operation and exits
non-zero if it does not resolve. It is the cheapest end-to-end check there is.

### Docker

There is no per-example Docker stack. `docker/docker-compose.{demo,examples}.yml`
used to be one, and were deleted on 2026-08-28 rather than repaired: neither could
start, and neither had ever been run by CI. They pointed `FRAISEQL_SCHEMA_PATH` at a
`schema.compiled.json` no step builds ([#1202](https://github.com/fraiseql/fraiseql/issues/1202)),
built an `admin-dashboard/Dockerfile` that exists nowhere
([#1189](https://github.com/fraiseql/fraiseql/issues/1189)), ran a
`graphql/graphql-playground` image Docker Hub no longer serves, and — like every
other stack in the repository — ran the server in production mode with no
`fraiseql.toml`, so it exited on its first line.

Use the `psql` + `fraiseql query` path above for examples. For a Docker deployment,
the root [`docker-compose.yml`](../docker-compose.yml) is the one stack CI brings up
and queries on every push; point it at your own compiled schema.

---

## Arrow Flight clients

Pull query results as Arrow record batches over gRPC — no JSON on the wire, no
row-by-row deserialization.

Authentication is **not optional**. The server's `do_get` validates a session token
before it decodes the ticket, so every call needs one.

### [`python/`](python) — pyarrow

```bash
cd examples/python
pip install -r requirements.txt
export FRAISEQL_FLIGHT_TOKEN='<a JWT from your identity provider>'
python3 fraiseql_client.py query '{ users { id name } }'
python3 fraiseql_client.py query '{ posts { title } }' --output posts.parquet
```

Handshake, session token, `query` and `view`; Parquet, CSV and Arrow IPC output.
Verified against a live Flight server.

### [`rust/flight_client/`](rust/flight_client)

Handshake, session token and the `authorization` header, implementing the protocol
`crates/fraiseql-arrow/tests/flight_auth_test.rs` pins. It compiles on every push —
`tools/check-example-crates-compile.sh` builds it, tests included — but nothing runs
it against a live server. `python/` is the client that has been.

### [`r/`](r)

The same protocol, driving pyarrow through reticulate: `arrow::flight_get()` has no
parameter for per-call gRPC metadata, so the `arrow` package cannot send the header
the server requires.

Two levels of verification, both reproducible (#1260):

* `tools/check-r-examples-parse.sh` parses it under a real R on **every push**.
* `make probe-r-flight` runs it against a Flight server that enforces the same
  handshake and `authorization` header, and asserts a tampered session token is
  refused. Manual, not a merge gate: the first build compiles libarrow from
  source, about fifteen minutes.

What that does not cover: neither exercises `fraiseql-server` itself, so the two
are not known to agree on the wire — `crates/fraiseql-arrow/tests/
flight_auth_test.rs` is what pins the server's side.

### [`clickhouse/`](clickhouse)

SQL analytics over Arrow events ingested by the ClickHouse sink.

```bash
clickhouse-client < examples/clickhouse/arrow_integration.sql
```

---

## Also in this tree

| Directory | What it is |
|-----------|------------|
| [`async-jobs-subgraph/`](async-jobs-subgraph) | a federation subgraph for non-SQL mutations, with a Docker image that builds |
| [`cascade-create-post/`](cascade-create-post) | the cascade mutation pattern, end to end |
| [`changelog-sidecar/`](changelog-sidecar) | consuming the change-log outbox |
| [`federation/basic/`](federation/basic), [`federation/composite-keys/`](federation/composite-keys) | Apollo Federation v2 subgraphs that compose |
| [`multitenant/`](multitenant), [`saas/`](saas) | multi-domain schemas driven by `fraiseql.toml` |
| [`ltree-hierarchical-data/`](ltree-hierarchical-data) | LTREE hierarchies: a management chain and a category taxonomy |
| [`mutation-patterns/`](mutation-patterns) | 18 PL/pgSQL mutation patterns with a test script |
| [`typescript-client/`](typescript-client) | generated TypeScript client |

---

## What keeps this file honest

Three gates, because a repaired example rots again by the next release:

| Gate | Checks |
|------|--------|
| `tools/check-examples-integrity.sh` | static: compose mounts resolve, `COPY` sources exist, no `\|\| true` around a build, no unanchored `healthy` grep, and every documented `cd` lands somewhere |
| `tools/check-examples-compile.sh` | every `schema.py` runs and every `fraiseql.toml`/`schema.json` compiles, each from its own directory |
| `make examples-smoke` | loads each example's SQL under `ON_ERROR_STOP=1`, compiles its schema, runs its `queries/*.graphql`, boots the server and asks it a question |

Run all three before changing anything here.

## Next steps

* [Architecture](../docs/architecture/)
* [Authoring guide](../docs/authoring.md)
* [Performance](../docs/performance.md)
* [Linting](../docs/linting.md)

## Contributing an example

An example PR needs: code that runs, a README that names the prerequisites, and a
green `tools/check-examples-integrity.sh` and `tools/check-examples-compile.sh`. If
it ships SQL, it needs a `sql/setup.sql` that loads clean under `ON_ERROR_STOP=1`;
if it ships a schema, `make examples-smoke` has to be able to ask it a question.
