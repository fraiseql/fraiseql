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

[identity.sender]                  # verified sender-identity (send path)
enabled = true
query   = "SELECT email_address AS sending_address FROM tb_sales_mailbox m \
           JOIN tb_actor a ON a.mailbox_id = m.id WHERE a.sub = $sub AND m.verified"
map     = { sending_address = "sending_address" }
```

- `$name` tokens are bound from the request's claims (and the well-known
  identity fields `sub` / `tenant_id` / `org_id` / `email` / `name` / `iss`);
  values are bound out-of-band, **never** interpolated into the SQL.
- Unknown keys are rejected (`deny_unknown_fields`) — a mistyped or stranded key
  fails loud at startup rather than being silently ignored.
- **Trigger = `enabled = true` alone.** When enrichment is enabled, *every*
  authenticated request resolves and fail-closes, whether or not the current
  operation reads an enriched field. An enabled profile whose schema declares no
  `enrichment` consumer emits a loud startup warning; the zero-cost path belongs
  to `enabled = false` / absent config only.
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
| **Zero rows** (unknown / unprovisioned subject) | `Denied` | **403, before dispatch** | refuse; permanent |
| **> 1 row** (ambiguous identity) | `Denied` | **403** | refuse; permanent |
| A declared mapped field is **NULL / absent** | `Denied` | **403** | refuse; permanent |
| A referenced `$param` missing from the token | `Denied` | **403** | refuse; permanent |
| DB down / query error / pool exhausted | `Unavailable` | **503** | retry (transient) |

- **No row ⇒ fail**, never silent-skip: the unknown subject is denied *before*
  any data query runs, not scoped to an empty set. Strictly stronger than relying
  on every view author to deny on `NULL`.
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
- **Invariant:** *a revocation or role change propagates within `cache_ttl_secs`,
  or immediately via `flush(sub)`.* Raising `cache_ttl_secs` widens that window —
  do it with open eyes.
- **Manual flush** is exposed on the admin API, behind the admin bearer token,
  when enrichment is enabled: `POST /api/identity/flush` with `{"sub": "..."}`
  evicts one subject; `POST /api/identity/flush-all` clears the cache.

---

## Operational notes

- **Provision actors out-of-band.** Under `enabled = true`, every authenticated
  request fail-closes, so an app that creates a user's actor row *from* that
  user's first authenticated request would deadlock. Provision via an admin path,
  an IdP webhook, or another unauthenticated path.
- **Verified sender-identity** resolves on the same primitive: `sub → verified
  from-address + mailbox`, cached and fail-closed. The default `LoginEmailSender`
  (sending address == login email) is the degenerate case; a DB-backed resolver
  replaces it where the sending mailbox differs. The `send_email` host op and SMTP
  transport that consume the seam land with the native-runtime hardening train.

---

## Where it lives

| Concern | Location |
|---|---|
| Resolver, cache, failure model, Postgres store, both consumers | `crates/fraiseql-server/src/identity/` |
| Config variants + namespaced read (no DB) | `fraiseql-core` (`SessionVariableSource::Enrichment`, `InjectedParamSource::Enrichment`, `security::ENRICHED_NAMESPACE_PREFIX`) |
| Sender seam (object-safe trait + login-email default) | `fraiseql-functions` (`SenderIdentityResolver`, `LoginEmailSender`) |

See [ADR-0016](../adr/0016-enriched-identity-resolution.md) for the decision
record.
