# ADR-0016: Enriched-identity Resolution

## Status: Accepted

This ADR records the decisions behind **enriched-identity RLS** (#539): resolving
an application's internal identity from its own database, once per request,
cached and fail-closed, and feeding it to read-scoping and verified
sender-identity.

Related: `docs/architecture/enriched-identity-rls.md`, ADR-0014 (federation
entity backing source), the native-runtime hardening train (send path).

---

## Context

An IdP asserts a stable subject (`sub`). Authorization, however, lives in the
application's database: `sub → actor_id / actor_role` for reads,
`sub → verified from-address` for sends. Two ways to bridge that gap:

1. Mint the DB-derived attributes into the JWT as raw claims. This pushes
   app-owned, DB-sourced authorization out into the IdP, and makes the RLS
   boundary depend on token minting.
2. Resolve `sub → identity` in FraiseQL against the app's DB, per request.

Predecessor #242 shipped a claims-enrichment query but only on the OIDC
`/auth/me` response — it never touched the security context, never ran under
HS256, and used a fail-**open** `LIMIT 1` lookup. It was stranded on `main` and
never forward-ported.

---

## Decision

Resolve `sub → DB → identity` with **one primitive**, one cache policy, one
failure model, wired to two consumers.

1. **Fail-closed at source.** Anything other than exactly one row with every
   mapped field present and non-null is a denial; a denial fails the operation
   (403 for the sync read path, refuse/DLQ for the durable send path). Never a
   silent skip, never an empty-string GUC, the mapped set all-or-nothing. A
   transient DB error fails the request (503) rather than falling through to an
   unscoped query. `> 1 row` is a hard fail (refining #242's silent `LIMIT 1`).

2. **A reserved, forge-proof namespace.** Resolved fields merge under
   `fraiseql.enriched.<field>`. The request extractor strips incoming `fraiseql.`
   claims, so the namespace can only be written by the resolver.
   `SessionVariableSource::Enrichment` / `InjectedParamSource::Enrichment` read
   **only** that namespace, with no fallback — so a raw claim of the same name
   can never impersonate a DB-derived field.

3. **Trigger = `enabled = true` alone.** Every authenticated request fail-closes
   when enrichment is enabled, independent of whether the operation reads an
   enriched field — a declaration-conditional check would reintroduce the exact
   silent-skip this design fights. Enabled-with-no-consumer is a loud startup
   warning; zero-cost belongs to disabled/absent only.

4. **Cache key = the bound-`$param` tuple**, not bare `sub`. `sub` is unique only
   per issuer, and FraiseQL speaks multi-IdP; keying on the bound parameters makes
   cache correctness track the `WHERE` clause exactly (a multi-issuer app binds
   `$iss`). Positive TTL bounded (default 60s) so a revocation propagates within
   that window or immediately via `flush(sub)`; short negative TTL; `Unavailable`
   never cached.

5. **Top-level `[identity]` config**, not nested under `[auth]`, so it applies
   under any auth mode — HS256/OIDC parity by construction (this is *why* #242
   never ran under HS256).

6. **Uniform, no bypass** — including service-account / API-key subjects: they
   need actor rows. An "except for these principals" carve-out is the silent hole
   the design closes.

7. **One primitive, two consumers.** The read path merges into the security
   context; the send path resolves `sub → verified from-address` through an
   object-safe `SenderIdentityResolver` seam. The whole DB-executing resolver
   lives in `fraiseql-server` (sqlx is dev-only in `fraiseql-functions`); core
   gets only the config variants and the namespaced read; the send seam is an
   object-safe trait (a boxed future, no new dyn-dispatch trait-macro).

---

## Consequences

- The RLS boundary no longer depends on token minting; identity is the app's DB
  row. An unknown subject is denied *before* any data query — strictly stronger
  than relying on every view author to deny on `NULL`.
- Request availability is coupled to enrichment-DB availability for enabled
  apps — acceptable, because it is the same DB the request's data query hits.
- Actors must be provisioned out-of-band (see the architecture doc); a
  first-authenticated-request provisioning flow would deadlock.
- A role change / revocation is visible within `cache_ttl_secs` (default 60s) or
  immediately via `flush(sub)`.
- Denials are debuggable server-side (WARN with reason + subject) while the
  outward body stays generic (no actor-table existence oracle).
- The `send_email` host op and SMTP transport that consume the sender seam are
  the native-runtime hardening train's; #539 lands the resolver and the seam.
