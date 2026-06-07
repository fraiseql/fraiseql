# Operation-level authorization

FraiseQL has two layers of operation-level access control:

| Layer | Question it answers | Where it's declared |
|-------|--------------------|---------------------|
| **Static** — `requires_role` | "Does this principal hold role `X`?" (and hide the operation's existence otherwise) | `query`/`mutation(requires_role="admin")` in the compiled schema |
| **Dynamic** — `Authorizer` | "May *this* principal run *this* operation, given its input?" | An app-supplied `Authorizer` on `RuntimeConfig` |

The static layer is a compile-time role check that returns *"not found in schema"*
(enumeration-hiding). The **dynamic** layer (this guide, issue #422) is a pluggable
Policy Enforcement Point (PEP): the engine *enforces*, but the *decision* is delegated
to an app-supplied trait object, so authorization can be backed by in-process rules, a
DB query, or an external service. It is the operation-level counterpart of the
field-level [`FieldAuthorizer`](field-authorization.md) and mirrors the `RLSPolicy`
plugin shape.

## The trait

```rust
use fraiseql_core::security::{Authorizer, AuthzRequest, AuthzDecision, OperationKind};
use fraiseql_core::error::Result;

/// Allow reads for everyone; require an authenticated principal for writes.
struct WritesNeedAuth;

impl Authorizer for WritesNeedAuth {
    fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        // Reads are public; writes (and any future operation kind) need a principal.
        // `OperationKind` is `#[non_exhaustive]`, so avoid an exhaustive match.
        if matches!(req.operation, OperationKind::Query) || req.principal.is_some() {
            Ok(AuthzDecision::Allow)
        } else {
            Ok(AuthzDecision::Deny { reason: "authentication required".to_string() })
        }
    }
}
```

`AuthzRequest` carries:

- `principal: Option<&SecurityContext>` — the authenticated principal, or **`None` on the
  anonymous (unauthenticated) entry path**. The authorizer is *always* consulted when
  configured, so an app can deliberately allow public operations or default-deny anonymous
  ones — the decision is the app's, not the engine's.
- `operation: OperationKind` — `Query`, `Mutation`, or `Subscription`.
- `name: &str` — the root operation field name (`"users"`, `"createUser"`, `"_entities"`,
  `"__schema"`, …).
- `input: Option<&serde_json::Value>` — the request's GraphQL variables / REST arguments.

## Wiring it up

Register the authorizer on `RuntimeConfig`, exactly parallel to `with_rls_policy` and
`with_field_authorizer`:

```rust
use fraiseql_core::runtime::RuntimeConfig;
use std::sync::Arc;

let config = RuntimeConfig::default().with_authorizer(Arc::new(WritesNeedAuth));
```

When no authorizer is configured (the default), the gate is a single `Option::is_some`
branch — **zero** overhead.

The same app object can implement **both** `Authorizer` and `FieldAuthorizer`: they share
the `SecurityContext` principal and the `Authorization` / `FORBIDDEN` error mapping, so
one policy type can serve operation- and field-level checks.

## Semantics

- **Fail-closed.** Any `Err` from `authorize` — or an `AuthzDecision::Deny` — fails the
  operation with HTTP **403 `FORBIDDEN`**; the operation never executes. Reserve `Err` for
  policy-evaluation failures (e.g. an unreachable policy backend); use `Deny` for ordinary,
  expected denials. A policy `Err` is **not** surfaced to the client (no information leak).
- **Deny reason.** The `reason` on a `Deny` is folded into the 403 error message.
- **Anonymous = `None` principal.** The anonymous path consults the authorizer with
  `principal: None` rather than defaulting to deny, so public operations remain expressible.
- **Multi-root.** Each root field of a multi-root query is authorized independently; a deny
  on *any* root fails the whole request **before any root is dispatched** (no partial data).
- **AND-composition with `requires_role`.** The authorizer does **not** replace the static
  `requires_role` gate; both must allow. The authorizer runs first (a `Deny` is a 403); the
  `requires_role` gate keeps its enumeration-hiding *"not found in schema"* response for a
  principal that lacks the role. An allowing authorizer never bypasses `requires_role`.
- **Response cache.** Unlike the field-level authorizer, the operation gate runs **before**
  the response cache is consulted, so it is *always* evaluated — a warm cache never replays
  an allow past a later deny. No cache bypass is required.

## Path coverage

A PEP is only as strong as its least-guarded entry path. The authorizer is enforced on
**every** operation entry path:

| Path | Where it is enforced |
|------|----------------------|
| Authenticated GraphQL (`execute_with_security`) — queries, aggregate, window, `node`, `_entities`, introspection | Chokepoint, before dispatch (`principal = Some`) |
| Anonymous GraphQL (`execute`), incl. multi-root, `execute_with_scopes`, `execute_with_context` | Chokepoint, before dispatch (`principal = None`) |
| MCP tool calls (auth + anon) | Route through the GraphQL chokepoints |
| Mutations — GraphQL, MCP, **authenticated and anonymous REST**, the direct API | The universal mutation chokepoint (`execute_mutation_impl`), covering the anonymous-REST write path that bypasses the `execute*`/`execute_with_security` chokepoints |
| REST reads — GET, count, streaming (NDJSON/CSV/XLSX), embedding sub-queries, bulk-by-filter lookup | The shared read runner methods (`execute_query_direct` / `count_rows`) |
| Subscriptions (`graphql-transport-ws` / `graphql-ws`) | At subscribe-time, with the connection's principal — a deny rejects with a `FORBIDDEN` error frame |

> **Introspection and federation are gated too** (as `Query` named `__schema`/`__type`/
> `_entities`/`_service`). If you want introspection always available, have your authorizer
> `Allow` those names.

## Current limitations (tracked follow-ups)

- **Per-event subscription re-evaluation.** A subscription is authorized **once**, at
  establishment. Per-event delivery does not route through the executor, so a policy that
  changes mid-stream is not re-applied to an already-established subscription.
- **`execute_with_scopes` principal fidelity.** That entry point carries scopes but not a
  full `SecurityContext`, so the authorizer sees it as anonymous (`None`). Use
  `execute_with_security` for principal-aware operation authorization.
- **Federation `_entities` granularity.** The authorizer sees the operation name
  `_entities`, not the per-`representation` entity types being resolved.
- **`RLSPolicy::evaluate()` argument widening.** Row-filter injection already receives the
  operation name; widening it to also receive the operation arguments is a separate
  (breaking) change tracked independently.
- **No TOML/env surface.** Like `FieldAuthorizer`, the `Authorizer` is a library-config
  plug today (`with_authorizer`); the server binary installs one only if an embedder sets it
  on the `RuntimeConfig`. An SDK/declarative authoring surface is a follow-up.

## See also

- [Dynamic field-level authorization](field-authorization.md) — the field-level counterpart (`FieldAuthorizer`).
- [`RLSPolicy`](../architecture/overview.md) — row-level (whole-row) security.
- `query/mutation(requires_role=…)` — static, enumeration-hiding role gates.
