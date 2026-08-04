# Audit logging: the actor model

Every mutation FraiseQL executes writes a change-log row
(`core.tb_entity_change_log`, the Change Spine outbox) inside the same
transaction as the mutation itself. Since v2.6.0 that row carries a first-class
**actor classification**: who — or what — performed the action.

| `actor_type` | Meaning |
|---|---|
| `human_user` | An ordinary end-user JWT. The default classification. |
| `service_account` | A non-human credentialed caller: an API key, or a JWT carrying the `service_account` scope. |
| `ai_agent` | An autonomous agent acting on behalf of a user, identified by an RFC 8693 `act` delegation claim. The delegated human is recorded separately in `acting_for`. |
| `system_job` | An internal scheduled / system-triggered job (a `Source` firing, a functions-runtime dispatch). Never derived from a token — constructed explicitly by the server. |

The same classification is stamped onto the tenant lifecycle audit log
(`actor_type` / `acting_for_user_id`) and surfaced by the change-log reader and
the NATS bridge envelopes.

## Derivation rules

The classification is derived once, when the validated token becomes a
`SecurityContext` — the single builder every transport (GraphQL, REST, MCP,
gRPC) calls. First match wins:

1. **`act` claim present** (RFC 8693 token exchange) → `ai_agent`, with
   `acting_for` = the token's top-level `sub` (the human being acted for),
   when it is UUID-shaped. Per RFC 8693, `sub` is the subject and `act` names
   the acting agent — the agent's own identity stays in the `act` claim.
2. **`service_account` scope present** → `service_account`.
3. **Otherwise** → `human_user`.

API-key authentication classifies `service_account` explicitly at its
construction site. `system_job` is only ever constructed by internal callers
(`SecurityContext::system_job`), with a fail-closed `run_as` ceiling.

## What a client cannot do

- **Forge the classification.** Claims in the framework-reserved `fraiseql.`
  namespace (e.g. a claim literally named `fraiseql.actor_type`) are stripped
  before the token's claims reach the security context, and a look-alike bare
  `actor_type` claim is simply ignored — derivation reads the token's
  *structure* (the `act` claim, the scopes), not client-named fields.
- **Write unattributed.** With authentication configured, an unauthenticated
  mutation is refused at the door — it never executes, so it never records an
  unattributed row. (Deployments running without authentication record `NULL`
  — `fraiseql doctor` warns about those rows.)
- **Write an out-of-range value.** Migration 08 installs a database `CHECK`
  constraint (`chk_entity_change_log_actor_type`) that refuses any
  `actor_type` outside the four canonical tokens on every new write.

The classification is *recorded*, never consumed by an engine authorization
decision. An application that wants actor-aware policy (e.g. "agents cannot
delete a tenant") can read `actor_type` from the `SecurityContext` in its own
`Authorizer` — trusting it exactly as far as it trusts its IdP's `act`
issuance.

## Forensic queries

```sql
-- Every action an automated process took on behalf of user X, last 30 days.
SELECT created_at, object_type, modification_type, object_id
FROM core.v_entity_change_log
WHERE actor_type = 'ai_agent'
  AND acting_for = '5a1e0000-0000-4000-8000-000000000390'
  AND created_at > now() - interval '30 days'
ORDER BY created_at DESC;

-- Everything service accounts wrote, by day.
SELECT date_trunc('day', created_at) AS day, count(*)
FROM core.v_entity_change_log
WHERE actor_type = 'service_account'
GROUP BY 1 ORDER BY 1 DESC;
```

## Operational checks

`fraiseql doctor --against-db <url>` includes an actor-attribution check:

- **Fail** — rows whose `actor_type` is outside the canonical token set
  (a rogue or pre-constraint writer).
- **Warn** — the `CHECK` constraint is missing (re-apply migration 08 or
  re-run `fraiseql setup`), or rows carry `NULL` `actor_type`
  (pre-actor-model rows, or writes made without authentication).

See also: `docs/architecture/change-log-contract.md` for the full column
contract, and ADR-0018 for service-account identities.
