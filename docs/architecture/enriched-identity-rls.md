# Enriched-identity RLS

**Status:** Shipped (#539). One request-scoped `sub → DB → identity` resolver,
feeding read-scoping (RLS / views / injected params) and verified
sender-identity from the application's **own** database rather than from
client-asserted token claims.

---

## What it is

An IdP asserts a stable subject (`sub`). The application maps that subject to an
internal identity in its own database — for reads, `sub → actor_id / actor_role`;
for sends, `sub → verified from-address + mailbox`. FraiseQL resolves that mapping
**once per request, cached, and fail-closed**, and feeds it to the two places
that consume DB-derived identity:

- the **session-variable / `inject_params`** path, so RLS and view predicates
  scope on it; and
- the outbound **`send_email`** path, so `From` is server-verified.

There is **one resolver primitive**, with one cache policy and one failure model,
wired to two call sites.

The load-bearing property is *fail-closed at source*: anything other than exactly
one row with every mapped field present and non-null is a denial, and a denial
fails the operation — never a silent skip, never an empty-string GUC, the mapped
set applied whole or not at all.

---

## Configuration

Top-level `[identity]`, so it applies under **any** auth mode — HS256 and OIDC
parity by construction. One query schema, reused by both profiles.

```toml
[identity.enrichment]              # read scoping
enabled           = true
query             = "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub"
map               = { actor_id = "actor_id", actor_role = "actor_role" }  # column -> field
cache_ttl_secs    = 60             # a role change propagates within this window (see below)
negative_ttl_secs = 5
provision         = "SELECT fn_provision_actor($sub, $iss, $email, $claims)"  # optional, see below

[identity.sender]                  # verified sender-identity (send path)
enabled = true
query   = "SELECT email_address AS sending_address FROM tb_sales_mailbox m \
           JOIN tb_actor a ON a.mailbox_id = m.id WHERE a.sub = $sub AND m.verified"
map     = { sending_address = "sending_address" }
```

- `$name` tokens are bound from the request's claims (and the well-known
  identity fields `sub` / `tenant_id` / `org_id` / `email` / `name` / `iss`);
  values are bound out-of-band, **never** interpolated into the SQL.
- `$claims` binds the whole verified claim set as one **`jsonb`** value — the
  forwarded attributes (inbound `fraiseql.*` claims are stripped by the request
  extractor) plus those well-known fields. Any claim whose value is a JSON object
  or array binds as `jsonb`; scalars bind as their own type.
- `provision` is legal on `[identity.enrichment]` only. It parses on
  `[identity.sender]` because the two profiles share one schema, and boot refuses
  it there: provisioning a *sending* identity would invent a verified
  from-address.
- Unknown keys are rejected (`deny_unknown_fields`) — a mistyped or stranded key
  fails loud at startup rather than being silently ignored.
- **Trigger = `enabled = true` alone.** When enrichment is enabled, *every*
  authenticated request resolves and fail-closes, whether or not the current
  operation reads an enriched field. An enabled profile whose schema declares no
  `enrichment` consumer emits a loud startup warning; the zero-cost path belongs
  to `enabled = false` / absent config only. What "every" covers is enumerated
  under [Which transports resolve](#which-transports-resolve) — for three releases
  that sentence was true of `/graphql` alone (#1336), which is why the list is now
  written down and gated rather than asserted.
- **The converse is a boot refusal.** A compiled schema that declares an
  `enrichment` consumer while `[identity.enrichment]` is disabled does not start.
  Nothing would resolve an identity, so every read of an enriched field would fail
  and every other request would be served without the fail-closed check the schema
  implies.
- The resolver runs on a **separate, unscoped** connection pool (the app role, no
  per-request GUCs), so identity is resolved *before* the identity that scopes
  the main query is applied — no chicken-and-egg.

---

## The `fraiseql.enriched.*` namespace

Resolved fields are merged into the security context under the reserved
`fraiseql.enriched.<field>` prefix. This is forge-proof **by construction**: the
request extractor strips any incoming JWT claim whose key begins with
`fraiseql.`, so a token cannot carry `fraiseql.enriched.actor_role`.

Two source kinds read **only** that namespace, with **no** fallback to a raw
claim or a well-known field:

- `SessionVariableSource::Enrichment { field }`
- `InjectedParamSource::Enrichment(field)`

That no-fallback rule is the security property: it prevents a raw JWT claim of the
same name from impersonating a DB-derived identity field if enrichment does not
run. `Jwt` / `Header` / `Literal` keep their existing lenient semantics — no
behaviour change for anyone not opting in.

---

## The failure model

One shared result — `Resolved` / `Denied` / `Unavailable` — produced by the
resolver and interpreted by each call site:

| Lookup outcome | Classification | Read path (sync) | Send path (durable) |
|---|---|---|---|
| Exactly one row, **all** mapped fields present & non-null | `Resolved` | merge → proceed | bind `from` → send |
| **Zero rows** (unknown / unprovisioned subject) | `Denied` | **403, before dispatch** — unless `provision` is set (below) | refuse; permanent |
| **> 1 row** (ambiguous identity) | `Denied` | **403** | refuse; permanent |
| A declared mapped field is **NULL / absent** | `Denied` | **403** | refuse; permanent |
| A referenced `$param` missing from the token | `Denied` | **403** | refuse; permanent |
| DB down / query error / pool exhausted | `Unavailable` | **503** | retry (transient) |

- **No row ⇒ fail**, never silent-skip: the unknown subject is denied *before*
  any data query runs, not scoped to an empty set. Strictly stronger than relying
  on every view author to deny on `NULL`. This is the **only** outcome an
  optional [`provision`](#provision-serving-a-subject-the-actor-table-has-never-seen-1324)
  statement may change, and it changes it by making the row exist and reading
  again — not by lowering the bar the row must clear.
- **`> 1 row` fails** — for identity, ambiguity is a misconfiguration; we fetch up
  to two rows and deny on the second rather than silently `LIMIT 1`.
- **Never an empty-string GUC** — a NULL/absent mapped field is a denial, so no
  predicate ever sees `''` where it expected an actor.
- **Fail-closed on transient error too** — a DB hiccup fails the request rather
  than falling through to an unscoped query. (Sends, being durable, retry.)
- **Uniform, no bypass** — this applies to service-account / API-key subjects too:
  a service account needs an actor row. An "except for these principals" carve-out
  is exactly the silent hole this design closes.

### Denial observability

A denial fires before dispatch, so no query reaches the DB log. The server logs
every `Denied` at WARN with the reason (`zero-rows` / `ambiguous` / `null-field
<name>` / `missing-param <name>`) and the subject, and every `Unavailable` with
the underlying error — "why is this user 403" is one grep. The **outward response
body stays generic** ("Access denied"): a client-distinguishable reason would be
an existence oracle over the actor table.

---

## Cache and revocation

- **Key = the ordered bound-`$param` tuple** the query actually references, not
  bare `sub`. `sub` is unique only *per issuer*, and FraiseQL speaks multi-IdP; a
  multi-issuer app must bind an issuer discriminator (`$iss`). Keying on the bound
  parameters makes cache correctness exactly track the `WHERE` clause.
- **Positive TTL** `cache_ttl_secs` (default **60s**), **negative TTL**
  `negative_ttl_secs` (default **5s**, so a freshly provisioned actor goes live
  quickly). `Unavailable` is **never** cached.
- **The pre-provision `ZeroRows` is never cached either.** A request that arrived
  while another is provisioning would find it and fail closed for the rest of
  `negative_ttl_secs` — every concurrent first request 403'd by the one that is
  fixing the problem. What *is* cached is the outcome after the statement ran,
  denial included: that is what bounds a refused subject to one statement per
  window instead of one per request.
- **Invariant:** *a revocation or role change propagates within `cache_ttl_secs`,
  or immediately via `flush(sub)`.* Raising `cache_ttl_secs` widens that window —
  do it with open eyes.
- **Manual flush** is exposed on the admin API, behind the admin bearer token,
  when enrichment is enabled: `POST /api/identity/flush` with `{"sub": "..."}`
  evicts one subject; `POST /api/identity/flush-all` clears the cache.

---

## The push path: subscription row visibility (#596)

The pull path (GraphQL queries) enforces per-row RLS. The **push path** — live
subscriptions over `/ws` (`graphql-transport-ws` / legacy `graphql-ws`) — historically
did not: any principal authorized to subscribe to an entity received **every** row's
after-images. It now consumes the *same* enriched identity fields, so the two paths
share one boundary.

An entity declares a row policy in the compiled schema:

```jsonc
"subscription_policy": {
  "owner_path": "$.owner_id",     // single-level path into the after-image
  "identity_field": "user_id",    // the fraiseql.enriched.* field resolved here
  "bypass_roles": ["admin"]       // roles that get full visibility
}
```

At subscribe time the server derives a **server-owned** owner condition from the
connection's enriched identity and enforces it on every delivered event (AND semantics
with any tenant gate and client filters — a client filter can only *narrow*):

- **Resolvable identity** → the subscription is scoped to `owner_path == <enriched
  identity_field>`. The value is read **only** from the `fraiseql.enriched.*`
  namespace, so a client-supplied claim or subscribe variable cannot widen it.
- **`bypass_roles` role** → full visibility, no added condition.
- **Unresolvable identity** (no enrichment configured, a denial, a resolver outage, a
  NULL field, or an anonymous connection) → the subscription is **refused at subscribe
  time** — fail-closed, never delivered unfiltered.
- **No policy** on the entity → unchanged behavior (no back-compat break).

The single policy→condition derivation lives in `fraiseql-core`
(`schema::SubscriptionPolicy::derive` → `OwnerCondition`), so the push seam and any
future seam consume identical semantics; a divergence — e.g. `bypass_roles` honored on
one path but not the other — would itself be a bypass. `extract_rls_conditions` is
fail-closed for the same reason: a clause shape it cannot enforce as equality refuses
the subscription rather than silently widening it.

> **DELETE events / pre-images.** The policy evaluates on whichever image the event
> carries; a scoped subscriber only learns of a delete when the change stream includes
> the row's owning image (i.e. `pre_image` is enabled for the entity). This is the
> fail-closed default — a scoped client is never shown a row it does not own, even a
> deleted one.

> **Removed: the `/realtime/v1` entity stream.** A second, dormant push subsystem
> (never assembled by the stock server) once carried entity after-images alongside the
> `/ws` path, with `POST /realtime/v1/broadcast` app-channel pubsub. It was removed in
> full (#605) as unhardened dead surface — the live `/ws` GraphQL-subscription path is the
> single supported real-time mechanism, and the only push seam this policy adapts.

> **Hot-reload.** Subscription policies are resolved at server start; a policy added or
> changed via a schema **hot-reload** takes effect on **restart**, not immediately (the
> subscription subsystem is not yet re-mounted on reload). Change subscription
> row-visibility policies with a restart. Tracked as a follow-up.

---

## Operational notes

- **Provisioning a new subject's actor row.** Under `enabled = true`, every
  authenticated request fail-closes, so an app that creates a user's actor row
  through *its own* authenticated mutation would deadlock: the gate refuses the
  request before the mutation can run. The row therefore comes from outside that
  path — an admin path, the IdP's `user.created` webhook, SCIM — or from
  `provision`, below, which runs inside the resolver and so is not behind the
  gate. See [ADR-0016](../adr/0016-enriched-identity-resolution.md)'s amendment.
- **Verified sender-identity** resolves on the same primitive: `sub → verified
  from-address + mailbox`, cached and fail-closed. The default `LoginEmailSender`
  (sending address == login email) is the degenerate case; a DB-backed resolver
  replaces it where the sending mailbox differs. The `send_email` host op and SMTP
  transport that consume the seam land with the native-runtime hardening train.

---

## `provision`: serving a subject the actor table has never seen (#1324)

For an IdP outside FraiseQL — Hanko, Clerk, Auth0, Kinde — the actor row is
normally written by that IdP's `user.created` webhook. The IdP hands the browser
a token before it delivers that webhook, so a brand-new user's first request
arrives with a valid token and no row, and `[identity.enrichment]` answers 403
for as long as the delivery takes.

With `provision` set, a **zero-row** resolution runs the statement and then runs
`query` again. The re-read decides the outcome; nothing else changes.

```toml
[identity.enrichment]
enabled   = true
query     = "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub"
provision = "SELECT fn_provision_actor($sub, $iss, $email, $claims)"
map       = { actor_id = "actor_id", actor_role = "actor_role" }
```

### The contract the statement must keep

- **Idempotent under concurrency.** N concurrent first requests for one new
  subject each run the statement; they must produce one row.
  `INSERT … ON CONFLICT (<sub column>) DO NOTHING` is the documented shape, and
  a function wrapping the same insert is equivalent.
- **One conflict target, shared with every other writer.** Whatever column holds
  the IdP's subject is the conflict target here *and* in the IdP-webhook handler
  that writes the same row out of band (`after:ingest:webhook:<provider>`). If
  the two disagree — one keying on the subject, the other on the email — a user
  who signs up and is provisioned in the same second ends up with two actor rows,
  and the next resolution denies them as `Ambiguous`.
- **To refuse a subject, insert nothing.** The re-read then denies (403), and
  that denial is negative-cached for `negative_ttl_secs`, so a refused subject
  costs one statement per window rather than one per request. **Raising is an
  outage** (503, never cached): it tells the caller to retry, which is not what a
  refusal means.
- **Creation only.** Updates and deletions stay with webhooks or SCIM. The
  statement never runs for a subject that already has a row.

### What it never does

`provision` fires on `ZeroRows` alone. An ambiguous row set, a NULL mapped field
and a missing `$param` are the denials of an identity that **exists**, and none
of them is offered the statement — otherwise a misconfigured deployment could
overwrite a real actor into one that resolves.

### Security note

With `provision`, the IdP's user base writes the actor table: every subject that
IdP will issue a token for gets a row, and on a self-signup IdP that is anyone
who can complete a signup. **The provision function is the policy.** Decide in it
what a new subject is allowed to become — the role it gets, the tenant it lands
in, whether a domain is allowed at all — and insert nothing for the ones you
refuse. Binding `$iss` matters here for the same reason it matters to
`query`: `sub` is unique only per issuer.

Provisioning runs on the unscoped enrichment pool, below the fail-closed gate.
That is what makes it work, and it is also why the statement is the only thing
standing between the IdP and the actor table.

---

## Which transports resolve

Resolution happens where a credential becomes a principal, not where an operation is
dispatched. That placement is deliberate: the engine is *not* below every transport
(gRPC's read arms go straight to the adapter, #1348), and the resolver binds its
`$param`s from the context's attributes, so the shared context builder has to have run
first. It also keeps the seam clear of `RuntimeConfig`, which per-tenant executors do
not inherit (#1333) — so a tenant-keyed request resolves like any other.

| Transport | Resolves | Where |
|---|---|---|
| `POST`/`GET /graphql`, GraphQL SSE | yes | `routes/graphql/handler/stages.rs::enrich_identity` |
| REST | yes | `RestSecurityContext`, the extractor that also enforces `require_auth` |
| MCP | yes | `mcp/handler.rs::authenticate` |
| gRPC (unary + server-streaming, reads and writes) | yes | `routes/grpc/mod.rs::principal_from_user` |
| Async operations (`/operations/v1`) | yes, at submission — the snapshot the background worker runs with carries the resolved identity | `routes/async_operations.rs::submit` |
| `/ws` subscriptions | resolves; a failed resolve leaves the enriched field absent, so a policy-declaring subscription refuses at derivation | `routes/subscriptions.rs::enrich_principal` |
| **Arrow Flight** | **no — the one exemption**, tracked as [#1349](https://github.com/fraiseql/fraiseql/issues/1349) | its handlers live in `fraiseql-arrow`, which cannot reach the resolver |

Two things keep that table honest, because a list in a document is exactly what was
wrong before:

- **`tools/check-principal-producers.sh`** fails the build when a site turns a credential
  into a principal — or takes one from the shared extractor — and does not resolve it.
  The Flight handlers are its only `KNOWN` entries, and a staleness check fails the
  moment either starts resolving, so #1349 cannot be fixed and left listed.
- **The engine refuses an unresolved principal.** `enforce_enrichment_resolved` rejects a
  context carrying no enrichment mark when the schema declares an enrichment consumer,
  at every executor entry point. Absence of the mark is the fail-closed state, so a
  transport added tomorrow is refused rather than served — including Flight, whose reads
  do reach the engine. The exemption above therefore means "refuses", not "serves
  unenriched".

A `system_job` principal — the server acting as itself, from no credential — marks
itself exempt at its construction site. It has no subject a resolver could look up.

---

## Where it lives

| Concern | Location |
|---|---|
| Resolver, cache, failure model, Postgres store, both consumers | `crates/fraiseql-server/src/identity/` |
| The seam every transport calls between authenticating and dispatching | `identity::resolve_request_identity` |
| Config variants + namespaced read (no DB) | `fraiseql-core` (`SessionVariableSource::Enrichment`, `InjectedParamSource::Enrichment`, `security::ENRICHED_NAMESPACE_PREFIX`) |
| The engine's fail-closed backstop + the mark it reads | `fraiseql-core` (`enforce_enrichment_resolved`, `security::EnrichmentMark`) |
| Sender seam (object-safe trait + login-email default) | `fraiseql-functions` (`SenderIdentityResolver`, `LoginEmailSender`) |

See [ADR-0016](../adr/0016-enriched-identity-resolution.md) for the decision
record.
