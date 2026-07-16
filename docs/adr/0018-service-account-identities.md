# ADR-0018: Service-Account Identities

## Status: Accepted

This ADR records how FraiseQL grants an **external daemon or service** a named,
auditable, ceiling-bounded identity for calling the API — reusing the authority
model scheduled sources and functions already use internally (`run_as` /
`ActorType::ServiceAccount`) rather than letting each adopter hand-roll service auth
in a sidecar. It is design-only; no implementation lands on an un-accepted ADR.

Related: ADR-0016 (enriched-identity resolution, #539), the sources/functions
`run_as` ceiling (`docs/architecture/sources.md`, ADR-0015), the #390 actor taxonomy,
`crates/fraiseql-server/src/api_key.rs` (the existing static-API-key mechanism this
extends), phase 06 subscription row-visibility (#596).

---

## Context

External clients that are **not human users** — a reconciliation daemon, a data
pipeline, a partner integration — need to call the API with a stable, least-privilege
identity that is visible in the audit trail. Today FraiseQL has the right *internal*
authority concept but no *external* grant for it:

- **Internally**, scheduled sources and dispatched functions run under a fail-closed
  `run_as` ceiling (`RunAs { roles, scopes, tenant }`) minted into a
  `SecurityContext::system_job(...)` with `ActorType::SystemJob` — authority is
  *exactly* the granted roles + scopes, never more, and the actor type is recorded for
  audit but is never an authorization input.
- **Externally**, there is a *partial* mechanism: static API keys
  (`crates/fraiseql-server/src/api_key.rs`). A configured key is SHA-256-hashed and
  constant-time-compared, and on match yields a `SecurityContext` named `apikey:<name>`
  stamped `ActorType::ServiceAccount`. But it grants **scopes only** — no roles, no
  tenant, no `run_as`-style ceiling — and its `key_hash` sits **inline in the compiled
  schema JSON** rather than behind an environment indirection.

So an adopter who needs a daemon with a real role/tenant ceiling ends up hand-rolling
service auth in a sidecar: an undocumented, unauditable, per-deployment token scheme,
with the authority boundary living outside FraiseQL entirely. The `ServiceAccount`
actor type exists (#390) and the ceiling type exists (`RunAs`), but nothing binds them
to an external credential.

This ADR closes that gap by making a **service account** a first-class principal:
a named identity + an env-indirected credential + a `run_as` ceiling, authenticated on
the standard request path and recorded as `ActorType::ServiceAccount`.

---

## Decision

### 1. A service account is `named identity + credential + run_as ceiling`

Declared in the compiled schema / config as a `[service_accounts.<name>]` block:

```toml
[service_accounts.reconciler]
secret_env = "FRAISEQL_SA_RECONCILER_SECRET"   # names an env var; never the secret inline
# The run_as ceiling — identical shape to sources'/functions' `run_as`.
roles      = ["ledger:read", "ledger:reconcile"]
scopes     = []
tenant     = "acme"                            # omit ⇒ global / NULL tenant
```

The ceiling is the **existing** `RunAs { roles, scopes, tenant }` (fail-closed:
absent/empty ⇒ anonymous, RLS/field-authz deny writes) — not a new authority type.
Authentication yields the principal's `SecurityContext` via the same
`RunAs::identity`-style path sources use (`SecurityContext::system_job(...)` with the
account's `roles`/`scopes`/`tenant`), so a service account "can never exceed its
ceiling" is true by construction and by reuse, not by a second implementation.

### 2. Credential = a hashed static bearer secret behind an env indirection

- The credential is a **static bearer secret** (`Authorization: Bearer <secret>`), the
  form a daemon can present without an OIDC flow. Exactly what lives where:
  - **The environment variable named by `secret_env` holds the plaintext secret.** It
    is read once at startup (mirroring the SMTP `password_env` / webhook `secret_env`
    precedent).
  - **Process memory holds only the SHA-256 hash** of that secret; the plaintext is
    not retained past startup, and the server never logs it.
  - **The compiled schema / config JSON holds only the env-var *name*** — never the
    secret, and **never an inline hash**. An inline-`key_hash` form (as
    `StaticApiKeyConfig` uses today) is deliberately **not** offered: putting even a
    hash into the schema JSON — which is built, committed, and distributed — is the
    exact `api_key.rs` posture this ADR set out to fix (credential material travelling
    with the schema). Env indirection is the *only* credential source.
- At authentication the presented bearer secret is SHA-256-hashed and
  **constant-time-compared** against the in-memory hash — reusing the exact posture of
  `ApiKeyAuthenticator` (never store the plaintext, `ct_eq`). This is the one place a
  service account is *credentialed* differently from a human (a shared secret, not a
  JWT); everything downstream is a normal `SecurityContext`.
- **JWT `sub` mapping is a later phase, explicitly out of scope here.** A future
  extension may map an OIDC `sub` (or an `act` claim) to a service account so a daemon
  can present a short-lived JWT instead of a static secret; this ADR fixes the
  identity + ceiling + audit model so that extension is additive.

### 3. Authenticated on the standard bearer path; no parallel plane

Service-account authentication slots into the existing request auth seam alongside the
static-API-key check (`ApiKeyResult::{Authenticated, NotPresent, Invalid}` →
fall-through to JWT). A matched service account produces a `SecurityContext` exactly
like any other authenticated principal; every downstream consumer (RLS, field-authz,
the change log, subscriptions) sees a normal context and needs no service-account
special-casing. This ADR **supersedes the scopes-only static API key** as the
*recommended* service-principal mechanism: a service account is the general case (full
ceiling + env-indirected secret), a static API key the degenerate one (`scopes` only,
in-schema hash).

**The two config blocks coexist; existing keys are not silently migrated.** An entry
under the legacy `[security.api_keys]` block keeps its audit identity
`user_id = apikey:<name>` unchanged — no rename, no behavior change — so audit
consumers and any RLS keyed on that principal string are untouched. The new
`[service_accounts.<name>]` block mints `user_id = service_account:<name>`. Converting a
key to a service account is therefore an **explicit operator choice that changes the
account's audit principal string** (`apikey:<n>` → `service_account:<n>`), and is
documented as such (a breaking change for that one account's audit/RLS identity), never
an automatic rewrite. New principals should use `[service_accounts.<name>]`; existing
keys migrate only when their owner accepts the identity change.

### 4. Recorded as `ActorType::ServiceAccount` — reuse, no new variant

The #390 taxonomy already has `ServiceAccount` (serialized `"service_account"`), and
the API-key path already stamps it. A service account's `SecurityContext` carries
`ActorType::ServiceAccount`, so its writes land in `tb_entity_change_log.actor_type`
automatically via the existing envelope (`SecurityContext.actor_type()` →
`CHANGELOG_PORTABLE_INSERT_COLUMNS`). The account **name** is the `user_id`
(`service_account:<name>`, mirroring `system_job:<id>` / `apikey:<name>`), so an audit
row names the exact account. **No new actor type is introduced** — introducing one
would fragment the taxonomy and the change-log contract for no gain.

> Note: internal system jobs keep `ActorType::SystemJob`; an external service account
> is `ActorType::ServiceAccount`. The distinction is *provenance* (an external
> credentialed caller vs. an internal scheduled job), and both are audit-only, never an
> authorization input.

### 5. Enrichment applies uniformly — **no bypass** (ADR-0016 decision 6)

ADR-0016 established that enriched-identity resolution is *uniform, no bypass —
including service-account / API-key subjects: they need actor rows*. This ADR **holds
that line**: when `[identity.enrichment]` is enabled, a service account is subject to
`sub → DB → identity` resolution like any principal, keyed on its `user_id`
(`service_account:<name>`). Consequences:

- A service account that is meant to read enriched-scoped data must have a provisioned
  **actor row** the enrichment query resolves; absent one, it is **denied**
  (fail-closed), not silently unscoped. This is a feature: the account's read scope is
  its DB row, not a self-asserted claim.
- The inbound-claim stripping (`fraiseql.enriched.*` is removed by the request
  extractor) means a service account **cannot** self-assert enriched fields via its
  credential; any enriched field is server-resolved.
- **Escape hatch, opt-in and explicit:** a `[service_accounts.<name>]` block MAY
  declare `static_enriched = { ... }` fields that the server injects into the
  `fraiseql.enriched.*` namespace *in lieu of* the DB resolve, for a daemon that has no
  natural actor row (e.g. a cross-tenant pipeline). This is the *only* sanctioned
  deviation from 0016's no-carve-out stance, and it is (a) explicit per account, (b)
  server-injected (never token-asserted), and (c) still fail-closed if the declared
  set is incomplete. The default — and the recommendation — is a real actor row.

This makes a service account a first-class **subscriber** too: phase 06 subscription
row-policies resolve the owner filter against the account's enriched identity the same
way they do for a human, so a trusted daemon subscribing to a scoped entity is filtered
(or fail-closed refused) by the same policy path, with no service-account special case.

### 6. Fail-closed everywhere; rotation = config reload

- Unknown account / bad secret ⇒ **401** (indistinguishable — no account-existence
  oracle), never a fall-through to anonymous-with-authority.
- An account declared **without a ceiling** ⇒ anonymous authority: RLS/field-authz deny
  writes (the same words as the sources `run_as` docs).
- **Rotation is a config reload:** update the value of the environment variable named
  by `secret_env` and reload; there is no online rotation API and no stored hash to
  edit (the hash lives only in process memory, re-derived from the env var each boot).
  This ADR is **not** a secrets manager — the
  `crates/fraiseql-secrets` backend remains the place for managed secret resolution if
  an adopter wants it; here the credential is a hash + an env indirection.

### Rejected alternatives

- **Status quo — a per-deployment sidecar.** Leaves the authority boundary outside
  FraiseQL: undocumented, unauditable, and re-invented per adopter. This is the problem.
- **Reuse human OIDC/JWT for daemons.** Pushes service authority into the IdP's token
  minting (the exact coupling ADR-0016 rejected for reads), needs a client-credentials
  flow for a process that just wants a static secret, and gives no `run_as` ceiling. The
  *future* `sub → service account` mapping (decision 2) keeps the ceiling in FraiseQL.
- **A new `ActorType::ExternalService` variant.** Fragments the #390 taxonomy and the
  change-log contract; `ServiceAccount` already means "a non-human credentialed
  caller." Rejected in favor of reuse.
- **A blanket enrichment carve-out for service accounts.** Rejected against ADR-0016
  decision 6 — a silent "except for these principals" hole. The `static_enriched`
  escape hatch (decision 5) is the bounded, explicit, per-account alternative.
- **A dedicated `/service-token` issuance endpoint + rotation API.** Scope creep toward
  a secrets manager; config-reload rotation + the existing secrets crate cover the need.

---

## Consequences

- The scopes-only static API key (`api_key.rs`) is **subsumed** by service accounts:
  the same authenticator seam, extended to carry a full `run_as` ceiling and an
  env-indirected secret. Existing keys keep working (a service account with scopes and
  no roles/tenant); the config surface is what changes.
- **No new authority type, no new actor type, no new change-log column** — the value is
  entirely in *binding* the existing `RunAs` ceiling + `ActorType::ServiceAccount` to an
  external credential. Implementation reuses `SecurityContext::system_job` /
  `RunAs::identity` and the API-key hash/compare, per phase 02's shared-ceiling
  extraction.
- A service account is a first-class principal for **every** downstream: RLS,
  field-authz, the change log, and — via decision 5 — phase-06 subscription policies,
  with no special-casing anywhere.
- Enrichment coupling (decision 5) means an enrichment-enabled deployment must
  provision an actor row (or declare `static_enriched`) for each service account that
  reads scoped data — the same operational cost every enriched principal already pays,
  and strictly safer than an unscoped service identity.
- Credential security rests on the operator: a strong secret in `secret_env`, rotated by
  reload. The server never logs the secret and stores only its hash; a leaked secret is
  contained by the account's ceiling (it can do no more than its `run_as` grants).
- **Out of scope, tracked for follow-up:** JWT `sub → service account` mapping; online
  rotation; a managed-secrets (`fraiseql-secrets`) credential source. The identity +
  ceiling + audit model here is designed so each is additive.

---

## Implementation note

Accepted; implementation may proceed. The minimal first implementation is: the
`[service_accounts.<name>]` config (secret via env indirection, `deny_unknown_fields`,
**no** inline hash), authentication on the existing bearer/API-key seam, the ceiling
minted through the shared `RunAs::identity` path with `ActorType::ServiceAccount`, and a
conformance test — a service principal reads via a policy-scoped subscription (phase 06)
and writes under its ceiling, with audit rows carrying the `service_account:<name>`
identity, plus the fail-closed cases (unknown account / bad secret → 401; no ceiling →
anonymous). A public issue (generic framing + evidence + this shape) is filed for
visibility ahead of the work.
