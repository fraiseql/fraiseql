# SCIM 2.0 provisioning

FraiseQL implements the SCIM 2.0 provisioning surface (RFC 7643 / RFC 7644) so an enterprise
IdP — Okta, Entra, OneLogin — can create, update, deactivate and delete users and groups.

> **Why this is a security feature, not only an integration one.** SAML covers
> *authentication*. Without provisioning, an offboarded employee's FraiseQL account stays
> active: SAML stops them signing in *through the IdP*, and every other credential on the
> same account — a local password, a social link — keeps working. `active = false` is what
> closes that, and it is the part of this surface with tests devoted to it.

## Enabling it

```toml
[scim]
enabled = true
# Externally reachable base of the SCIM surface; provisioning clients follow the
# meta.location and $ref URLs built from it.
base_url = "https://api.example.com/scim/v2"
```

Requires a database pool and an `admin_token`. Boot refuses `enabled = true` without the
latter: provisioning credentials are minted through the admin API, so without one the
surface would exist with no way to authenticate to it.

## Two credentials, deliberately

| Surface | Credential | Grants |
|---|---|---|
| `/scim/v2/*` | provisioning bearer token | provisioning, and nothing else |
| `/api/scim/tokens` | admin bearer token | minting and revoking provisioning tokens |

A provisioning credential is handed to an IdP and configured there ~forever. If it doubled
as the admin bearer, every SCIM integration would also carry the ability to rewrite roles
and IdP configuration. The separation is structural — nothing but the SCIM router reads the
provisioning-token table — and it is asserted in both directions by the e2e suite.

```bash
# Mint one (the token is shown exactly once; only sha256(token) is stored)
curl -X POST https://api.example.com/api/scim/tokens \
  -H "Authorization: Bearer $ADMIN_TOKEN" -H 'Content-Type: application/json' \
  -d '{"idp_name": "acme-okta", "tenant_id": "…-uuid-…"}'

# Revoke it
curl -X DELETE https://api.example.com/api/scim/tokens/<id> -H "Authorization: Bearer $ADMIN_TOKEN"
```

The tenant is a property of the **credential**, never of the request, so one IdP cannot
provision into another's tenant — there is no request field that could say otherwise.

## Endpoints

| Route | Notes |
|---|---|
| `GET/POST /scim/v2/Users` | `filter=userName eq "…"`, `startIndex`, `count`, `attributes`, `excludedAttributes` |
| `GET/PUT/PATCH/DELETE /scim/v2/Users/{id}` | `ETag` / `If-Match` concurrency |
| `GET/POST /scim/v2/Groups` | `filter=displayName eq "…"` |
| `GET/PUT/PATCH/DELETE /scim/v2/Groups/{id}` | |
| `POST /scim/v2/.search`, `/Users/.search`, `/Groups/.search` | RFC 7644 §3.4.3 |
| `GET /scim/v2/ServiceProviderConfig`, `/ResourceTypes`, `/Schemas` | discovery |

### Deactivation

`active = false` — whether by `PATCH` or by a `PUT` that flips it — does two things:

1. **revokes every existing session**, so access ends now rather than when a refresh token
   happens to expire; and
2. **blocks new sessions**, enforced at session creation — the one point every credential
   path converges on (password login, the MFA second factor, social callback, email and
   phone OTP, the SAML ACS).

Reactivation restores both. A principal with no account row — an anonymous or JWT-only
session — is a different identity space and is unaffected.

### Groups become permission-less roles

A SCIM group is mirrored onto an RBAC role and its members onto role assignments. Creating a
group creates a role with **no permissions**: a provisioning credential that could grant
permissions would be an admin credential under another name. The IdP decides *who is in* a
role; a FraiseQL admin decides *what it may do*, through `/api/roles`.

### Filtering is strict

Only `attribute eq "value"` is supported, on `userName` for users and `displayName` for
groups — the shape provisioning clients actually send. **Anything else is refused with
`400 invalidFilter` rather than ignored.** Dropping a filter we did not understand would
answer "does this user exist?" with the entire directory, and the client would read the
first row as a match — provisioning onto the wrong account.

## Conformance

RFC 7644's shapes are exercised in CI by **`scim2-tester`**, a third-party SCIM 2.0 client,
via `tools/scim-conformance.py` in the Dagger `saml` leg. That matters: a suite written here
passes on the request shapes we thought of, which is exactly the failure mode to avoid — the
third-party run found several real defects the hand-written tests had missed.

Okta's SCIM validator and the Entra provisioning agent are hosted services needing a public
URL and a vendor tenant, so neither can run in CI; validating against a real tenant remains
a manual pre-release step.

Two known deviations are carried in `ACCEPTED_DEVIATIONS` with their reasons, plus one
tracked defect ([#1090]):

- **`active` cannot be removed, only set.** It is the offboarding switch and is `NOT NULL`
  by design; a nullable deactivation flag would mean deciding whether `NULL` is active — a
  fail-open hazard on the one attribute this feature exists to enforce.
- **One primary email per account.** `core.tb_user.email` is the cross-provider
  account-linking key, so a second address would either be invisible to linking or silently
  widen it.

[#1090]: https://github.com/fraiseql/fraiseql/issues/1090
