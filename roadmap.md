# FraiseQL Roadmap

The roadmap is the open issue list. What shipped in each version is in
[CHANGELOG.md](CHANGELOG.md); what is deprecated and when it goes is in
[DEPRECATIONS.md](DEPRECATIONS.md). This page lists the open work that is larger than a fix,
by issue number, so that every claim here is a link to something that can be checked. Anything
not listed is not planned; open an issue to propose it.

## Before the next tag

The next release is tagged when the open backlog is closed:
<https://github.com/fraiseql/fraiseql/issues?q=is%3Aissue+is%3Aopen>.

## Larger open items

| Area | Issue | What it is |
|---|---|---|
| Webhooks | [#1323](https://github.com/fraiseql/fraiseql/issues/1323) | Standard Webhooks signature scheme, used by Clerk and every other Svix sender |
| Webhooks | [#1322](https://github.com/fraiseql/fraiseql/issues/1322) | `jwt-jwks` signature scheme, used by Hanko, Kinde and FusionAuth |
| Inbound spine | [#1175](https://github.com/fraiseql/fraiseql/issues/1175) | A replay path so `after:ingest` dispatch is at-least-once |
| Auth | [#1088](https://github.com/fraiseql/fraiseql/issues/1088), [#1089](https://github.com/fraiseql/fraiseql/issues/1089) | Tenant-scoped account store and admin principal for SAML deployments |
| Tenancy | [#633](https://github.com/fraiseql/fraiseql/issues/633) | Metering and enforcement of `max_storage_bytes`, advisory today |
| Tenancy | [#444](https://github.com/fraiseql/fraiseql/issues/444) | Optional per-schema change-log tables for schema-per-tenant deployments |
| Authorization | [#626](https://github.com/fraiseql/fraiseql/issues/626) | Compiled-schema declarative authorization engine |
| Observers | [#428](https://github.com/fraiseql/fraiseql/issues/428) | Real SMS, push and search transports; the config rejects those action types today |
| Schema contract | [#965](https://github.com/fraiseql/fraiseql/issues/965), [#995](https://github.com/fraiseql/fraiseql/issues/995) | `compile --emit-ddl` as a versioned desired-state artifact, and which artifact is canonical |
| Schema intelligence | [#963](https://github.com/fraiseql/fraiseql/issues/963) | Spec-to-migration generation, a risk-classified policy gate, online orchestration |
| Query surface | [#1159](https://github.com/fraiseql/fraiseql/issues/1159) | `{Entity}OrderByInput.field` as an enum of the sortable keys |
| Pagination | [#1306](https://github.com/fraiseql/fraiseql/issues/1306) | Whether deep offset pagination should steer clients to cursors |
| Vector search | [#1314](https://github.com/fraiseql/fraiseql/issues/1314) | Saying so when a filtered ANN search gives up before finding k results |
