# Composite-Key Federation — one entity, two identifying fields

Multi-tenant federation where a user's identity is the **pair**
`(organizationId, userId)`, not a single column.

```
        client
          │
        router (:4010)          Apollo Federation v2 supergraph
          │
    ┌─────┴─────┐
    │           │
users-service  orders-service    (:4011, :4012)
    │           │
 postgres    postgres            two databases, no foreign key between them
```

## What it demonstrates

A federation key can name more than one field. `users-service` owns `User`, keyed on
both halves:

```graphql
type User @key(fields: "organizationId userId") {
  organizationId: ID
  userId: ID
  name: String
  email: String
  role: String
  createdAt: DateTime
}
```

`orders-service` owns `Order` and extends that same `User`, borrowing **both** key
fields as `@external`:

```graphql
extend type User @key(fields: "organizationId userId") {
  organizationId: ID @external
  userId: ID @external
  orders: [Order]
}
```

The tenancy consequence is the point: no subgraph can resolve a user without naming the
organization, so a cross-tenant reference is not expressible in the graph at all. The
router's `_entities` representation carries both halves:

```json
{"__typename": "User",
 "organizationId": "00000000-0000-4000-8000-00000000000a",
 "userId": "10000000-0000-4000-8000-000000000001"}
```

Composed, the two halves are one type:

```graphql
type User
  @join__type(graph: ORDERS, key: "organizationId userId", extension: true)
  @join__type(graph: USERS, key: "organizationId userId")
{
  organizationId: ID
  userId: ID
  orders: [Order]     @join__field(graph: ORDERS)
  name: String        @join__field(graph: USERS)
  email: String       @join__field(graph: USERS)
  role: String        @join__field(graph: USERS)
  createdAt: DateTime @join__field(graph: USERS)
}
```

## Run it

Prerequisites: docker + docker compose, and the Apollo
[`rover`](https://www.apollographql.com/docs/rover/getting-started) CLI. Ports differ
from [`../basic/`](../basic/) so both examples can run side by side.

```bash
cd examples/federation/composite-keys
make run    # boots both subgraphs, composes the supergraph, starts the router
make demo   # one tenant-scoped query, answered from two databases
make down   # tears everything down
```

## The federated query

```bash
curl -X POST http://localhost:4010/graphql \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ users(organizationId: \"00000000-0000-4000-8000-00000000000a\") { userId name orders { id status total } } }"}'
```

Two hops: `users-service` answers `users(organizationId:)`, then the router calls
`_entities` on `orders-service` once per user, passing both key fields.

## Each subgraph on its own

```bash
make demo-subgraphs
```

```bash
curl -X POST http://localhost:4011/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ organizations { id name } }"}'
```

```json
{"data":{"organizations":[
  {"id":"00000000-0000-4000-8000-00000000000a","name":"Acme Corp"},
  {"id":"00000000-0000-4000-8000-00000000000b","name":"Globex"}]}}
```

```bash
curl -X POST http://localhost:4011/graphql -H 'Content-Type: application/json' \
  -d '{"query":"{ user(organizationId: \"00000000-0000-4000-8000-00000000000a\", userId: \"10000000-0000-4000-8000-000000000001\") { organizationId userId name role } }"}'
```

```json
{"data":{"user":{
  "organizationId":"00000000-0000-4000-8000-00000000000a",
  "userId":"10000000-0000-4000-8000-000000000001",
  "name":"Alice Johnson","role":"admin"}}}
```

## The database side

Each subgraph follows the Trinity pattern: `tb_*` tables with an integer surrogate key
that never leaves the database, the public identity as UUID column(s), and a `v_*` view
exposing those identity columns natively next to a JSONB `data` column.

`tb_user` carries `UNIQUE (organization_id, user_id)` — the composite identity stated in
the schema rather than implied — and `orders-service`'s extended `v_user` groups by the
same pair:

```sql
CREATE VIEW v_user AS
SELECT o.organization_id, o.user_id,
       jsonb_build_object('organization_id', o.organization_id,
                          'user_id', o.user_id,
                          'orders', jsonb_agg(...)) AS data
FROM tb_order o
GROUP BY o.organization_id, o.user_id;
```

Note the casing: the SQL keys are snake_case and FraiseQL projects them to camelCase on
the GraphQL surface, so `@key(fields: …)` names `organizationId userId` — the published
spelling, which is what Apollo composition resolves against.

## Extending this

- **A tenant-scoped mutation.** Declare it with `@fraiseql.mutation` and back it with a
  PostgreSQL function that takes the organization as its first argument.
- **Row-level security.** The views are the natural place: make them
  `security_invoker = true` and let base-table RLS decide which tenant's rows a role can
  see.
- **A single-field key** is the simpler shape — see [`../basic/`](../basic/).
