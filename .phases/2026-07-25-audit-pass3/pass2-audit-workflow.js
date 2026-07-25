export const meta = {
  name: 'fraiseql-pass2-audit',
  description: 'Second-pass bug-hunting audit of FraiseQL: parallel area reviewers -> adversarial per-finding verifiers',
  phases: [
    { title: 'Review', detail: 'one reviewer per shadow area, reads files in full' },
    { title: 'Verify', detail: 'adversarial refute-by-default per candidate finding' },
  ],
}

const DEDUP = `PASS 1 already filed these — DO NOT re-report (these exact defects are known):
#715 arrow build_insert_query two-arg to_timestamp/NaN; #716 arrow Flight cache keyed on SQL text (cross-tenant);
#717 arrow batched one schema header for N schemas; #718 ClickHouse flush timer reset / ES sink drops client;
#719 core value_json incomplete JSON escaping + $-prefix collision; #720 core input-validation fail-open (Display round-trip, malformed rules ignored, Length counts bytes);
#721 cli non-PG sql_templates broken; #722 db LIKE escaping no ESCAPE clause over-matches SQLite/SQLServer;
#723 cli compile schema-load fallback swallows [domains]/includes errors; #724 cli schema-validator diagnostic gaps (panic in suggest_similar_type);
#725 auth compare_padded/compare_jwt_constant equal beyond 512 bytes; #726 secrets Vault renew_token unreachable through Arc<dyn>;
#727 secrets zeroization stops at trait boundary / SSRF guard all-or-nothing / tls_verify footgun; #728 federation composition validator doesn't enforce documented rules + value_to_sql_literal dialect-unaware;
#729 wire soft_limit_warn ignored / pause() corrupts snapshot; #730 server raw client variables logged at warn! on GET parse fail;
#731 server edge polish (413-doc, literal-IP SSRF, limiter eviction, REST coercion, 408); #732 server decompose execute_graphql_request;
#733 TS SDK decorator façade emits empty schema; #734 docs onboarding broken (phantom fraiseql.config(), wrong flag/ports, unrunnable examples/basic);
#735 chore remove committed archaeology; #736 core polish (dead tenancy string-SQL helpers, GATE-1 dup); #737 auth polish (PKCE capacity TOCTOU, un-zeroized state key); #738 cli/codegen polish (acronym-naive to_snake_case, unescaped */ in TS docs).
ALSO already-open, do not re-file: #611 subscriptions row-visibility fixed at start (not propagated on hot-reload); #677 type-level requires_role never enforced; #687/#653 cascade node-vs-value classification; #650 fraiseql run -d flag collision; #569 tb_entity_change_log install; #374 multi-db parity; #634/#633/#632/#631 observer metrics/actions; #628-#621 config-inert sections; #621 PKCE OAuth-client compiled path.`

const PREAMBLE = `You are a senior bug-hunting reviewer for FraiseQL, a compiled GraphQL->SQL engine in Rust (~840k LOC, workspace at /home/lionel/code/fraiseql). This is PASS 2 of a quality audit.

${DEDUP}

META-PATTERN (this is where real defects live — steer by it): pass 1 found that defects cluster in SILENT FAIL-OPEN on code paths that are never exercised against a real system. The Postgres happy path is clean and heavily tested. Spend your effort in the SHADOWS: error branches, secondary/fallback paths, cfg(feature) arms where one arm drifted, façade/placeholder code, hand-built SQL or JSON via format!, and specifically these seams:
  - .ok()? or .ok().flatten() on parse/deserialize results (silently drops -> can WIDEN a filter or skip a check)
  - unwrap_or_default() / unwrap_or_else that turns an error into a permissive default
  - if let Ok(...) { } else fallback chains that swallow real errors
  - let _ = <result>  (dropped Result — did the side effect actually happen?)
  - magic string sentinels ("$"-prefix, "..."-markers, "__"-prefixes) used for in-band signalling that collides with real data
  - structured errors round-tripped through Display/to_string then re-parsed
  - doc/comment claims ("constant-time", "parameterized", "validated at creation", "enforced", "atomic", "idempotent") — verify EACH against the actual code; a false claim is a finding
  - fail-OPEN on the security side: an auth/RBAC/visibility check that returns "allow" on error, on missing config, or on an unrecognized value

CALIBRATION — do NOT waste findings on: style, naming, missing docs, missing // Reason: on #[allow], unwrap in #[cfg(test)] code, or anything clippy pedantic+deny / unwrap_used=deny / missing_docs=deny already blocks. A finding MUST have a concrete failure scenario: specific inputs/state -> specific wrong output, crash, leak, or contract violation. "This could be cleaner" is not a finding. If the worst case is a cosmetic log message, it's at most low severity.

METHOD (follow exactly):
1. Enumerate your assigned files (ls / glob), then Read each one IN FULL — not grep excerpts. You must understand the whole module.
2. For each candidate defect, trace the value from where it is produced to where it is consumed. Confirm the bad path is REACHABLE in a real deployment (not dead code, not test-only, not gated behind an impossible cfg).
3. Adversarially re-check your top findings before returning: try hard to REFUTE each. Keep only what survives, with an honest confidence.
4. Return raw structured data (you are a tool; your output is parsed, not read by a human). No prose preamble.`

const FINDINGS_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['area', 'files_read', 'findings'],
  properties: {
    area: { type: 'string' },
    files_read: { type: 'array', items: { type: 'string' }, description: 'repo-relative paths you actually Read in full' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['title', 'severity', 'category', 'file', 'line', 'summary', 'details', 'failure_scenario', 'suggested_fix', 'confidence'],
        properties: {
          title: { type: 'string', description: 'house style: "area: concise summary"' },
          severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low'] },
          category: { type: 'string', enum: ['bug', 'security', 'documentation', 'enhancement'] },
          file: { type: 'string' },
          line: { type: 'integer' },
          summary: { type: 'string', description: 'one sentence' },
          details: { type: 'string', description: 'what the code does, with file:line references and the exact seam' },
          failure_scenario: { type: 'string', description: 'concrete inputs/state -> concrete wrong behavior' },
          suggested_fix: { type: 'string' },
          confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['verdict', 'severity', 'reachable', 'reasoning', 'refutation_attempt', 'runtime_repro'],
  properties: {
    verdict: { type: 'string', enum: ['CONFIRMED', 'PLAUSIBLE', 'REJECTED'] },
    severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low'] },
    reachable: { type: 'boolean', description: 'is the bad path reachable in a real deployment (not dead/test-only)' },
    duplicate_of: { type: 'string', description: 'issue number if this duplicates an already-filed finding, else empty' },
    reasoning: { type: 'string' },
    refutation_attempt: { type: 'string', description: 'the strongest case you could make AGAINST the finding, and why it does or does not hold' },
    runtime_repro: { type: 'string', description: 'if verdict claims a runtime failure: the exact test/command/steps to prove it, else empty' },
    corrected: { type: 'string', description: 'if the finding is real but mis-described, the corrected summary; else empty' },
  },
}

const AREAS = [
  {
    key: 'core-executor-mutation',
    prompt: `AREA: fraiseql-core mutation & query execution runners.
FILES: crates/fraiseql-core/src/runtime/executor/ (core.rs, execution.rs, mutation.rs, security.rs, context.rs, mod.rs, support/), crates/fraiseql-core/src/runtime/executor/runners/ (mutation/mod.rs, query.rs, query_params.rs, query_projection.rs, query_relay.rs, query_regular.rs, aggregate.rs), crates/fraiseql-core/src/runtime/mutation_result.rs, crates/fraiseql-core/src/runtime/input_validator.rs (only NEW bugs beyond #720/#719), crates/fraiseql-core/src/runtime/tenant_enforcer.rs.
HUNT: mutation dispatch selecting the wrong runner; parameter binding order/coercion; error branches that fall through to a permissive default; tenant_enforcer bypass on missing/blank tenant; result envelope shape mismatches; auto_error_union / typename handling; SETOF vs single-row function result parsing; NULL/absent field vs explicit-null confusion in mutation inputs.`,
  },
  {
    key: 'core-cascade-cache-apq',
    prompt: `AREA: fraiseql-core cascade change-spine, cache invalidation, and APQ.
FILES: crates/fraiseql-core/src/runtime/cascade.rs, crates/fraiseql-core/src/cache/ (cascade_invalidator.rs, cascade_metadata.rs, cascade_response_parser.rs, dependency_tracker.rs, invalidation.rs, invalidation_api.rs, query_analyzer.rs, response_cache.rs, relay_cache.rs, entity_key.rs, key.rs, uuid_extractor.rs, fact_table_cache.rs, fact_table_version.rs), crates/fraiseql-core/src/apq/ (hasher.rs, storage.rs, memory_storage.rs, redis_storage.rs, metrics.rs).
HUNT: cache key that omits a tenant/role/variable dimension (stale or cross-tenant serve); invalidation that parses the cascade log with .ok()? and silently under-invalidates (serves stale after write); APQ SHA-256 hash mismatch handling or hash-not-checked-on-store (cache poisoning); uuid_extractor magic-prefix parsing collisions; dependency_tracker missing an edge so a dependent query is never invalidated; interplay where APQ-cached query bypasses cache-invalidation registration.`,
  },
  {
    key: 'core-security-rbac',
    prompt: `AREA: fraiseql-core security: introspection, complexity/depth limits, RBAC & field authorization.
FILES: crates/fraiseql-core/src/security/ (introspection_enforcer.rs, query_validator.rs, authorizer.rs, authorizer/, field_authorizer.rs, field_authorizer/, field_filter.rs, field_masking.rs, rls_policy.rs, auth_middleware/, security_context.rs, errors.rs, error_formatter.rs, profiles.rs, headers.rs, tls_enforcer.rs). Also crates/fraiseql-core/src/validation/ and crates/fraiseql-core/src/security/actor_type.rs.
HUNT (this is a prime fail-open area): a depth/complexity limit that is computed but never enforced, or off-by-one so the limit is one deeper than documented; introspection enforcer that allows __schema through a fragment or alias; an authorizer/field_authorizer that returns Allow on an unknown role, missing policy, or parse error; field_masking that masks in one code path but not another; rls_policy that emits no WHERE clause when the policy string is empty/unparseable (fail-open to full-table). Verify every "enforced"/"denied by default" doc claim.`,
  },
  {
    key: 'federation-saga',
    prompt: `AREA: fraiseql-federation saga orchestration + entity resolution.
FILES: crates/fraiseql-federation/src/saga_coordinator/, saga_store/, saga_recovery_manager/, saga_compensator/, saga_executor/ (read each mod.rs and submodules IN FULL). Also the _entities / entity-representation resolution path and key extraction (NOT composition_validator or value_to_sql_literal — those are #728).
HUNT: compensation that runs in the wrong order or skips a step on partial failure; saga step marked complete before its side effect is durably persisted (let _ = store...); idempotency token generated but not checked, or checked with a colliding key; recovery manager that re-runs an already-committed step; a saga that reports success while a compensation silently failed; _entities resolver that trusts client-supplied representation fields without validating the @key; missing tenant scoping in the saga store.`,
  },
  {
    key: 'server-rbac-admin-studio',
    prompt: `AREA: fraiseql-server RBAC management API, admin routes, and /studio.
FILES: crates/fraiseql-server/src/api/rbac_management.rs + rbac_management/, crates/fraiseql-server/src/routes/ (enumerate and read admin/studio/management route handlers), crates/fraiseql-server/src/service_account.rs + service_account/, crates/fraiseql-server/src/token_revocation.rs + token_revocation/, crates/fraiseql-server/src/validation.rs.
HUNT: an admin/RBAC mutation route that is missing an auth/role guard that its siblings have (compare handlers side by side); /studio data-editor or query-runner route that bypasses tenant scoping or RBAC; role-grant endpoint that doesn't validate the role exists; token revocation that returns Ok without actually revoking (let _ =); management route that trusts a client-supplied tenant_id; missing rate-limit/authz on a destructive admin action. Read the axum Router builders to confirm which middleware layers actually wrap which routes.`,
  },
  {
    key: 'server-subscriptions-ws',
    prompt: `AREA: fraiseql-server /ws GraphQL subscriptions protocol.
FILES: crates/fraiseql-server/src/subscriptions/ (protocol.rs, lifecycle.rs, event_bridge.rs, webhook_lifecycle.rs, mod.rs). Read all IN FULL.
HUNT (row-visibility on hot-reload is already #611 — find OTHER bugs): graphql-transport-ws protocol violations (missing ack/pong, wrong close codes, connection_init timeout not enforced); a subscription that leaks events across tenants because the filter is applied at subscribe-time only and not re-checked per event; auth token captured at connect but never re-validated as it expires mid-stream; event_bridge that drops or duplicates events under backpressure; unbounded per-connection buffering (DoS); a Complete/error frame path that leaves server-side state registered (leak). Verify any "isolated per-connection" or "authorized" claim.`,
  },
  {
    key: 'server-hotreload-env',
    prompt: `AREA: fraiseql-server SIGUSR1 hot-reload + env-var override precedence vs compiled config.
FILES: crates/fraiseql-server/src/server/, crates/fraiseql-server/src/server_config/, crates/fraiseql-server/src/config/, crates/fraiseql-server/src/main.rs, crates/fraiseql-server/src/lib.rs, and any reload/signal handler (grep SIGUSR1 / reload). Also crates/fraiseql-server/src/subsystems/.
HUNT: an env-var override that is read at boot but ignored on hot-reload (or vice-versa) so a reload silently reverts a production override; precedence documented as env>compiled but implemented compiled>env for some keys; a security setting (rate limit, auth, metrics token) that can be toggled OFF by a reload without re-validation; a reload that swaps the schema but keeps a stale cache/authorizer/rls policy pointer; partial reload that leaves subsystems inconsistent if one fails mid-swap (fail-open). Verify env-override precedence key-by-key against the docs/CLAUDE.md claim (env overrides compiled).`,
  },
  {
    key: 'server-webhooks-inbound',
    prompt: `AREA: fraiseql-server webhook + inbound + sources routes.
FILES: crates/fraiseql-server/src/inbound/, crates/fraiseql-server/src/sources/, crates/fraiseql-server/src/routes/ (webhook handlers), and crates/fraiseql-webhooks + crates/fraiseql-cdc-sinks only where the server wires them. Read route handlers IN FULL.
HUNT: inbound webhook signature verification that is skipped when the secret is unset (fail-open to unauthenticated writes), or verified with == instead of constant-time, or verified against the wrong body (post-parse vs raw bytes); replay protection missing or keyed on a spoofable field; source idempotency token check that can be bypassed; a webhook route mounted without the auth layer its siblings have; JSON body size unbounded before signature check (DoS + amplification).`,
  },
  {
    key: 'auth-local-password',
    prompt: `AREA: fraiseql-auth local password flows.
FILES: crates/fraiseql-auth/src/local_password.rs, crates/fraiseql-auth/src/local_password/ (reset.rs, reset/, tests.rs), crates/fraiseql-auth/src/rate_limiting.rs, crates/fraiseql-auth/src/otp/, crates/fraiseql-auth/src/phone_otp.rs, crates/fraiseql-auth/src/totp_mfa/ (if present), crates/fraiseql-auth/src/session.rs.
HUNT: password verify that isn't constant-time / user-enumeration via differing register-vs-login error or timing; lockout counter that resets on a wrong path or never triggers; password reset token that is predictable, not single-use, not expired, or logged; reset token compared with == ; OTP/TOTP window too wide or replay-accepted; a successful-login side effect (reset lockout, rotate session) done with let _ = and not verified; register that trusts client-supplied role/tenant. Verify any "hashed with argon2/bcrypt" and "single-use" claim in code.`,
  },
  {
    key: 'auth-oidc-oauth-saml',
    prompt: `AREA: fraiseql-auth OIDC/OAuth token exchange + SAML.
FILES: crates/fraiseql-auth/src/oauth/ (client.rs, provider.rs, refresh.rs, claims_validator.rs, pkce.rs, failover.rs, audit.rs, types.rs), crates/fraiseql-auth/src/oidc_provider.rs, crates/fraiseql-auth/src/oidc_server_client.rs, crates/fraiseql-auth/src/jwt/, crates/fraiseql-auth/src/jwks.rs, crates/fraiseql-auth/src/saml/ (handler.rs, verify.rs, replay.rs, linking.rs, config.rs), crates/fraiseql-auth/src/multi_provider.rs, crates/fraiseql-auth/src/proxy.rs.
HUNT (classic auth fail-open zone): JWT/JWKS validation that accepts alg=none or an unexpected alg; issuer/audience/nonce/exp check skipped when the claim is absent (the recent Hanko issuer-optional work #708/#709 — verify iss-optional didn't drop iss validation when iss IS present); state/nonce compared with == not constant-time; SAML signature verify that validates the assertion but consumes an unsigned one, or XML-canonicalization/comment-injection; SAML replay cache that never expires or is bypassable; refresh-token rotation that doesn't invalidate the old token; claims_validator returning Ok on a missing required claim.`,
  },
  {
    key: 'auth-store-parity',
    prompt: `AREA: fraiseql-auth Redis-vs-memory store parity (state, PKCE, sessions) + JWKS cache.
FILES: crates/fraiseql-auth/src/state_store.rs, crates/fraiseql-auth/src/state_encryption.rs, crates/fraiseql-auth/src/pkce.rs, crates/fraiseql-auth/src/session.rs, crates/fraiseql-auth/src/session_postgres.rs, crates/fraiseql-auth/src/jwks.rs (cache), and any redis-vs-memory backend pair (grep for "Redis"/"Memory" store impls, cfg(feature) arms).
HUNT (cfg-drift + parity is the target): the memory backend and the redis backend disagree on a security-relevant behavior — e.g. one expires state/PKCE/session and the other doesn't; one enforces single-use (delete-on-read) and the other leaves it readable (replay); one encrypts at rest and the other stores plaintext; TTL applied in one arm only; a feature-gated store that silently no-ops when the feature is off (fail-open to no state validation). Read BOTH arms of each pair side-by-side and diff the semantics.`,
  },
  {
    key: 'featureflag-drift',
    prompt: `AREA: cross-crate cfg(feature) drift — arms that diverged.
METHOD: grep the workspace for #[cfg(feature = and #[cfg(not(feature = , focusing on functions that have BOTH arms (real impl vs stub/no-op). For each pair, read both arms and check the stub arm doesn't silently disable a security or correctness guard while the type signature stays identical (caller can't tell). Priority crates: fraiseql-server (arrow, observers, sources, saga, functions, storage flags), fraiseql-core (cache, apq, security flags), fraiseql-auth (redis, saml, social flags), fraiseql-secrets (vault backends).
HUNT: a feature-off stub that returns Ok/true/allow where the feature-on path enforces something; a metric/audit/log that only fires under one feature so disabling it loses the audit trail; a default-feature set that turns a security feature OFF by default; a function documented as always-present but actually cfg-gated so it vanishes in a common --features combo. Report each divergence with both file:line locations.`,
  },
  {
    key: 'seam-schema-contract',
    prompt: `AREA: cross-layer contract seam — compiled schema producer (cli) vs consumer (core), and SDK schema.json vs cli compile schema.
FILES: crates/fraiseql-cli/src/schema/converter/, crates/fraiseql-cli/src/schema/intermediate/, crates/fraiseql-cli/src/schema/merger.rs, crates/fraiseql-cli/src/schema/mutation_contract/, crates/fraiseql-cli/src/schema/validator/, crates/fraiseql-cli/src/output_schemas.rs; and the consumer side crates/fraiseql-core/src/schema/compiled/ (+ compiled.rs), crates/fraiseql-core/src/schema/mod.rs, config_types.rs, security_config.rs, source_types/. Also skim sdks/official/fraiseql-python and sdks/official/fraiseql-typescript schema emitters vs the cli input schema.
HUNT (history: SpecQL output was REJECTED because input_types lacked fields): a field the cli emits with one name/shape and core deserializes with a different serde name/default so it's silently dropped to a default (serde(default) hiding a producer/consumer mismatch); an enum variant the producer can emit that the consumer's match handles as a catch-all fail-open; a required-on-consumer field the producer sometimes omits; camelCase vs snake_case serde rename drift between the two sides; a compiled-schema field the SDK generates that the cli rejects or ignores. Diff the structs field-by-field. Report each mismatch with both producer and consumer file:line.`,
  },
]

phase('Review')
log(`Pass-2 audit: ${AREAS.length} area reviewers -> adversarial per-finding verifiers`)

const results = await pipeline(
  AREAS,
  (a) => agent(`${PREAMBLE}\n\n=== ${a.key} ===\n${a.prompt}`, {
    label: `review:${a.key}`,
    phase: 'Review',
    schema: FINDINGS_SCHEMA,
  }),
  (review, area) => {
    if (!review || !review.findings || review.findings.length === 0) return { area: area.key, review, verified: [] }
    return parallel(review.findings.map((f) => () =>
      agent(`You are an ADVERSARIAL verifier for a FraiseQL pass-2 audit finding. Your default is to REFUTE it. Read the cited file(s) IN FULL plus the relevant call sites, and only CONFIRM if a concrete, reachable failure survives your best refutation.

${DEDUP}

If the finding restates any already-filed issue above, verdict=REJECTED with duplicate_of set.
If the bad path is dead code, test-only, or gated behind an impossible cfg, verdict=REJECTED and reachable=false (note if it's still a worthwhile low-severity cleanup in reasoning).
If it's real and reachable but you cannot be certain without executing code, verdict=PLAUSIBLE and put the exact repro (test/command) in runtime_repro.
If it's a solid, reachable, non-duplicate defect provable by reading, verdict=CONFIRMED.
Correct the severity to what the true blast radius warrants (a cosmetic-only worst case is 'low').

CANDIDATE FINDING (JSON):
${JSON.stringify(f, null, 2)}

Return raw structured data.`, {
        label: `verify:${area.key}:${f.file.split('/').pop()}:${f.line}`,
        phase: 'Verify',
        schema: VERDICT_SCHEMA,
      }).then((v) => ({ ...f, area: area.key, verdict: v })).catch(() => null)
    )).then((verified) => ({ area: area.key, review, verified: verified.filter(Boolean) }))
  }
)

const all = results.filter(Boolean)
const coverage = all.map((r) => ({ area: r.area, files_read: r.review?.files_read?.length || 0, raw: r.review?.findings?.length || 0 }))
const confirmed = all.flatMap((r) => r.verified).filter((f) => f.verdict && f.verdict.verdict === 'CONFIRMED')
const plausible = all.flatMap((r) => r.verified).filter((f) => f.verdict && f.verdict.verdict === 'PLAUSIBLE')
const rejected = all.flatMap((r) => r.verified).filter((f) => f.verdict && f.verdict.verdict === 'REJECTED')

return {
  coverage,
  counts: { confirmed: confirmed.length, plausible: plausible.length, rejected: rejected.length },
  confirmed,
  plausible,
  rejected: rejected.map((f) => ({ title: f.title, file: f.file, line: f.line, verdict: f.verdict.verdict, duplicate_of: f.verdict.duplicate_of, reason: f.verdict.reasoning?.slice(0, 200) })),
}
