# Deploying a Subgraph Behind a Gateway

A federated FraiseQL server is a subgraph. Clients reach it through a gateway (Apollo
Router, Hive Gateway, Cosmo Router), and the gateway composes the supergraph from each
subgraph's `_service { sdl }`. This guide covers which schema each layer shows and to whom,
and how to keep `requires_role` operations out of sight of the clients that may not call
them.

## What each surface shows

| Surface | Who sees it | What it lists |
|---------|-------------|---------------|
| Subgraph `__schema` / `__type`, schema exports | The caller | What the caller's roles may see |
| Subgraph `_service { sdl }` | The gateway, at composition | Every operation, gated ones included |
| Supergraph composed without a contract | Every client of the gateway | Every operation of every subgraph |
| Contract supergraph | The clients the contract is served to | What the contract's tags keep |

The subgraph filters its own introspection per caller, but `_service { sdl }` is fetched
once, for composition, and is the same document for every caller. **A supergraph composed
without a contract lists every gated query and mutation, with its arguments and types, to
every client of the gateway.** The subgraph still checks the role on every call, so this is
a disclosure of the schema surface, not of access. The rest of this guide narrows that
surface.

## 1. Let only the gateway reach the subgraph

- Put the subgraph on a network only the gateway reaches. A client that bypasses the gateway
  sees the subgraph's own surface, including its `_service { sdl }` if it holds a credential.
- Leave `service_sdl_require_auth` on (`FRAISEQL_SERVICE_SDL_REQUIRE_AUTH`; unset, it follows
  `introspection_require_auth`, which defaults to `true`). The gateway authenticates as a
  [service account](../adr/0018-service-account-identities.md) whose ceiling grants no more
  than reading the schema, or you compose offline from the artifact
  (`fraiseql federation sdl schema.compiled.json`). See
  [Giving a gateway the subgraph's SDL](../architecture/federation.md#giving-a-gateway-the-subgraphs-sdl).
- Turn the subgraph's introspection off (`introspection_enabled = false`,
  `FRAISEQL_INTROSPECTION_ENABLED`). The gateway composes from `_service { sdl }`, which is
  served whether introspection is on or off, so nothing in the gateway path needs it.

## 2. Turn the gateway's introspection off, or authenticate it

The gateway answers introspection from the supergraph, not from the subgraph, so the
subgraph's per-caller filtering does not apply there. Disable introspection at the gateway
in production, or serve it only to authenticated tooling. Without a contract, a client who
can introspect the gateway reads every gated operation.

## 3. Enforce trusted documents at the gateway

Run strict trusted documents (a persisted-query safelist) **at the gateway**, so a client can
only send operations you published. Do not run strict mode at the subgraph: a strict store
refuses every request that carries no `documentId`, and the gateway sends each subgraph
operations it planned itself, which no manifest lists: the per-subgraph fetches of a query
plan, and the `_service` and `_entities` requests it composes and resolves entities with.
Those requests would all be refused.

A safelist limits what clients can *send*. It does not hide the schema from a client who can
introspect the gateway, so pair it with step 2.

## 4. Give each audience a contract

When different clients may see different parts of the graph, build a contract supergraph per
audience (GraphOS, Hive and Cosmo all support tag-based contracts). The subgraph's SDL tags
what a role gates:

- each `requires_role` root query and mutation carries `@tag(name: "requires-role/<role>")`;
- so does each object type of the subgraph's own that only that role's operations reach, or
  that the role gates itself.

A contract that **excludes** `requires-role/admin` drops the `admin` operations and the types
only they expose. A contract that includes it keeps them, and the subgraph still checks the
role on every call routed through it.

```graphql
type Query {
  products: [Product!]!
  productCosts: [ProductCost!]! @tag(name: "requires-role/catalog-admin")
}

type ProductCost @tag(name: "requires-role/catalog-admin") {
  productId: ID!
  unitCost: Money!
}
```

`Money` is `@shareable`, so it stays untagged (see below).

How roles map to tags:

- Letters, digits, `-` and `_` are written as they are. Any other byte is written as `/` and
  two hex digits, so `org:admin` becomes `requires-role/org/3Aadmin` and no two roles share a
  tag.
- A role whose tag would pass the 128-character limit contracts impose makes the SDL fail
  rather than go untagged.

What is never tagged, and why:

- **Entities, `@shareable` types, inputs and enums.** Another subgraph may declare the same
  name, and a supergraph tag covers every subgraph's declaration of it, so tagging it here
  would strip another subgraph's fields. Turn on the contract's option to remove unreachable
  types: those only gated operations use then go with them.
- **A type two roles reach.** Excluding either role leaves it reached through the other, so a
  tag would remove it from under a field the contract keeps.

## Checklist

- [ ] The subgraph is reachable only from the gateway's network
- [ ] `service_sdl_require_auth` is on, and the gateway authenticates as a service account (or composes offline)
- [ ] The subgraph's `introspection_enabled` is off
- [ ] The gateway's introspection is off, or authenticated
- [ ] Strict trusted documents run at the gateway, not the subgraph
- [ ] Each audience that must not see a role's operations gets a contract excluding `requires-role/<role>`, with unreachable types removed
- [ ] Anyone served the full supergraph is someone who may see every gated operation's name, arguments and types
