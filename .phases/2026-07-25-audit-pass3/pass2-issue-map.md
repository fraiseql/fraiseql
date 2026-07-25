# Pass-2 filed issues (#739–#788) — dedup reference for pass 3

Pass 1 filed #715–#738 (see the crate grades + mapping in the memory file / issue tracker). Pass 2 filed the 50 issues below from 65 adversarially-verified findings. Do NOT re-report any of these in pass 3.

| Issue | Labels | Location | Title |
|---|---|---|---|
| #739 | security,bug | `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:862` | query_regular: execute_query_direct resolves inject_params then discards them, dropping tenant/owner row scoping on the entire REST read surface |
| #740 | bug | `crates/fraiseql-core/src/cache/result.rs:257` | cache: moka Replaced/Expired eviction listener detaches re-cached entries from invalidation indexes, making them uninvalidatable |
| #741 | bug | `crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs:1336` | cache: CREATE mutations that return entity_id are misrouted to entity-aware eviction, leaving all cached list queries stale |
| #742 | bug | `crates/fraiseql-core/src/cache/result.rs:422` | cache: list-query classification by result row count (len > 1) means empty and single-row list results are never invalidated on CREATE |
| #743 | security,bug | `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:716` | core-security-rbac: field-level requires_scope RBAC is bypassed for unauthenticated requests |
| #744 | bug | `crates/fraiseql-federation/src/saga_executor/orchestrator.rs:77` | saga: forward replay re-executes already-Completed steps (recovery double-executes committed mutations) |
| #745 | bug | `crates/fraiseql-federation/src/saga_store.rs:1297` | saga: claim_stuck_sagas has no staleness threshold — recovery loop claims and concurrently re-drives actively-executing sagas |
| #746 | bug | `crates/fraiseql-federation/src/saga_coordinator/coordinator.rs:503` | saga: cancel_saga and execute_saga discard CompensationResult — report compensated=true and mark saga Cancelled even when zero steps actually rolled back |
| #747 | bug | `crates/fraiseql-federation/src/mutation_http_client.rs:316` | saga: mutation dispatch is at-least-once with no idempotency token — transport retries and step timeouts duplicate non-idempotent mutations |
| #748 | bug | `crates/fraiseql-server/src/api/rbac_management/db_backend.rs:90` | rbac: ensure_schema SQL is a PostgreSQL syntax error — setting admin_token bricks server boot and RBAC API has never worked against real PG |
| #749 | security,bug | `crates/fraiseql-server/src/routes/studio/auth_users.rs:131` | studio: admin mutation endpoints return fabricated success — session revoke, secret set/delete, row mutate, and user invite are silent no-ops |
| #750 | bug | `crates/fraiseql-server/src/routes/graphql/app_state.rs:387` | server: schema hot-reload rebuilds the executor with RuntimeConfig::default(), silently dropping audit logging, page-size ceiling, changelog toggle, and relay dispatch |
| #751 | security,bug | `crates/fraiseql-server/src/inbound/webhook.rs:254` | inbound-webhook: replay/dedup protection keyed on unsigned, attacker-controlled headers (webhook-id / x-github-delivery) |
| #752 | security,bug | `crates/fraiseql-auth/src/totp_mfa/mod.rs:379` | totp_mfa: MFA verify has no attempt limit and does not consume the challenge on a wrong code — TOTP is brute-forceable into a full victim session |
| #753 | bug | `crates/fraiseql-auth/src/session_postgres.rs:108` | auth: PostgresSessionStore HMAC fallback signs access tokens with a per-token random key that is immediately discarded — every issued token is unverifiable |
| #754 | security,bug | `crates/fraiseql-server/src/server/extensions.rs:67` | server: with_relay_pagination/with_flight_service silently drop compiled [subscriptions] config (webhook auth hooks + per-connection limit) and [pool_tuning] |
| #755 | bug | `crates/fraiseql-cli/src/schema/merger.rs:530` | cli: SchemaMerger drops enums/input_types/interfaces/unions/subscriptions/observers/custom_scalars/sources from SDK JSON on every TOML-workflow compile path |
| #756 | bug | `crates/fraiseql-cli/src/schema/merger.rs:487` | cli: TOML-declared query/mutation arguments are silently dropped — merger emits "args"/"required" but IntermediateSchema deserializes "arguments"/"nullable" |
| #757 | bug | `crates/fraiseql-cli/src/config/security.rs:467` | seam: [fraiseql.security] role_definitions/default_role/tenant_claim compiled as camelCase keys the core SecurityConfig deserializes as snake_case — field-level RBAC grants silently empty |
| #758 | security,bug | `crates/fraiseql-core/src/runtime/subscription/manager.rs:387` | seam: security.multi_tenant has no producer — subscription tenant fail-closed gate and cache+RLS boot gate can never activate |
| #759 | bug | `crates/fraiseql-core/src/runtime/executor/support/classify.rs:131` | classify: multi-root mutations silently execute only the first root field and drop the rest |
| #760 | bug | `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:493` | cache: response-cache key omits the nested selection set and nested field arguments — different queries collide on one cached response |
| #761 | bug | `crates/fraiseql-core/src/cache/adapter/query.rs:193` | cache: additional_views is authored, CLI-validated, and documented as required for correct invalidation, but no runtime path ever consumes it |
| #762 | security,bug | `crates/fraiseql-core/src/cache/adapter/mod.rs:540` | cache: validate_rls_active checks the row_security GUC (default 'on'), so the documented RLS safety gate passes vacuously on any stock PostgreSQL |
| #763 | bug | `crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs:1337` | cache: entity-aware eviction on UPDATE misses list entries the updated row newly matches; acknowledged as 'no false positives' but has false negatives |
| #764 | bug | `crates/fraiseql-federation/src/entity_resolver.rs:235` | federation: _entities database errors are swallowed into null entities — router receives data:[null,…] with no GraphQL error |
| #765 | bug | `crates/fraiseql-federation/src/saga_executor/prefetch.rs:124` | saga: @requires pre-fetch builds an invalid _entities selection for dotted field paths (object field requested without subselection) |
| #766 | bug | `crates/fraiseql-federation/src/saga_recovery_manager.rs:374` | saga: recovery replays remote-subgraph steps against the local SQL adapter (wrong-database writes) instead of failing loud |
| #767 | bug | `crates/fraiseql-federation/src/saga_compensator.rs:319` | saga: get_compensation_status fabricates results via magic-key sniffing of forward payloads and hardcodes failed_steps empty (PartiallyCompensated unreachable) |
| #768 | bug | `crates/fraiseql-server/src/api/rbac_management.rs:351` | rbac: GET /api/audit/permissions is a façade returning a hard-coded empty array — audit trail claims with zero recording |
| #769 | bug | `crates/fraiseql-server/src/api/rbac_management.rs:167` | rbac: tenant scoping is inert and role listing silently truncates at 100 — handlers hard-code tenant None and pagination 100/0 |
| #770 | security,bug | `crates/fraiseql-server/src/token_revocation.rs:655` | token-revocation: malformed redis_url silently downgrades the revocation store to per-replica in-memory |
| #771 | security,bug | `crates/fraiseql-server/src/routes/subscriptions.rs:602` | subscriptions: JWT expiry/revocation never re-checked mid-stream — RLS-scoped event delivery continues indefinitely on an expired token (in-code A44 TODO) |
| #772 | bug | `crates/fraiseql-server/src/observers/runtime.rs:1099` | subscriptions: silent event loss under backpressure — bridge try_send drops CDC events at capacity 100 and broadcast Lagged skips events with no client notification |
| #773 | bug | `crates/fraiseql-server/src/subscriptions/event_bridge.rs:167` | subscriptions: EventBridge defaults unknown operations to Create — Debezium 'r' (snapshot/read) rows delivered to subscribers as phantom created events |
| #774 | bug | `crates/fraiseql-server/src/server/builder.rs:329` | server: compiled-schema rate limiting silently shadows CLI/env rate-limit overrides, inverting the documented CLI>env>config precedence |
| #775 | bug | `crates/fraiseql-server/src/inbound/email/sink.rs:148` | inbound-email: spine dedup keyed on sender-controlled Message-ID globally across all mailboxes — cross-mailbox message loss and pre-claim drop attack |
| #776 | security,bug | `crates/fraiseql-auth/src/oidc_provider.rs:163` | oidc: issuer SSRF allow-list bypassed by IPv4-mapped IPv6 and 0.0.0.0, contradicting its own IMDS-blocked claim |
| #777 | bug | `crates/fraiseql-server/src/server/initialization.rs:122` | server/init: configured Redis PKCE backend silently and permanently downgrades to in-memory when Redis is unreachable at boot |
| #778 | security,bug | `crates/fraiseql-server/src/server/initialization.rs:222` | server: rate_limiter_from_schema swallows security.rate_limiting deserialize errors with .ok() — malformed config silently disables rate limiting and the production proxy-trust guard |
| #779 | bug | `crates/fraiseql-cli/src/schema/converter/mod.rs:191` | cli: SDK-authored observers are validated then silently discarded — converter hardcodes observers: Vec::new() |
| #780 | documentation | `crates/fraiseql-core/src/schema/config_types.rs:766` | seam: GrpcConfig claims to be 'compiled from [grpc] in fraiseql.toml' but no compile path can produce it — gRPC transport unreachable |
| #781 | bug | `` | webhooks: signature verifiers reject every genuine delivery (LemonSqueezy hex-vs-Base64; slack/discord/sendgrid/twilio timestamp/url not threaded) |
| #782 | bug | `` | server: schema hot-reload leaves subsystems, naming acronyms, and boot-safety gates stale |
| #783 | security,bug | `` | server/arrow: with_flight_service never wires the OIDC validator (feature-on path and cfg(not(auth)) arm both drift from from_executor) |
| #784 | bug,documentation | `` | core: low-severity second-pass audit checklist |
| #785 | bug | `` | federation/saga: low-severity second-pass audit checklist |
| #786 | bug | `` | server: low-severity second-pass audit checklist |
| #787 | bug | `` | webhooks: low-severity second-pass audit checklist |
| #788 | security,bug | `` | auth: low-severity second-pass audit checklist |
