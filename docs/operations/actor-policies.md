# Actor-class policies and budgets

FraiseQL classifies every authenticated request as one of four actor kinds and
records it in the audit trail. Since #966 that classification is also
*consumable*: an operation can restrict which classes may run it, and a tenant
can give each class its own cost budget.

| Class | Derived from |
|---|---|
| `human_user` | the default for an ordinary user token |
| `service_account` | a `service_account` scope, or an API key |
| `ai_agent` | an RFC 8693 `act` delegation claim |
| `system_job` | never token-derived; set by internal callers |

## Restricting an operation to actor classes

```json
{
  "name": "deleteTenant",
  "return_type": "Tenant",
  "requires_actor": ["human_user"]
}
```

Empty (the default) means unrestricted. Non-empty is an **allow-list**: the
request's class must be one of the listed ones, or the operation is refused with
`FORBIDDEN`.

### Why an allow-list and not a deny-list

A deny-list admits every class invented after it was written. An allow-list
refuses them. If a fifth `ActorType` is ever added, it is in nobody's list until
an author puts it there — which is the direction you want a security gate to fail
in.

### Delegation is deliberately not consulted

A delegated token (RFC 8693) says "this agent is acting for that human". It
carries the **human's** roles, so `requires_role` already admits it — that is what
composite authorization means, and it is why `requires_role` cannot express what
this can.

`requires_actor` does **not** fall back to the delegating human's permissions. An
agent acting for an administrator is still an agent. If it did fall back, the
predicate would be a no-op for exactly the case it exists for: "autonomous agents
cannot delete a tenant, regardless of the underlying user's permissions".

### Ordering against `requires_role`

The role gate runs first, and answers "not found" to avoid role enumeration. So:

| Caller | Answer |
|---|---|
| lacks the role | `Query 'x' not found in schema` — learns nothing about the class rule |
| holds the role, wrong class | `FORBIDDEN` |
| holds the role, right class | served |

Both gates compose as AND. An unauthenticated request has no classification, so
any non-empty list refuses it.

### Every transport, one gate

The predicate is enforced **inside the executor**, at the gates every read and
write passes through on its way to the database:

| Path | Gate |
|---|---|
| GraphQL query | `execute_regular_query_with_security` |
| GraphQL mutation | the universal mutation chokepoint |
| REST read, `Prefer: count=exact`, streaming exports | `resolve_direct_read` |
| MCP `tools/call` | reaches the GraphQL gate |
| Relay `node(id:)` | the node lookup |
| Federation `_entities` | the entity resolver |

That is a deliberate choice over per-transport checks. A predicate advertised on
one transport and enforced on another is not enforced, and a transport added
later inherits this one for free. The `actor_predicate_e2e_pg` suite drives four
of these doors at the same restricted operation and requires the same refusal
from each — its REST case found a real hole during development, which is the
whole argument for gating at the read rather than at the mount.

### Authoring

Today `requires_actor` is expressible from `schema.json` and compiled by
`fraiseql compile`. **No official SDK authors it yet** — tracked as
[#1123](https://github.com/fraiseql/fraiseql/issues/1123). An unrecognised token
is a compile error naming the offender, never a silently-dropped restriction: an
allow-list that fails to parse is an *open* operation, so silence is the wrong
failure mode.

### What makes the classification trustworthy

`requires_actor` is only as good as the classification being underivable from
anything the caller controls. It is:

- the `act` claim is honoured only on **signature-verified** tokens;
- the derived value lives in the reserved `fraiseql.` attribute namespace, which the token extractor **strips** from claims — a caller cannot supply `fraiseql.actor_type`;
- the API-key, gRPC and Flight paths never forward token claims at all;
- nothing deserializes a security context from an untrusted payload.

A deployment trusting `ai_agent` restrictions is trusting its IdP's `act`
issuance — exactly as `requires_role` trusts its role claims.

## Per-actor-class cost budgets

A service account draining a nightly report and a human clicking through a UI are
the same tenant. Without per-class budgets they share one allowance: the batch
job exhausts the window the humans need, and sizing the window for the batch job
removes the ceiling from everyone.

```jsonc
// PUT /api/v1/admin/tenants/{key}
{
  "cost_budget": 1000,              // tenant-wide per-request ceiling
  "cost_budget_per_minute": 60000,  // tenant-wide rolling window
  "cost_budget_per_actor": {
    "ai_agent":        { "per_request": 200, "per_minute": 5000 },
    "service_account": { "per_minute": 200000 }
  }
}
```

Two rules, and both matter:

- **A class's budget replaces the tenant-wide one; it does not stack.** A class configured with a
  *larger* allowance than the tenant default is a legitimate configuration (agents run the
  expensive reports), and stacking would make that override unreachable.
- **Each class draws on its own rolling window.** Charging one shared counter would make the
  tenant-wide window a function of the traffic mix, which is the coupling per-class budgets exist
  to remove.

A class absent from the map falls back to the tenant-wide budget. An
unauthenticated request has no class and takes the tenant-wide budget — not
`human_user`'s, which is what a naive default would hand it.

A tenant may configure **only** per-class budgets, with no tenant-wide ones; the
cost estimator still runs and the class budgets are still charged.

An override that sets neither `per_request` nor `per_minute` is **refused at
registration**, not stored. It would otherwise be reported by the admin API and
consulted on every request while changing no decision — a budget an operator can
read back and that refuses nothing.

### Where budgets are charged, and where they are not

Cost budgets apply to an explicitly-keyed, registered tenant, charged at the
same chokepoint as the concurrency and per-second quotas: GraphQL, the SSE
stream, Arrow Flight and async-operation submission.

**MCP is deliberately excluded**, and was before this change. An MCP document's
shape is fixed by the schema — one root field, a fixed scalar projection,
arguments as variables — so its cost score is constant per tool. Charging it
would either always pass or permanently disable that tool for a budgeted tenant,
metering nothing. Volume over MCP is bounded by the concurrency permit and the
per-second limiter, which do apply. The schema-wide `[security.cost_budget]
per_request_max` is different: it is enforced inside the executor for every
transport, MCP included.
