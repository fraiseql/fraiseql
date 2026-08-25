# Basic Federation — two subgraphs, one graph

Two FraiseQL subgraphs, each with its own PostgreSQL database, composed by the Apollo
Router into a single graph.

```
        client
          │
        router (:4000)          Apollo Federation v2 supergraph
          │
    ┌─────┴─────┐
    │           │
users-service  orders-service    (:4001, :4002)
    │           │
 postgres    postgres            two databases, no foreign key between them
```

## What it demonstrates

`users-service` **owns** the `User` entity — it holds the profile rows and answers for
the type:

```graphql
type User @key(fields: "id") {
  id: ID
  name: String
  email: String
  createdAt: DateTime
}
```

`orders-service` **owns** `Order` and **extends** that same `User` with one field:

```graphql
extend type User @key(fields: "id") {
  id: ID @external
  orders: [Order]
}
```

That is the pattern federation exists for. `orders-service` has no users table: the only
thing it ever learns about a user is the `id` the router passes it. Composed, the two
halves are one type:

```graphql
type User
  @join__type(graph: ORDERS, key: "id", extension: true)
  @join__type(graph: USERS, key: "id")
{
  id: ID
  orders: [Order]     @join__field(graph: ORDERS)
  name: String        @join__field(graph: USERS)
  email: String       @join__field(graph: USERS)
  createdAt: DateTime @join__field(graph: USERS)
}
```

Both halves are authored in Python. `key_fields=` makes an entity, `extends=True` makes
an extension, and `field(external=True)` marks the borrowed key — see
`users-service/schema.py` and `orders-service/schema.py`.

## Run it

Prerequisites: docker + docker compose, and the Apollo
[`rover`](https://www.apollographql.com/docs/rover/getting-started) CLI.

```bash
cd examples/federation/basic
make run    # boots both subgraphs, composes the supergraph, starts the router
make demo   # one query, answered from two databases
make down   # tears everything down
```

`make run` does three things in order, and the order matters: the subgraphs must be
answering before rover can introspect them, and the router needs a composed supergraph
before it can serve. `make build` waits for the subgraphs to report healthy rather than
assuming they came up.

## The federated query

```bash
curl -X POST http://localhost:4000/graphql \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ users { id name orders { id status total } } }"}'
```

The router resolves this in two hops:

1. `{ users { id name } }` against **users-service**, which owns `User`;
2. for each user returned, an `_entities` call against **orders-service**, passing the
   `id` it just got:

```graphql
query($representations: [_Any!]!) {
  _entities(representations: $representations) {
    ... on User { orders { id status total } }
  }
}
```

with `representations = [{"__typename": "User", "id": "1111…"}]`. That second call is
the whole of federation: a subgraph resolving a type it does not own, from a key.

## Each subgraph on its own

Both are ordinary GraphQL endpoints, and querying them directly is the quickest way to
see which half owns what.

```bash
make demo-subgraphs
```

```bash
curl -X POST http://localhost:4001/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ users { id name email } }"}'
```

```json
{"data":{"users":[
  {"id":"11111111-1111-4111-8111-111111111111","name":"Alice Johnson","email":"alice@example.com"},
  {"id":"22222222-2222-4222-8222-222222222222","name":"Bob Smith","email":"bob@example.com"},
  {"id":"33333333-3333-4333-8333-333333333333","name":"Charlie Brown","email":"charlie@example.com"}]}}
```

```bash
curl -X POST http://localhost:4002/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ userOrders(userId: \"11111111-1111-4111-8111-111111111111\") { id status total } }"}'
```

```json
{"data":{"userOrders":[
  {"id":"aaaaaaaa-0001-4000-8000-000000000001","status":"completed","total":99.99},
  {"id":"aaaaaaaa-0002-4000-8000-000000000002","status":"completed","total":149.99},
  {"id":"aaaaaaaa-0003-4000-8000-000000000003","status":"pending","total":199.99}]}}
```

## How each subgraph is built

The Dockerfiles run the v2 pipeline in the order the architecture prescribes:

```
authoring (Python)  ->  schema.json
compilation (CLI)   ->  schema.compiled.json
runtime (server)    ->  serves the compiled schema, no authoring tools present
```

Two details are easy to get wrong and are commented in place:

- the build context is the **repository root**, because the Dockerfile needs the
  workspace and the in-repo SDK — `fraiseql-cli` is a workspace member and cannot be
  compiled from its own directory;
- the server is built `--features fraiseql-server/federation`. Without it the server
  boots, answers ordinary queries and reports healthy, and returns
  `Federation is not enabled in this build` for `query { _service { sdl } }` — the one
  query a router asks.

## The database side

Each subgraph follows the Trinity pattern: `tb_*` tables with an integer surrogate key
that never leaves the database, a UUID `id` that is the public identity, and a `v_*`
view exposing `id` natively next to a JSONB `data` column.

The extended entity is just another view. `orders-service`'s `v_user` is one row per
user it has orders for, carrying the key and the field it contributes:

```sql
CREATE VIEW v_user AS
SELECT o.user_id AS id,
       jsonb_build_object('id', o.user_id, 'orders', jsonb_agg(...)) AS data
FROM tb_order o
GROUP BY o.user_id;
```

## Extending this

- **A third subgraph.** Add it to `router/supergraph.yaml` and recompose; the router
  stays the single endpoint clients use.
- **Composite keys.** `key_fields` takes more than one field — see
  [`../composite-keys/`](../composite-keys/).
- **Mutations.** Nothing here writes; mutations are declared in the same schema.py with
  `@fraiseql.mutation` and backed by a PostgreSQL function.
