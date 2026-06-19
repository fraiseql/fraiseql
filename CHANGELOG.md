# Changelog

All notable changes to FraiseQL are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- **Demo / example / test deployment artifacts hardened (#436).** Follow-up to the
  Phase 13 production deploy sweep, covering the demo/example/test residue the
  production gate (`tools/check-deploy-security.sh`) does not guard. Docker port
  publishing bypasses host firewalls, so every backing-service port in the demo and
  test compose stacks (`docker/docker-compose.{prod,prod-examples,demo,examples,test}.yml`,
  `docker/tls-postgres/`, `examples/ecommerce_api/`, `examples/async-jobs-subgraph/`,
  `examples/observability/`) is now bound to `127.0.0.1` — still locally usable for
  demos and CI, no longer reachable from the network. Weak literal passwords became
  overridable env vars with a documented demo default
  (`${POSTGRES_PASSWORD:-…}`) so the stacks still start out of the box while honouring
  an override. The two misnamed `docker/docker-compose.prod*.yml` files now carry a
  header clarifying they are local demo stacks, not production templates. Dockerfile
  hardening: `tutorial/Dockerfile` and the two `examples/async-jobs-subgraph/*`
  images now drop to a non-root `USER`; the federation example images pin
  `FROM rust:latest` → `rust:1.92`; the `fraiseql-wire` CI test fixture gained a
  comment marking its deliberately-open settings as test-only. Loki/Grafana/nginx
  monitoring artifacts gained comments documenting their no-auth / TLS-terminated-
  upstream assumptions. No production artifact changed; the deploy-security gate
  still passes.
- **Example apps no longer ship insecure patterns users copy to prod (#438).**
  Sibling of #436, covering the example *application* code (not the deploy
  artifacts). Four findings fixed: (1) `examples/multitenant/fraiseql.toml` had no
  `[security]` block, so `listTenants`/`listResources` returned every tenant's rows
  to any anonymous caller — added `default_policy = "authenticated"` plus per-tenant
  row-scoping rules (mirroring `examples/saas/`) and a "Tenant isolation" note in the
  README. (2) `examples/ecommerce_api/.../customer_functions.sql` `register_customer()`
  stored the plaintext password into `password_hash`; it now bcrypt-hashes via
  `crypt(p_password, gen_salt('bf'))`, and the init migration creates the `pgcrypto`
  extension. (3) `examples/async-jobs-subgraph/router/router.yaml` dropped
  `allow_any_origin: true`, which silently overrode the `origins:` allow-list above
  it. (4) `examples/analytics_dashboard/` and `examples/cascade-create-post/` were
  re-pinned off the legacy v1 `fraiseql[fastapi]==1.8.1` to the v2 line
  (`fraiseql==2.8.0`, no v2 `fastapi` extra) and gained a `requirements.lock`.
- **Row-Level Security on the change-spine change-log — BREAKING (#437 F6 / #443).**
  `core.tb_entity_change_log` holds the full before/after payload for every tenant,
  and until now any database role with `SELECT` on the table or its views
  (`core.v_entity_change_log`, `core.v_entity_change_log_debezium`) could read all
  of them — the contract called `tenant_id` an "RLS partition stamp" but RLS was
  never enabled. Migration `12_enable_change_log_rls.sql` turns it on: the table is
  now **deny-by-default** (a role that is neither owner nor `BYPASSRLS`, and has not
  set the `fraiseql.tenant_id` GUC, reads zero rows), with a forward-looking
  per-tenant SELECT policy and a permissive INSERT policy (the executor outbox + the
  now-`SECURITY DEFINER` capture function stamp the tenant). The two views are
  flipped to `security_invoker = true` (PostgreSQL 15+) so they enforce the
  base-table RLS instead of bypassing it as the view owner; on PostgreSQL < 15 the
  migration warns and the views must be access-restricted to trusted roles. The
  capture function `core.fn_entity_change_log_capture()` is now `SECURITY DEFINER`
  with a pinned `search_path = pg_catalog, core`, so external-write capture keeps
  working under RLS. The migration also `REVOKE ALL … FROM PUBLIC` on the table and
  both views (least-privilege baseline — the change-log is never world-readable, so
  RLS is genuine defence-in-depth rather than the sole control). A new
  `fraiseql doctor --against-db` check warns when RLS is enabled on the change-log
  but the connecting role is neither the table owner nor `BYPASSRLS` — catching the
  silent-empty-pipeline footgun before it bites in production. **Operator action
  (BREAKING):** the change-log consumers (poller, the 3 NATS bridges, the server
  changelog HTTP handlers, the mutation executor outbox) all run on the server's
  database role — that role must be the table owner or carry `BYPASSRLS`, otherwise
  the CDC pipeline and the admin change-log query silently return empty. FraiseQL
  does not set `fraiseql.tenant_id` on its read paths today, so the practical effect
  is deny-by-default; per-tenant GUC filtering is forward-looking. MySQL / SQL Server
  change-log isolation is a tracked follow-up.
- **Per-tenant GraphQL operation cost budgets (#379).** `max_query_depth` and the
  complexity limit stop naive recursion, but not an expensive within-depth query. A new
  per-tenant `cost_budget` (on `TenantQuota`, settable via the tenant admin API) rejects a
  request whose estimated cost exceeds the tenant's budget at the same chokepoint as the
  rate/concurrency quotas (HTTP 429). The cost reuses the existing complexity score
  (`estimate_query_cost`); a root operation listed in `[fraiseql.cost_weights]` counts as
  its manual `@cost` weight instead of its walked subtree, letting operators pin the cost
  of a known-expensive query. Off by default (no budget configured ⇒ unlimited).
- **`[security] persisted_queries_only` shorthand (#379).** A single top-level flag that
  forces the trusted-document store into `strict` mode — reject any operation that is not a
  persisted/trusted document — regardless of the declared `[security.trusted_documents].mode`.
  Equivalent to setting `mode = "strict"`, but expressed as one operator-facing toggle. It
  only takes effect when a trusted-documents manifest is configured (there must be persisted
  operations to allow-list); the server logs a warning if the flag is set without an enabled
  manifest so it never fails silently. Off by default.
- **The production Docker Compose no longer exposes backing services to the network
  (H46).** `docker-compose.prod.yml` published PostgreSQL (`5432`), Redis (`6379`), and
  Prometheus (`9090`) on `0.0.0.0` — and because Docker's port publishing inserts its own
  `iptables` rules ahead of the host firewall, those services were reachable from the
  internet regardless of any `ufw`/firewall policy. Redis additionally ran with no
  password (`protected-mode` is off once a port is published), so anyone who could reach
  it had full command access. The backing-service ports are now bound to `127.0.0.1`
  (containers still reach each other by service name over the bridge network, and host
  loopback access remains for local admin/migration tooling), Redis now requires
  `--requirepass ${REDIS_PASSWORD}`, and `${DB_PASSWORD}`/`${REDIS_PASSWORD}` use a
  fail-loud `:?` guard so an unset secret aborts startup instead of silently creating a
  passwordless database. The root dev `docker-compose.yml` backing services were likewise
  rebound to loopback. A static gate (`tools/check-deploy-security.sh`, wired into
  ShellGates/CI) prevents regressions. **Operator action:** if you relied on reaching
  Postgres/Redis/Prometheus from another host via the published port, front them with an
  SSH tunnel or reverse proxy, and set `DB_PASSWORD` and `REDIS_PASSWORD` in your
  environment.
- **The NATS observer transport refuses plaintext `nats://` by default
  (L-nats-plaintext).** Change-log events bridged to NATS previously crossed the wire in
  the clear over `nats://` with no TLS enforcement. `validate_nats_url` now requires
  `tls://`; plaintext `nats://` is accepted only when `FRAISEQL_NATS_ALLOW_PLAINTEXT` is
  set to `1`/`true` **and** no production marker is present (`KUBERNETES_SERVICE_HOST`,
  `FRAISEQL_ENV=production`, `FRAISEQL_PROFILE=production`) — the same refused-in-production
  policy as the SSRF bypass, but a separate flag so allowing plaintext NATS does not also
  disable the outbound SSRF guards. **Behavior change:** a deployment configured with a
  `nats://` URL must switch to `tls://` (or set the opt-in outside production).
- **Presigned storage URLs are now clamped to a maximum validity (L-presigned-expiry).**
  The `GET /storage/v1/object/sign/*key` endpoint accepted an unbounded `expiry_secs`, so a
  client could mint a credential-free URL valid for years. The requested expiry is now
  clamped to a configurable ceiling (default 7 days, `StorageRouteState::with_max_presign_expiry_secs`).
- **Hardened the shipped Kubernetes and Helm deployment manifests.** `k8s/service.yaml`
  carried a `Secret` with the literal database password `password`; it is now a
  non-functional placeholder with guidance to inject the real value out-of-band. The raw
  manifests, the Helm chart, and the "hardened" manifest pinned images to `:latest` (now
  `2.8.0`) and ran with a writable root filesystem (`readOnlyRootFilesystem: false`, now
  `true` — including the Helm values and the PodSecurityPolicy). Lower-severity
  demo/example/test deployment artifacts are tracked in #436.
- **Allow-list-backed `redirect_uri` flow for multi-provider auth (#427).**
  `GET /auth/v1/authorize` previously accepted a `redirect_uri` and then discarded it
  (returning tokens as JSON), with a server-side redirect deliberately deferred as an
  open-redirect risk. `MultiProviderAuthState::with_redirect_uri_allowlist` now enables a
  safe redirect: `authorize` rejects any `redirect_uri` not on the allow-list (400) and
  binds the validated URI to the CSRF state; `callback` redirects the browser to it with
  the tokens in the URL fragment. Matching is exact scheme + host + port + path-boundary
  prefix, so `https://app.example.com` does not match `https://app.example.com.evil.com`.
  With no allow-list configured the legacy JSON response is preserved and the URI is never
  used as a redirect target (no open-redirect surface). Closes the deferred audit finding
  L-redirect-uri.
- **SQL-layer hardening for shipped/template SQL (#437).** The two prod-init
  `SECURITY DEFINER` functions now pin `SET search_path = pg_catalog`
  (search-path-hijack hardening); the application role's grants are narrowed from
  `ALL PRIVILEGES` to `SELECT, INSERT, UPDATE, DELETE` (+ `USAGE, SELECT` on sequences);
  the `fraiseql` helper schema grants EXECUTE per function instead of the snapshot
  `ON ALL FUNCTIONS … TO PUBLIC`; and the init-script role passwords are documented as
  insecure placeholder defaults that must be overridden. (Change-spine view RLS — F6 — is
  deferred to a dedicated effort.)

### Removed

- **Unwired "enterprise" field-encryption modules removed from `fraiseql-secrets`
  (BREAKING).** The `encryption::{compliance, dashboard, error_recovery, mapper,
  performance, query_builder, refresh_trigger, rotation_api, schema, transaction}`
  modules (~7,300 LOC) had zero production consumers — reachable only from their own
  tests, never from the server binary. They are no longer part of the public API. The
  encryption primitives (`FieldEncryption`, `VersionedFieldEncryption`) and the three
  wired modules (`middleware`, `database_adapter`, `credential_rotation`) are retained.
  This matches the field-encryption stance documented in v2.7.0 (the write path is
  inert; the server refuses to boot on encryption-marked fields).
- **`fraiseql_core::validation::CustomScalarRegistry` removed (BREAKING).** It was a
  public API wired to nothing. The `CustomScalar` trait it managed is unaffected.
- **Dead CLI command handlers removed.** The unreachable `generate-proto`/`openapi`
  handlers, the `gateway` command module (no `Commands` variant), and the orphaned
  `codegen` tree they depended on are gone (~3,600 LOC), along with the now-unused
  `prost`/`prost-types`/`thiserror` dependencies. No wired subcommand changes.
- **`fraiseql-db` empty `grpc` feature removed** (it gated nothing; real gRPC lives
  behind `fraiseql-server`'s `grpc` feature), and the `fraiseql-core` passthrough with it.
- **`ArrowFlightError::Flight` variant removed** — defined but never constructed. The
  enum is `#[non_exhaustive]`, so downstream matches (which already need a wildcard arm)
  are unaffected.

### Changed

- **The `lint --verbose` CLI flag was removed** — it was parsed and discarded. The
  global `--verbose` flag is unaffected.
- `fraiseql-arrow` type-conversion errors now surface as `ArrowFlightError::Conversion`
  instead of the mislabeled `InvalidTicket`. `ClickHouseSink::run` now terminates (with a
  final flush) when its channel closes instead of spinning forever, and the health-check
  `version` reports `CARGO_PKG_VERSION` instead of a hardcoded `2.0.0-a1`.
- `fraiseql federation check --json` no longer double-prints its result under the global
  `--json` flag; `fraiseql run` now warns loudly when a sibling `fraiseql.toml` fails to
  parse instead of silently falling back to defaults.

### Added

- **Persistent user / identity store (#411).** New `PostgresAccountStore` — a durable
  PostgreSQL backend for the existing `AccountStore` trait, so account-linking survives a
  process restart (the in-memory store loses it). It is a drop-in: same trait, same
  `"user_<uuid>"` identifier that joins `_system.sessions.user_id`, so `multi_provider` /
  `phone_otp` need no change beyond which `Arc<dyn AccountStore>` they are handed.
  `init()` idempotently creates `core.tb_user` and `core.tb_auth_identity` (the
  `CREATE … IF NOT EXISTS` form is the back-compat path for deployments with no user
  table). Both tables carry a `tenant_id` and Row-Level Security **deny-by-default**
  (mirroring the change-log RLS: `ENABLE`-not-`FORCE`, GUC `fraiseql.tenant_id`,
  `REVOKE ALL … FROM PUBLIC`) — the store is the trusted owner that bypasses, while any
  other role reads zero rows unless scoped to a tenant. Account-linking semantics
  (verified-email cross-provider linking; H26 fail-closed on absent/unverified email)
  match the in-memory store exactly, verified against PostgreSQL. This unblocks the
  Argon2id local-password authenticator (#412), social auto-linking (#368), and SCIM
  provisioning (#381). See `docs/auth/identity-store.md`.
- **Compiler→runtime contract gate.** A new test (`fraiseql-cli`) compiles fixtures with
  the real CLI and asserts the server boot seam (`RuntimeConfig::from_compiled_schema`)
  accepts the output, that an enterprise security toggle survives emit→parse→derive, and
  that core parse drops no compiler-emitted field — closing the class behind two past
  config-drift boot failures.
- **Signature-verification tests** for the Postmark and LemonSqueezy webhook verifiers
  (previously the only two of 13 with zero coverage), and a loud-failure assertion for
  the `fraiseql-test-support` database-URL harness.
- **`release-smoke`** now runs one real GraphQL query through the full pipeline, not just
  the health endpoint.
- **`fraiseql watch` — recompile + zero-downtime live reload (#383).** A new CLI command
  watches a schema source and, on every (debounced) save, recompiles `schema.compiled.json`
  and — when `--reload-url` is given — POSTs it to a running server's
  `POST /api/v1/admin/reload-schema` admin endpoint, which swaps the executor via `ArcSwap`
  (in-flight queries finish on the old schema, no restart). Unlike `run --watch` (which
  restarts an in-process server), `watch` drives a separately running server: `fraiseql
  watch schema.json --reload-url http://localhost:8080 --admin-token $TOKEN`. Compile and
  reload failures are reported but never stop the loop. Omit `--reload-url` to recompile to
  disk only.
- **`fraiseql compile --database` now lints more of the view contract (#384).** Three
  residual checks were added to the compile-time database validator: (1) each mutation's
  `inject_params` and call/response shape are validated against the real `pg_proc`
  signature (PostgreSQL), reusing the `validate --against-db` contract logic; (2) a
  query argument that resolves to a native column whose SQL type cannot drive the
  predicate (e.g. an `Int` argument filtering a `uuid` column) is flagged — conservative,
  so permissive `ID`/`String` filters never warn; and (3) the `MissingJsonKey` warning
  now names the owning GraphQL type so the field is locatable. All findings remain
  advisory warnings (the compile never fails on them).
- **`fraiseql doctor --against-db` gained two change-log hardening checks (#443).**
  Alongside the existing change-log RLS posture check, the live-database pass now
  verifies the rest of the migration-12 / migration-11 hardening: (1) **Change-log
  PUBLIC grants** warns when `PUBLIC` still holds any privilege on
  `core.tb_entity_change_log` or its two views (the `REVOKE ALL … FROM PUBLIC`
  least-privilege baseline is not in force — every tenant's before/after payload is
  world-readable); and (2) **Change-log capture function** warns when
  `core.fn_entity_change_log_capture()` is not `SECURITY DEFINER`, or is DEFINER but
  has no pinned `search_path` (a DEFINER function reachable from a trigger on any
  schema with a mutable `search_path` is a privilege-escalation vector). Both are
  advisory warnings; an absent table or function is an informational pass
  (single-tenant or pre-migration deployments).
- **GraphQL subscription clients now receive the Change-Spine envelope (#425).** Each
  delivered `next` event carries the audit / provenance metadata the Change-Spine
  already records — `actorType` (human / service account / AI agent / system job),
  `actingFor` (the human a delegated agent acted for, #390), `schemaVersion` (the
  producer schema, #377), `tenantId`, `durationMs`, and `seq` — bringing the
  subscription path to the same envelope parity the change-log reader and NATS bridges
  have. The envelope rides in the graphql-transport-ws `extensions.changeSpine` slot of
  the `next` payload (the spec-blessed, client-ignorable channel), so the resolved
  entity `data` is untouched and no schema or SDK-codegen change is required. It is
  always present, carrying only the fields the producer stamped (unset fields are
  omitted); an event with no stamped envelope delivers the plain payload unchanged. The
  metadata round-trips observer `EntityEvent` → `BridgeEntityEvent` → `SubscriptionEvent`
  → client, with tenant filtering and resolved-`data` delivery unaffected.

### Fixed

- **The FraiseQL-Wire database backend now honors `ORDER BY` (#442).** `FraiseWireAdapter`
  silently dropped the `order_by` argument, so relay/keyset pagination over the
  `wire-backend` feature returned database-native order. Both the streaming and the
  in-memory limit/offset paths now push the (validated, dialect-aware) ordering down to the
  wire query builder. Also removes a dead `build_query` method and an unreachable SQL Server
  pagination branch, and de-duplicates the sqlite/sqlserver adapter test files.
- **The `ecommerce_api` example's database now initializes cleanly (#446).** The flat
  `docker-compose` migration tree (`db/migrations` + `db/views` + `db/functions`) had
  several latent SQL errors that left the schema half-built — `migrate` printed
  "Migrations completed!" regardless because `psql` ran without `ON_ERROR_STOP`. Fixed:
  the shared `mutation_response` composite type (returned by every function but never
  defined in the flat tree) is now created in `001_initial_schema.sql`; `add_customer_address()`
  and `submit_review()` reorder their parameters so no required parameter follows a defaulted
  one (PostgreSQL rejects that); the `product_detail` / `customer_wishlists` views compute
  per-product images, variants, price and stock via correlated subqueries instead of
  `json_agg(DISTINCT …)` over multiplying joins (invalid `ORDER BY`/`json`-equality/nested-aggregate
  forms); and `related_products` counts shared tags with an `INTERSECT` instead of the
  non-existent `&` array operator. The `migrate` step now runs each file with
  `ON_ERROR_STOP=1` so a broken migration fails loudly. Verified end-to-end against
  PostgreSQL (full init + mutation calls + view reads). Found while hardening the examples
  for #438.

## [2.8.0] - 2026-06-18

### Security

- **The `sql_query` host read-only guard now inspects CTE bodies (M-cte-classifier).** The
  SQL classifier mapped any `Statement::Query` to read-only without walking its `WITH`
  clause, so a data-modifying CTE — `WITH t AS (DELETE FROM x RETURNING *) SELECT * FROM t`
  (and the `INSERT`/`UPDATE`/`MERGE` equivalents, including nested and derived-subquery
  CTEs) — passed as read-only, bypassing the guard. The classifier now recurses through CTE
  and subquery bodies and rejects data-modifying statements with the (previously dead)
  `RejectionReason::WritableCte`.
- **Deno function resource limits are now enforced by V8, not by string matching
  (M-deno-limits, DoS).** The "limits" were `source.contains("while (true)")` substring
  checks that the configured memory cap never reached V8 — trivially bypassed and prone to
  false positives. They are replaced with real enforcement: a V8 heap limit
  (`CreateParams::heap_limits` + a near-heap-limit callback that terminates execution) and a
  watchdog thread that calls `terminate_execution()` after the configured duration (catching
  tight synchronous loops that never yield to the event loop). The substring heuristics are
  deleted.
- **SSRF guards converged and hardened across the functions, federation, and observers
  crates (M-fn-ssrf, M-fed-mut-ssrf, M-fed-allow-insecure, M-ssrf-blocklist).** Each
  outbound-HTTP path now resolves DNS and blocks private/reserved addresses (closing the
  DNS-rebinding TOCTOU), disables redirects (`Policy::none()` — a `3xx` can no longer bounce
  to an internal target), and fails closed:
  - **functions** `http_validator`: the default domain allowlist was `["*"]` (allow-all) —
    now empty (deny-by-default); the guard now resolves the host and rejects private IPs
    instead of only checking literal-IP hosts; the outbound client disables redirects.
  - **federation** `HttpMutationClient` (the state-changing direction) gained the
    `redirect(Policy::none())` + `https_only(true)` + DNS-rebinding guards its sibling entity
    resolver already applied.
  - **federation** `FRAISEQL_FEDERATION_ALLOW_INSECURE` is **removed**: it logged "HTTPS
    enforcement disabled" while `https_only(true)` was unconditional — a lying no-op with no
    recorded user. `http://` subgraph URLs are now rejected unconditionally.
  - **observers**: the drifted SSRF blocklist duplicated in `executor/dispatch.rs` is deleted
    in favour of the canonical `ssrf::validate_outbound_url` (with `0.0.0.0/8` and
    `localhost.*`-alias coverage merged into the canonical first so nothing is lost) plus a
    dispatch-time `dns_resolve_and_check`.
- **The `sql_query` host read-only guard now inspects CTE bodies (M-cte-classifier).** The
  `provider::PkceChallenge::validate` compared the recomputed challenge with `==`
  (variable-time), a timing-attack vector, while the parallel `oauth::pkce::PkceChallenge`
  used constant-time `ct_eq`. The `provider` path now uses `subtle::ConstantTimeEq`, so all
  PKCE verification paths are constant-time.
- **JWKS fetch pins the connection to the validated IP (M-jwks-toctou, DNS-rebinding SSRF).**
  `dns_resolve_and_check` validated the resolved IPs, but the subsequent reqwest call
  re-resolved the host independently — a TOCTOU window where attacker-controlled DNS could
  flip the host to a private IP after the check (blind internal SSRF). The fetch now resolves
  and validates once, then pins reqwest to the validated addresses (`resolve_to_addrs`) and
  disables redirects (`Policy::none()`) so the connection cannot be re-pointed to an internal
  target. **Behavior change:** a `jwks_uri` that issues an HTTP redirect is no longer followed
  (OIDC `jwks_uri` endpoints are served directly; following redirects on this fetch is an SSRF
  amplifier).
- **Vault `AppRole` login validates the address before sending credentials (H15, SSRF).**
  `with_approle` POSTed the `role_id`/`secret_id` to the configured address and only
  afterwards ran the SSRF address check, so a misconfigured/attacker-influenced address
  (e.g. `169.254.169.254`) received the high-value `secret_id` before the guard fired.
  `validate_vault_addr` now runs as the first statement, matching the token path.

### Added

- **`naming_convention` is now configurable for the JSON-schema compile workflow
  (`[fraiseql.naming] convention`).** Previously only the author-in-TOML workflow could set a
  naming convention; the `fraiseql-cli compile schema.json` + `fraiseql.toml` workflow was
  hardwired to `preserve`, so a backend on that path could never activate the server's
  single-JSONB mutation input-key recasing (gated on `camelCase`) and had to hand-roll a
  `camelCase → snake_case` input shim. `[fraiseql.naming] convention = "preserve" | "camelCase"`
  now flows through the compiler into the compiled schema's `naming_convention`, the same value
  the TOML workflow already populated. With `camelCase`, the engine owns all casing end-to-end
  (`snake_case` columns/functions in the database, `camelCase` operation/field names to clients,
  input keys recased before they reach the SQL functions), letting such backends delete the shim.
- **Configurable casing acronyms (`[fraiseql.naming] acronyms`).** Identifiers shaped as a
  lowercase word plus a digit (`s3`, `ipv4`, `oauth2`) are ambiguous to reverse — `phone1`
  (from `phone_1`) and `s3` are structurally identical — so they now keep their digit attached
  via an acronym registry. A built-in default set covers the common cases (`s3`, `ec2`, `ipv4`,
  `ipv6`, `oauth2`, `sha256`, `md5`, `base64`, …); add your own `<word><digit>` keys with, e.g.,
  `[fraiseql.naming]\nacronyms = ["widget3", "iso9001"]`. Registering an acronym declares its
  JSONB key is the atomic form (`s3`, not `s_3`) — author the field accordingly. The list flows
  from `fraiseql.toml` through the compiler into the compiled schema and is installed at server
  boot; only the reverse (`to_snake_case`) consults it, so the GraphQL surface is unchanged.
- **Opt-in auto-synthesis of mutation result unions (`[fraiseql.mutations] auto_error_union`).**
  When enabled, the compiler synthesizes a shared `MutationError` type and a per-mutation
  `<Mutation>Result` union (`= Entity | MutationError`) for every object-returning mutation,
  rewriting the mutation's return type to that union — so the server's existing success/error
  discrimination over the `app.mutation_response` composite has a union to resolve against
  without declaring `Entity | MutationError` by hand for each mutation. Off by default;
  mutations already returning a union (and scalar/enum returns) are left untouched, and an
  existing type name is never overwritten. The synthesized `MutationError` exposes `status`,
  `message`, `httpStatus`, and `errorClass`, now surfaced from the composite's first-class
  columns on the error arm (previously only `status` was injected). See the "result unions"
  section of `docs/guides/typed-clients.md` for the authoring contract.
- **Changelog tail query for tip checkpointing (H28, server side).** The
  `GET /api/observers/changelog` endpoint accepts `?latest=true`, returning only the single newest
  entry (`ORDER BY pk DESC LIMIT 1`, honouring the `object_type` filter) and echoing its cursor as
  `next_cursor`. This lets a consumer checkpoint at the real tail without replaying history — the
  server-side half of the `from_now` consumer fix (the consumer half lands in a later release).
- **Per-mutation `input_style: flatten | jsonb`, decoupling input-passing from the DML verb.**
  A new opt-in mutation flag controls how the GraphQL `input` argument reaches the SQL function,
  independently of `operation`. The executor takes the single-JSONB-argument path when
  `input_style == jsonb` **or** the operation is `Update` (today's behavior). This lets a backend
  using the single-JSONB wrapper convention (`fn(input_payload jsonb, …) RETURNS app.mutation_response`)
  register the *real* verb (`Insert`/`Delete`/`Custom`) and still receive the whole input as one
  `jsonb` arg — so the Change Spine records the true `modification_type` instead of a blanket
  `UPDATE` (creates and deletes were previously indistinguishable in the audit/CDC stream when a
  backend forced `operation = Update` purely to opt into single-JSONB passing). The forced
  single-JSONB path composes with the #400 acronym-aware input-key recasing. Surfaced as
  `@fraiseql.mutation(input_style="jsonb")` in the Python SDK, `@Mutation({ inputStyle: "jsonb" })`
  in the TypeScript SDK, and `input_style = "jsonb"` on a `[mutations.<name>]` table in the TOML
  schema. Fully opt-in and backward compatible: the default `flatten` is byte-for-byte today's
  behavior, and an absent value adds no compiled-schema bytes (no codegen schema-hash churn).
- **Per-mutation `changelog_pre_image` — opt-in Debezium-style pre-image for the Change Spine.**
  A new opt-in flag makes a mutation also record the changed entity's **before-state** alongside
  the after-state it already writes, into a new nullable `object_data_before JSONB` column on
  `core.tb_entity_change_log`. The pre-image is sourced from an optional `entity_before` on the
  mutation's `app.mutation_response` (the same way the after-image is sourced from `entity`), and
  the in-transaction outbox CTE reads `r.entity_before` **only when the flag is set**. `object_data`
  stays the after-image for *every* consumer — the pre-image is a separate column, never a
  `{before, after}` envelope — so audit-sensitive mutations (price/contract/order edits, financial
  deletes) get an inline `{before, after}` without paying that cost on every change. The
  out-of-band #366 capture trigger is unified on the same shape: it now writes `object_data = NEW`
  (after-image) and, for tables that opt in via `@subscribable(tables=[...], pre_image=True)`,
  `object_data_before = OLD`. A new `core.v_entity_change_log_debezium` view projects the classic
  `{before, after, op, source}` event from the columns (a view, not a stored shape). Surfaced as
  `@fraiseql.mutation(changelog_pre_image=True)` in the Python SDK,
  `@Mutation({ changelogPreImage: true })` in the TypeScript SDK, and `changelog_pre_image = true`
  on a `[mutations.<name>]` table in the TOML schema. Fully opt-in and backward compatible: the
  default is off (after-image only, byte-for-byte today's behavior) and an absent value adds no
  compiled-schema bytes (no codegen schema-hash churn). The nullable column is added to the
  PostgreSQL, MySQL, and SQL Server contracts for parity; only the PostgreSQL outbox CTE and
  capture trigger write it.

### Fixed

- **`SMALLINT`/`int2` columns now decode to JSON numbers instead of `null` (incl.
  `mutation_response.http_status`).** The PostgreSQL `row_to_map` decoder tried `i32`/`i64`
  for integers but had no `i16` branch, so a non-null `int2` value fell through the type
  ladder to `Null` (`FromSql for i32` rejects `int2`). The headline symptom: a failed
  mutation's `MutationError.httpStatus` came back **absent** — the `http_status` `SMALLINT`
  column nulled here, so the parser's `Option<i16>` read `None` and the projection's
  `if let Some(code)` guard skipped the field, while `errorClass` (a `TEXT` column) resolved
  fine through the same path. An `i16` branch is added to `row_to_map`, fixing `httpStatus`
  (404 not_found / 409 conflict / 422 validation / 500 internal) and every other `SMALLINT`
  column generally. No behavior change for any other column type.

- **GraphQL variables nested inside object/list literal arguments are now substituted.**
  A variable used as a value *inside* an object or list literal argument
  (`where: { field: { eq: $v } }`, `createMachine(input: { f: $v })`) was not resolved from the
  request `variables` map — only a whole-argument variable (`where: $where`, `input: $input`) was.
  Nested `$v` placeholders reached WHERE-clause SQL generation and mutation input coercion verbatim,
  so filters silently matched nothing and inline mutation inputs surfaced as a missing required
  argument. The matcher now recurses into object/list members (depth-bounded; an unknown variable
  resolves to `null`, matching GraphQL's treatment of an omitted nullable), and the mutation path
  carries the root field's inline arguments so an inline `input: { ... }` literal with nested vars
  is visible before required-argument validation. Whole-argument behavior is unchanged.

- **Mutation results now surface `updatedFields`, selection-gated (#433).** The executor
  surfaced the `cascade` wire payload without the SQL function embedding it in the entity
  JSONB, but its sibling envelope column `updated_fields` (the GraphQL field names a mutation
  changed) was parsed into the typed `mutation_response` row and then dropped at the success
  boundary, so `mutation { updateOrder(input: $input) { updatedFields … } }` silently returned
  no `updatedFields` key. The success arm now injects `updatedFields` symmetric with `cascade`,
  but **selection-gated** — present only when the client selects it (including inside an inline
  fragment), so a mutation that does not ask for it keeps an exact projected shape. An empty
  list (a noop) surfaces as `[]` when selected.
- **List field and argument types now compile to a list, not a single object (#434).** The CLI
  schema converter's `parse_field_type` matched built-in scalar names and routed everything else
  — including the SDL list string `"[Item!]"` — to `FieldType::Object`, so a list field arrived
  as `Object("[Item!]")`: a single object whose type name does not exist. The runtime then
  projected `parent { items { id } }` as `{"items": {"id": null}}` (one null object) instead of
  `{"items": [{"id": …}]}`. `parse_field_type` now unwraps an SDL list wrapper (`[Inner]` /
  `[Inner!]`, recursing for nested lists like `[[Inner!]!]`) into `FieldType::List`, and strips a
  trailing non-null `!` before matching the base name (outer-field nullability is tracked
  separately). This applies to both type fields and list query arguments (`ids: [ID!]`).
- **Digit-suffixed field names now camelize and resolve correctly (`phone_1` → `phone1`).**
  A field whose `snake_case` name ended in a digit segment (`phone_1`, `address_2`, `line_2`)
  was emitted into the GraphQL schema unchanged (`phone_1`) while every other field camelized,
  and the runtime could not map a collapsed digit field back to its JSONB key. The casing pair
  is now bijective, mirroring FraiseQL v1: the Python SDK and the engine's `to_camel_case`
  collapse the digit boundary (`phone_1` → `phone1`, `dns_1_id` → `dns1Id`), and the canonical
  reverse `to_snake_case` reinserts it (`phone1` → `phone_1`), so a field surfaced as `phone1`
  reads `data->>'phone_1'`. **Behavior change:** the GraphQL surface name of digit-suffixed
  fields changes from `phone_1` to `phone1`; clients querying the old `phone_1` name must switch
  to `phone1` (or add an explicit GraphQL alias). Common acronyms (`s3`, `ipv4`, `oauth2`, …)
  stay whole via the built-in acronym registry — extend it for your own `<word><digit>` keys
  with `[fraiseql.naming] acronyms` (see Added). An unregistered `<word><digit>` name still
  splits, so author the underscore form or add the acronym/an alias.
- **Mutation input recasing now covers nested composites on the Insert/Custom path (#400).**
  Under `naming_convention = "camelCase"`, the Update path already recased a mutation's whole
  `input` payload to the schema's canonical (`snake_case`) field names before it reached the
  SQL function; the Insert/Custom path recased only its top-level keys (which map to columns
  positionally), passing a *nested* composite input field as one JSONB arg with its keys
  verbatim — so a `jsonb_populate_record(NULL::config, $arg)` saw camelCase keys it could not
  read, silently writing NULLs (`affected_count = 0`). Both paths now share one
  `recase_input_field_value` helper that recurses into nested input objects and lists of them,
  so a create with `config: { s3Bucket, maxConnections }` reaches the function as
  `{ s3_bucket, max_connections }`. Recasing is driven by the input type's per-field map (not a
  lossy `camel→snake` regex), so it honours the acronym registry in both directions
  (`dns1Id` → `dns_1_id`, `s3Key` → `s3_key`) and leaves scalar values, enum values, and
  free-form JSON untouched; a `Preserve`-convention schema is unaffected. This completes the
  server-side `naming_convention` input work (#216/#400): a backend reading `snake_case`
  composite columns no longer needs a `jsonb_camel_to_snake(input)` SQL shim, for reads or writes.
- **Federation mutations recase input keys to canonical column names (#400, federation path).**
  The federation mutation builder turned GraphQL input variable keys *directly* into quoted SQL
  column identifiers (`INSERT INTO "users" ("s3Key") …`) and looked the `@key` value up by its
  canonical name, so a camelCase surface (`s3Key`, `dns1Id`) produced an `INSERT`/`UPDATE`
  against a column that does not exist — and the `UPDATE`/`DELETE` key lookup missed entirely
  (`Key field 'dns_1_id' missing`). `FederationMutationExecutor` now recases the input keys to
  their canonical `snake_case` names (via the same acronym-aware `to_snake_case`, scalar-only as
  federation mutations are) before SQL generation, gated by a `recase_input_keys` flag set from
  the schema's `naming_convention == CamelCase` (off for `Preserve`).
- **Mutation input recasing now also covers the single-JSONB-argument path when no Input type
  drives it (#400).** The field-driven recasing above only fires when a *registered* Input type
  supplies the per-field name map. A custom `mutation(input: JSON)` whose SQL function takes
  `(input jsonb, …)` — and an Update whose declared Input type is absent from the compiled
  schema — fell through to the catch-all argument path and reached the function with the whole
  object as one **verbatim camelCase** JSONB blob, so `jsonb_populate_record(NULL::…, input)` /
  `input->>'snake_field'` saw keys it could not read (spurious validation error or
  `affected_count = 0` no-op). The single-JSONB path now recases the object's keys itself: it
  uses the field-driven map when the Input type is known, and otherwise the canonical
  acronym-aware `to_snake_case` directly on the keys — recursing into nested objects and lists,
  leaving scalar values untouched. Because that is the same `to_snake_case` the read path uses,
  write keys round-trip exactly as reads do (`dns1Id` → `dns_1_id`, `s3Key` → `s3_key`,
  `ipv4Cidr` → `ipv4_cidr`, `oauth2Token` → `oauth2_token`). It is gated by
  `naming_convention == CamelCase` and scoped to the single `input`-named argument, so a
  `Preserve` schema, a plain-scalar `input` arg, and free-form JSON arguments on multi-argument
  mutations are all left untouched. This is the last single-JSONB-convention backend's reason to
  keep a hand-rolled `jsonb_camel_to_snake(input)` write shim.
- **Injected params now filter on a real column when the view has one (native-column inference
  gap).** Compile-time native-column inference (`database_validator.rs`) consulted only a query's
  explicit arguments, so an injected param (e.g. a `tenant_id` from a JWT claim) was never added to
  `native_columns`. The runtime then rendered `WHERE data->>'tenant_id' = $1` even when the backing
  view had a real `tenant_id` column — returning 0 rows for inject-scoped list queries whose views
  keep `tenant_id` as a column (not inside `data`). Native-column inference now consults inject-param
  names against the introspected columns too; a match renders `WHERE tenant_id = $1::uuid`.
  Explicit-arg behaviour (including the `NativeColumnFallback` warning) is unchanged, and inject-param
  misses stay silent (a claim may legitimately live in the `data` JSONB). Requires recompiling with
  `--database` so the inference can see the view's columns.
- **SDK publishing is no longer silently frozen (H30, release integrity).** `tools/release.sh`
  bumped the Rust manifests but never the SDK manifests, so the Python `pyproject.toml`/
  `__init__.py` and the npm `package.json`/`package-lock.json` stayed pinned at `2.1.6`
  (the TypeScript `version` constant had drifted further, to `2.0.0-alpha.1` — L-ts-version).
  Each `v*` release then built that stale version; `twine upload --skip-existing` and the
  npm "already published, skipping" branch no-oped, and the validation step installed the
  *old* version — so v2.3.0–v2.6.0 SDK publishes reported success while shipping nothing.
  `release.sh` now bumps all SDK manifests in lockstep with the crates, and the
  `publish-python`/`publish-typescript` jobs gained a fail-loud gate
  (`assert_sdk_version_matches`) that refuses to publish when the manifest version does not
  match the release tag; the validation steps now assert the *new* tag version specifically.
  New unit coverage in `make test-release-tooling` exercises the bump and the gate.
- **Python SDK: `except fraiseql.FraiseQLError` now catches async-client errors (H27).**
  There were two unrelated `FraiseQLError` classes — one in `client.py` (the package-level
  `fraiseql.FraiseQLError`) and one in `errors.py` (the base of `GraphQLError`/`NetworkError`/
  `TimeoutError`/`AuthenticationError` raised by `AsyncFraiseQLClient`). They shared a name
  but no inheritance, so the documented catch-all silently caught nothing the async client
  raised (`issubclass` was `False`). The hierarchy is now consolidated in `errors.py` under a
  single `FraiseQLError` base; `client.py` re-exports it, so both clients' errors are
  catchable as `fraiseql.FraiseQLError` and existing `from fraiseql.client import FraiseQLError`
  imports keep working. The two clients deliberately classify differently (async: HTTP status;
  sync: GraphQL `extensions.code`), now documented on the module. Behaviour change: code that
  relied on the catch-all *not* catching async errors will now catch them.
- **Python SDK: `ChangelogConsumer(startup_mode="from_now")` no longer replays history (H28).**
  `_initialise_cursor` fetched the first page (`after_cursor=0, limit=1`) and checkpointed at
  its `next_cursor` — the *oldest* entry's cursor — so the next poll replayed almost the entire
  changelog with side effects. It now resolves the real tail via the `?latest=true` tail query
  (Phase 09), then pages forward to the true tail (correctness on older servers that ignore
  `?latest`), checkpointing there and processing zero pre-existing rows.
- **SDK correctness cluster.** Several SDK behaviour bugs:
  - **Python `AsyncFraiseQLClient` honours `RetryConfig.retry_on` (M-retry-config).** The
    retry loop's `except` tuple was hardcoded to `(NetworkError, TimeoutError)`, so a custom
    `retry_on` (e.g. `AuthenticationError`) was never caught and the request ran once instead
    of `max_attempts` times. It now catches broadly and lets `RetryConfig.should_retry` decide.
  - **Python `export_schema(include_custom_scalars=False)` now drops the block
    (M-export-schema).** The filter checked the snake_case key `custom_scalars` while the
    registry emits camelCase `customScalars`, so the flag was a no-op. (The neighbouring test
    passed vacuously — it never registered a scalar.)
  - **Python injected clients keep the configured Authorization (L-sdk-injected-client).**
    `AsyncFraiseQLClient` and `ChangelogConsumer` discarded the `authorization` argument when a
    client was injected; they now apply it to the injected client's headers.
  - **TypeScript: malformed `inject` specs are rejected, not silently dropped (M-ts-inject).**
    `normaliseConfig` dropped any spec without a `jwt:<claim>` shape; it now validates the
    param identifier, the `jwt:<claim>` source, and argument-name collisions and throws —
    matching the Python SDK's `_validate_inject`.
- **SDK: the no-op `config()` helper is removed (H29).** Both SDKs shipped a `config()` that
  the docs told users to `return` from a decorated function (`return fraiseql.config(sql_source=...)`)
  — but its result was stored in a holder nothing ever read, so the call did nothing. Removed
  `config()`/`_ConfigHolder` (Python) and `config()`/`getPendingConfig`/`ConfigHolder`
  (TypeScript), with their package exports, and corrected the docstrings/examples to the real
  pattern: pass config as decorator arguments (`@fraiseql.query(sql_source="v_user")`) or via
  `fraiseql.toml`.
- **One cross-SDK parity comparator; empty output fails (M-parity-comparators).** Two
  comparators had drifted: the strict, CI-wired `sdks/official/tests/compare_schemas.py` and a
  lenient copy `tools/compare_parity_schemas.py` that *skipped* any item missing from a
  candidate — so an SDK generator emitting nothing passed vacuously. The lenient copy is
  removed and `make parity-compare` now uses the strict comparator (which hard-fails when
  type/query/mutation name sets differ, including against empty output).
- **`tools/lint.sh` reports failures honestly (L-lint-sh).** The `sql-helpers-sync` check
  called `fail`/`pass` itself and then returned 0, so `run_check` *also* printed ✅ on a real
  divergence; it now returns a status and lets `run_check` report. The `lint-gate-db` count
  used `grep -c … || echo 0`, which emitted a two-line `"0\n0"` on no match and broke the
  numeric comparison; it now uses `|| true`.
- **Wire hygiene cluster (L-wire-*).** A set of low-severity wire-crate correctness fixes:
  - **`Field::JsonbField` extracts text (`->>`) as documented (L-wire-jsonb).** It emitted
    `(data->'field')` (JSONB) while its own doc and the `sql_gen` cast strategy assume text
    extraction — so a string comparison saw a quoted JSON value and the numeric/inet/ltree
    casts had no valid source type. It now emits `(data->>'field')`.
  - **Connection-string credentials are percent-decoded (L-wire-connstr).** The userinfo
    parser split on the *first* `@` and never decoded `%XX` escapes, so a password containing
    `@`, `:`, or `%` was mangled. It now splits on the last `@` and percent-decodes the user
    and password (rejecting malformed `%` escapes).
  - **`connect_timeout` is now applied (L-wire-timeout).** The config field was parsed but
    never used; `connect_with_config`/`connect_with_config_and_tls` now bound the
    transport-connect future with it, surfacing a lapse as `WireError::Connection`.
  - **The SCRAM PBKDF2 result is propagated, not discarded (L-wire-scram).** Both key
    derivations did `let _ = pbkdf2(...)`; a swallowed error would have left an all-zero
    salted password and silently produced a wrong proof. The result is now checked (a new
    full round-trip test verifies the client proof against an independently-derived server
    key).
  - **Adaptive-chunking builder options now take effect (L-wire-builder).** `execute_query`
    hardcoded adaptive chunking off and dropped the builder's
    `adaptive_chunking`/`adaptive_min_size`/`adaptive_max_size`; the options are now threaded
    through and the streaming loop actually observes channel occupancy and retunes the batch
    size. The builder default is now explicitly off (preserving the prior effective
    behaviour — fixed-size chunking is the zero-overhead path).
  - **`StreamStats` row counters are populated (L-wire-stats).** `total_rows_yielded` /
    `total_rows_filtered` were always zero; the stream now counts rows yielded to the consumer
    and rows rejected by a `QueryStream` predicate.
  - **De-duplicated the chunk-flush logic (L-wire-chunk-dup).** Two ~70-line copies with
    drifted error termination (the final-chunk path reported success even after the consumer
    dropped) were factored into one `stream_chunk_rows` helper that fails consistently.
  - **Removed a 29 MB `test_import` ELF binary committed to the repo (L-wire-elf).**
- **Wire `metrics`-facade emissions are now captured by an installed recorder (H45).** The
  workspace carried two incompatible `metrics` facade versions — `fraiseql-wire` emitted via
  `metrics` 0.22 while the server's `metrics-exporter-prometheus` was built against 0.24 — and
  no recorder was installed at all, so the emission and the (absent) recorder bound to
  different process-global statics and every one of wire's ~40 counters/histograms/gauges was
  silently dropped. `fraiseql-wire` is bumped to `metrics` 0.24 (single facade version in the
  lock), the server installs a process-global `PrometheusBuilder` recorder at startup behind
  the `metrics` feature, and the `/metrics` endpoint appends the rendered facade metrics to
  its hand-rolled output. The server's unreferenced direct `metrics` 0.22 dependency (the
  server emits its own metrics via hand-rolled atomics, not the facade) was dropped.
- **Wire stream pause/resume now actually reaches the background reader (H43).**
  `JsonStream` allocated its pause/resume state lazily, on the first `pause()` call — but the
  background reader task had already captured `None` clones of those handles at spawn time,
  so `pause()`/`resume()` never affected it: the reader streamed on regardless, and the
  pause-timeout and paused-occupancy metrics were permanently dead. The state is now
  allocated eagerly in `JsonStream::new`, so the reader shares the same handles the caller
  drives. As a result: `pause()` parks the reader at the next chunk boundary (and records the
  buffered-row count in `paused_occupancy()`); `set_pause_timeout` is honoured live via a
  shared handle (and the auto-resume timeout metric fires); and a drop-while-paused now tears
  the reader down cleanly instead of leaking a task blocked forever (the pause wait also
  selects on cancellation). The dead `pause_signal` (notified but never awaited) was removed.
- **The wire connection no longer hangs on a malformed, unrecognized, or ordinary
  control message (H42).** `receive_message` decoded with `if let Ok(..)`, discarding the
  error kind and treating *every* decode failure as "the frame is incomplete, read more
  bytes" — so a malformed message, an unknown tag, or an unsupported message looped forever,
  buffering toward the size cap. Decode errors are now classified by `io::ErrorKind`: only
  `UnexpectedEof` reads more; `InvalidData`/`Unsupported`/oversized are fatal and surface as
  `WireError::Protocol`. Decode arms were added for the ordinary `EmptyQueryResponse` (`I`,
  the reply to an empty query) and `NotificationResponse` (`A`, `LISTEN`/`NOTIFY`) — which
  were previously mistaken for unknown tags and wedged `simple_query("")` and any session
  that received a `NOTIFY` — and the `COPY` family (`G`/`H`/`W`) now decodes to an explicit
  `Unsupported` error rather than an infinite wait.
- **Federation local mutations read the row back instead of echoing the input (#430,
  M-fed-mut-executor).** `execute_local_mutation` built its response from the input `variables` and
  ran the `INSERT`/`UPDATE`/`DELETE` without inspecting the result, so it returned a fabricated
  "success" even when an `UPDATE`/`DELETE` matched no row (the entity didn't exist), and never
  reflected database-computed columns. The mutation SQL now uses `RETURNING *`; the response is the
  actual post-mutation row (`__typename` plus every returned column), and a 0-row `UPDATE`/`DELETE`
  returns `FraiseQLError::NotFound` (404). **Behavior change:** a federation mutation against a
  non-existent entity now fails loud instead of reporting success. (Un-parks the two
  `mutation_cross_graph` tests that were deferred to this work.)
- **The MSSQL→NATS bridge honours its configured `batch_size` (M-mssql-batch).** The change-log
  fetch query hardcoded `SELECT TOP (100)` and discarded the configured `batch_size` (a `let _ =
  batch_size` swallowed it), so a deployment that tuned the batch size was silently capped at 100
  rows per poll. SQL Server accepts a parameter in `TOP (expression)`, so the row cap is now bound
  (`TOP (@P1)`) from the configured value.
- **Twilio webhook signature verification decodes form bodies correctly (H44).** The
  percent-decoder pushed each decoded byte as its own `char` (Latin-1 per byte), so a UTF-8
  sequence like `%C3%A9` became `Ã©` instead of `é`, and `+` was never decoded to a space — so a
  legitimately-signed webhook whose body contained an accented character or a space failed
  verification. Decoding now accumulates bytes and interprets the result as UTF-8, and `+` decodes
  to a space. The vacuous test helper that re-implemented the in-repo signing algorithm (verifying
  the bug against itself) is deleted; the new tests sign with Twilio's published algorithm
  independently.
- **Webhook replay protection no longer wraps to reject every request (M-webhook-replay-drift).**
  `SlackVerifier`/`SendGridVerifier::with_tolerance` cast the `u64` tolerance with `as i64`, so a
  large configured tolerance wrapped to a *negative* window that rejected every timestamp
  (replay protection inverted into a total outage) — the wrap-safe fix had landed only in the
  Discord and Paddle copies. All five timestamped verifiers (Slack, SendGrid, Discord, Paddle,
  Stripe) now share one `check_timestamp_freshness` seam that stores the tolerance as a `u64` and
  saturates it to `i64::MAX` at comparison time, so the freshness logic can't drift between
  providers again.
- **Webhook errors map to an HTTP status that reflects fault (M-webhook-error-status).** Every
  `WebhookError` variant boxed into `FraiseQLError::Webhook`, which maps to HTTP 400 — so a
  transient database error while handling a webhook returned 400 ("permanent client error, do not
  retry") and the event was lost. The conversion now routes per variant: `Database` → 5xx
  (retryable, the sender re-delivers), `MissingSecret` → 5xx (a server-side misconfiguration),
  and only `InvalidPayload` (a genuinely malformed sender payload) stays 400.

- **Arrow schema inference maps JSON null to `Utf8`, not `DataType::Null` (H37).**
  `schema_gen`'s `infer_type_from_value` mapped a JSON `null` to `DataType::Null`, which the
  Arrow array converters reject — so a result column whose *first* row was `null` poisoned the
  entire batch, while the sibling `metadata.rs` path correctly mapped null to a nullable `Utf8`
  column. Both paths now route through one shared `json_value_to_arrow_type` helper (null →
  `Utf8`), so they cannot drift again; a pre-existing test that asserted the buggy `DataType::Null`
  result was corrected.
- **The S3 storage backend detects a missing object structurally (H40).** `download()` and
  `exists()` decided "not found" by string-matching the `SdkError` Display for `"NoSuchKey"` /
  `"404"` — but that Display is just `"service error"` (the status lives in the typed error), so
  the match never fired: `exists()` returned an error instead of `Ok(false)` and `download()` of a
  missing key surfaced a generic 500 instead of a 404. Both now inspect the typed service error
  (`GetObjectError::is_no_such_key` / `HeadObjectError::is_not_found`), matching the structural
  pattern already used in the server's storage path.

- **The PostgreSQL adapter no longer nulls NUMERIC, UUID, and timestamp columns
  (H35).** `row_to_map` decoded a fixed ladder of types (`i32`/`i64`/`f64`/`String`/`bool`/
  `text[]`/`jsonb`) and fell through everything else to `Null`, so a `SUM(revenue)` aggregate,
  any raw `NUMERIC`/`DECIMAL` column, a `uuid` column (e.g. `mutation_response.entity_id`), and
  `timestamptz`/`timestamp`/`date` columns all silently became JSON `null`. The ladder now
  decodes `NUMERIC`/`DECIMAL` (as a JSON number, via `rust_decimal`), `UUID` (canonical string),
  and chrono timestamps/dates (ISO 8601 text); a column whose type still isn't representable is
  logged with its name and PostgreSQL type instead of nulling silently. A cross-type conformance
  test pins the mapping so the next drift fails a shared test.
- **MySQL database errors now carry a usable SQLSTATE (H36).** The `execute_raw` path parsed
  `db_err.code()` — which already *is* the SQLSTATE string — as a MySQL error *number* and fed it
  to `map_mysql_error_code` (which expects numbers like 1062), so the mapping never matched and
  every raw-query error surfaced with `sql_state: None`; the #413 client-input classifier never
  mapped a MySQL constraint violation to HTTP 400. All SQLSTATE extraction in the adapter is now
  routed through one `mysql_sql_state` seam that reads MySQL's native error number via downcast,
  normalises the well-known integrity/serialization numbers to canonical SQLSTATEs, and falls
  back to MySQL's own SQLSTATE — so a duplicate-key violation now classifies as 400. The drifted
  inline copies (and the duplicate `map_mysql_error_code` in `helpers.rs`) are removed.

- **Federation `_entities` results are now positionally aligned to the input
  representations (H31).** The resolver grouped representations by typename and re-numbered the
  resolved entities with a per-group running counter, so for an interleaved request like
  `[User#1, Product#1, User#2]` the result array came back in group order
  (`[User#1, User#2, Product#1]`). Apollo Router zips the `_entities` result against the input
  array **by index**, so every consumer downstream of an interleaved batch received the wrong
  entity for a representation. Grouping now records each representation's original input index
  and scatters the resolved entities back to those positions, so the result array zips 1:1 with
  the input regardless of typename interleaving.

- **Every server constructor now applies the same schema-derived runtime config and boot
  validation (H16).** `Server::with_relay_pagination` and `Server::with_flight_service` (the
  Arrow Flight path) built the executor with `RuntimeConfig::default()`, so a server created
  via either constructor silently ignored the compiled `audit_logging_enabled` flag, the #421
  `max_page_size` ceiling (and its `FRAISEQL_MAX_PAGE_SIZE` override), and the change-log
  write toggle — and, unlike `Server::new`, never validated the compiled schema's format
  version or ran the at-rest-encryption refusal check (H12). The schema-derived config now
  flows through a single seam, `RuntimeConfig::from_compiled_schema`, that all three
  constructors call; the format-version validation is coupled into it so a constructor cannot
  obtain a config while skipping the check. The relay/Arrow constructors additionally run the
  H12 field-encryption boot refusal. (The #421 `page_size_precedence` helper moved from
  `fraiseql-server` to `fraiseql-core` alongside the seam.)
- **Authenticated multi-root queries no longer silently drop roots (H19).** The authenticated
  executor entry point (`execute_with_security`) had no multi-root branch, so a query like
  `{ users { id } posts { id } }` matched only the first root and silently discarded the rest;
  the anonymous path dispatched all roots in parallel. Both paths now route through one shared
  `execute_dispatch(.., Option<&SecurityContext>)` so the authenticated path also fans multi-root
  queries out in parallel (with the security context applied to every root), runs the GATE-1
  query-structure validator it previously skipped (L-gate1-skip), and consults the parse cache it
  previously bypassed (L-parse-cache). The `fraiseql_multi_root_queries_total` metric now counts
  authenticated multi-root queries too. (Also corrects a stale doc claim that the security context
  was "not yet applied" to aggregations/window/federation — it is, on both paths.)
- **REST error responses now use the correct HTTP status for every error variant
  (M-rest-error-mapper).** The REST `From<FraiseQLError>` mapper handled only a handful of
  variants and sent everything else to `500`, so `Conflict` (should be 409), `Timeout`/`Cancelled`
  (408), `RateLimited` (429), `ServiceUnavailable` (503), and `Unsupported` (501) were all reported
  as `500 Internal Server Error`. REST status is now derived from the canonical
  `FraiseQLError::status_code()` — the single source of truth shared with the GraphQL mapper
  (L-error-map-triplication) — with the one documented divergence being the #413 client-input
  SQLSTATE override (22xxx/23xxx → 400). A property test asserts REST status equals
  `status_code()` for every variant.
- **Observer audit-log write failures are no longer silently swallowed
  (M-observer-log-swallow).** The success- and error-path `INSERT INTO tb_observer_log` writes in
  the observer runtime discarded their result with `let _ = …`, so a failed audit-log write left no
  trace. Both now `warn!` with the observer and event id on failure (non-fatal — the event itself
  is already processed/counted).
- **Removed the dead `PreferHeader::applied_header_value` builder (L-prefer-header).** It built an
  RFC 7240 `Preference-Applied` header value that no production code emitted, and carried a no-op
  `resolution` branch. Emitting `Preference-Applied` is a deliberate REST feature to be added with
  its response-path wiring, not kept as dead code.
- **Error sanitization now defaults to ON in production (H7, behavior change).** A default
  deployment with no explicit `[security.error_sanitization]` config previously ran with
  sanitization disabled, so raw database/SQL error text (schema names, constraint detail, SQL
  fragments) could reach clients in `5xx` responses. The default is now **environment-aware** at the
  server boot seam: when `FRAISEQL_ENV` is not `development`/`dev` (i.e. production), sanitization
  is enabled; in development it stays disabled for verbose-error ergonomics. An explicit compiled
  config still overrides in either direction. The pure `ErrorSanitizationConfig::default()` shared
  with `fraiseql-cli` is unchanged (still `enabled = false`); only the runtime boot default flips.
  **Operators who relied on raw 5xx error text in production must set
  `[security.error_sanitization] enabled = false` explicitly.**
- **CLI gate flags now affect the exit code (H21).** `fraiseql lint --fail-on-critical` and
  `--fail-on-warning` printed a failure result but always exited 0, so they were inert as CI
  gates — a pipeline depending on them passed regardless of the findings. Lint now reports a
  `validation-failed` status when a gate trips and the runner exits **2** (the documented
  `validation_failed` code); operational errors (missing file, bad JSON) still exit 1. The
  lint output schema's failure variant is updated to `validation-failed` to match.
- **`fraiseql federation check` exits non-zero on a composition failure (H22).** A subgraph
  with composition errors (e.g. a federated type missing `@key`) printed the errors but
  exited 0, so federation composition gates in CI never failed. The command now exits 2 when
  the result is `validation-failed`.
- **`fraiseql setup` installs the dollar-quoted helper library correctly (#426).** The
  installer split the embedded SQL on `;` and ran the fragments individually, which shredded
  the `$$…$$` PL/pgSQL function bodies and the trailing `DO`-block self-test — so on a clean
  database it failed on the first body and installed zero helpers, leaving the documented
  install path unusable. It now runs the file as a single `batch_execute` (simple-query
  protocol), which understands dollar-quoting and multi-statement scripts the same way
  `psql -f` does.
- **`fraiseql compile` refuses to write its compiled output over the input file (H23,
  defense-in-depth).** A real write now errors when `--output` resolves to the input path,
  preventing the same source-clobbering class that motivated removing `serve` (below).
- **`PostgresSagaStore` no longer silently coerces corrupt state and ignores missing rows
  (M-saga-store-defaults, M-saga-rowcounts).** Row mappers coerced an unrecognised
  `state`/`mutation_type` string to a default (e.g. `Pending`), which could re-execute
  completed work; they now raise `SagaStoreError::CorruptStoredValue`. Step/saga writes
  ignored the affected-row count, so an update targeting a non-existent saga/step returned
  `Ok`; they now check it and raise `SagaNotFound`/`StepNotFound`.
- **The server refuses to boot when `FRAISEQL_SECRETS_BACKEND` is set on a build without
  the `secrets` feature (M-secrets-backend-stub).** The no-`secrets` build's
  `build_secrets_manager` returned `Ok(None)` unconditionally, so an operator who configured
  a secrets backend silently ran with none — believing secrets were managed when they were
  not. It now fails loud with an explicit error telling the operator to rebuild with
  `--features secrets` or unset the variable.
- **The `sql_query` host function fails loud instead of faking an empty result set
  (M-sql-query-stub).** A read-only-classified `SELECT` returned `Ok(vec![])` ("not yet
  implemented"), making a valid query look like it ran and matched no rows; it now returns
  `FraiseQLError::Unsupported`.
- **Deno and WASM function runtimes now share one failure contract (M-fn-failure-contract).**
  A guest WASM error was wrapped as *successful data* (`Ok(FunctionResult { value:
  {"error": …} })`) while the Deno runtime returned `Err` for the same failure. The WASM
  path now returns `Err(FraiseQLError::Unsupported)` for guest errors, timeouts, and traps,
  matching Deno — a guest failure can no longer be silently consumed as data.
- **Deno function duration is measured across execution, not just channel setup
  (M-deno-duration).** The elapsed time was captured immediately after spawning the executor
  thread, before awaiting the result, so reported durations were meaningless; it is now
  measured after the executor completes.
- **Federation mutation executor rejects unrecognised operation names (M-fed-mut-executor,
  partial).** `determine_mutation_type` defaulted any name without a `create`/`update`/`delete`
  prefix to `UPDATE`, so a typo'd or unsupported mutation silently issued an `UPDATE`; it now
  errors. The remaining read-back correctness (return the mutated row via `RETURNING` instead
  of echoing the input; treat 0-row `UPDATE`/`DELETE` as not-found) is documented and deferred
  to Phase 09 ([#430](https://github.com/fraiseql/fraiseql/issues/430)). The two cross-graph
  integration tests that relied on the old silent default (`verifyUser`,
  `executeTransaction`) are parked with `#[ignore]` pointing at the same issue, so they still
  compile as the acceptance spec for the Phase 09 rework.
- **`InMemoryStateStore` now evicts the oldest entry at capacity instead of returning 500
  (L-state-store-doc).** The struct documented LRU-style eviction, but `store` returned a
  `ConfigError` (500) once the cap was reached — an availability footgun under CSRF-state
  flooding. It now evicts the oldest (smallest-expiry) state to admit the new flow, keeping
  the map bounded while new logins keep working. Clock-read failure still fails closed
  (the store rejects rather than admitting a state whose TTL cannot be validated).
- **Clock failures now fail closed in four auth expiry checks (L-clock-failopen).**
  `Session::is_expired`, `OtpRecord::is_expired`, the multi-provider callback CSRF-state
  check, and `InMemoryStateStore::cleanup_expired` read the clock with
  `unwrap_or_default()`/`unwrap_or(0)`, so a clock failure yielded `now = 0` and treated
  expired sessions/OTP codes/CSRF states as still valid (fail-open) — contradicting the
  crate's fail-closed doctrine. They now treat an unreadable clock as expired/at-capacity.
- **`JwtValidator::validate_hmac` now emits the same audit log as `validate` (L-validate-hmac).**
  The HMAC path logged nothing on decode failure, expiry, temporal-claim rejection, or
  success; it now mirrors the asymmetric path's four audit points.
- **`auth_refresh` no longer records an audit success for a request that always fails
  (L-auth-refresh-500).** Access-token issuance (signing a JWT) is not wired, so refresh
  cannot complete; it logged `SessionTokenValidation` *success* and then returned 500. It
  now logs the refresh as a failure and returns the explicit not-implemented error.
- **Vault `rotate_secret` no longer self-deadlocks (H10).** It held the per-secret
  rotation mutex and then called `get_secret_with_expiry`, which re-acquired the same
  non-reentrant lock — a permanent hang on first invocation that wedged the
  lease-renewal loop. The fetch+cache body is now a lock-free helper both methods call
  while holding the lock exactly once.
- **Vault Transit encrypt/decrypt use padded standard base64 (H14).** Encryption sent
  `STANDARD_NO_PAD` plaintext (real Vault's Go `base64.StdEncoding` rejects unpadded for
  ~2/3 of lengths) and decryption decoded Vault's always-padded response with
  `STANDARD_NO_PAD` (errors on the trailing `=`). Both directions now use padded
  `STANDARD`, so Transit round-trips against a real Vault.

- **Account linking no longer collapses email-less provider identities into one account (H26, account takeover).**
  `link_or_create_user` previously keyed every account on the provider's email and treated a
  missing email as the empty string, so every user whose provider omits an email (a GitHub
  account with a private email is the canonical case) resolved to the **same** `user_id` —
  cross-user account takeover. Account linking is now fail-closed: cross-provider linking
  happens only when the provider supplies a non-empty, **verified** email; otherwise the
  identity is keyed on `(provider, provider_id)`, so distinct identities can never collapse and
  an unverified email can never link into another user's account.

### Changed

- **BREAKING (JSON-compile workflow only): the default naming convention is now `camelCase`,
  not `preserve`.** The `fraiseql-cli compile schema.json` + `fraiseql.toml` workflow now
  compiles to a `camelCase` GraphQL surface by default — `snake_case` columns/functions in the
  database, `camelCase` operation and field names exposed to clients, with mutation input keys
  recased `camelCase → snake_case` before they reach the SQL functions. This matches the
  standard GraphQL convention and the casing most (JS) clients expect. The default applies even
  when no `fraiseql.toml` is present. **Migration:** a backend relying on the old `snake_case`-
  on-the-wire behavior must set `[fraiseql.naming]\nconvention = "preserve"` to keep names exactly
  as authored. The author-in-TOML workflow (`TomlSchema`) is unaffected — it carries its own
  `naming_convention` (still defaulting to `preserve`).
- **The #366 external-write capture trigger now writes the after-image into `object_data`, not a
  `{op, before, after}` envelope (changelog_pre_image unification).** To make `object_data` the
  after-image from *every* producer (executor outbox AND capture trigger), the shipped capture
  trigger function (`core.fn_entity_change_log_capture`) now writes `object_data = to_jsonb(NEW)`
  (NULL for a DELETE) and, only for tables that opt into the pre-image, `object_data_before =
  to_jsonb(OLD)`; the Debezium `op` is the `modification_type` column. The change-log reader's
  `ChangeLogEntry::debezium_operation` / `after_values` / `before_values` were updated to match
  (op derived from `modification_type`, after from `object_data`, before from `object_data_before`).
  **Migration note:** any consumer that read trigger-captured `object_data` as a `{op,before,after}`
  envelope must switch to the column shape (or read the new `core.v_entity_change_log_debezium`
  view, which reconstructs the envelope). Executor-written rows are unaffected — they already wrote
  the after-image into `object_data`.
- **Default builds now link a single rustls crypto provider — ring (M-dual-crypto).** Every
  default build previously compiled *both* `aws-lc-rs` and `ring` into one `rustls 0.23`
  because `fraiseql-server` and `fraiseql-wire` pulled rustls/tokio-rustls with their default
  `aws_lc_rs` provider while the rest of the graph (reqwest, sqlx, lettre, tungstenite) used
  ring. The server's direct `rustls`/`tokio-rustls`/`rustls-pemfile` deps were dead (their
  `ServerConfig` plumbing was removed in v2.7.0) and are now dropped; `fraiseql-wire` pins
  `default-features = false` + `ring`. A new gate, `tools/check-crypto-providers.sh` (wired into
  `make security` and the Dagger security leg), asserts the default `fraiseql-server` build
  links one provider and one rustls major. The opt-in `metrics` and `aws-s3` features still pull
  additional stacks by design (documented in the gate).
- **The two side-by-side WebSocket stacks are collapsed to one (L-ws-stacks).** `tokio-tungstenite`
  is bumped `0.28 → 0.29` to match axum 0.8's transitive version, so `tungstenite`/`tokio-tungstenite`
  no longer compile twice; the corresponding `deny.toml` skip entries are removed.
- **BREAKING (`fraiseql-auth`):** `UserInfo.email` is now `Option<String>` (was `String`) and
  gains an `email_verified: bool` field; an empty/whitespace email claim is normalized to
  `None`. `AccountStore::link_or_create_user` now takes `(email: Option<&str>, email_verified:
  bool, provider, provider_id)` (was `(email: &str, provider, provider_id)`), and
  `AccountRecord.email` is now `Option<String>`. Implementors and direct callers of these
  published-crate APIs must update their signatures; the in-tree OAuth providers and handlers
  are already updated.
- **BREAKING (`fraiseql-auth`):** `AuthMiddleware::new` no longer takes `session_store` or
  `optional` (now `new(validator, public_key)`). Those parameters were stored but never
  consulted — no session-revocation check, no optional-auth handling — so they were removed
  rather than continue to advertise behavior that did not exist (L-authmw-ignores).
- **BREAKING (`fraiseql-cli`): the hidden `serve` command is removed (H23).** It derived its
  output path from the input via an extension swap (`.json` → `.compiled.json`); given an
  input with no `.json` segment (e.g. `serve fraiseql.toml`) the derived output path equalled
  the input, so it overwrote the source file with compiled output. Use `fraiseql run --watch`
  (compiles in-memory, no disk artifact, hot-reloads on change) instead.
- **BREAKING (`fraiseql-webhooks`): the crate docs no longer advertise capabilities it does
  not have, and the dead scaffolding is removed (M-webhooks-advertised).** The crate docs
  claimed built-in **idempotency** and **transaction boundaries** as Security Properties, but
  no inbound receiver pipeline exists — the crate provides signature verification and the
  `SignatureVerifier` trait as building blocks; the caller wires the pipeline. The docs now
  state that honestly (and a Paddle "RSA-SHA256" error was corrected to HMAC-SHA256). The 12
  never-constructed `WebhookError` variants and the unused `WebhookConfig`/`WebhookEventConfig`
  types are **removed** from the published API (the enum is `#[non_exhaustive]`, so exhaustive
  external matches already carry a wildcard arm). The real receiver pipeline is tracked in
  [#431](https://github.com/fraiseql/fraiseql/issues/431).
- **BREAKING (`fraiseql-federation`): distributed saga execution now fails loud instead of
  fabricating success (H32, H33, M-saga-coordinator, M-saga-recovery).** `SagaExecutor`
  (`execute_step`/`execute_saga`/`get_execution_state`), `SagaCompensator`
  (`compensate_saga`/`compensate_step`), `SagaCoordinator` (`create_saga`/`execute_saga`/
  `get_saga_status`/`cancel_saga`/`get_saga_result`/`list_in_flight_sagas`), and
  `SagaRecoveryManager` (`run_iteration`/`start_background_loop`) previously fabricated and
  **persisted** success — building fake result documents, marking sagas `Completed`/
  `Compensated` having done nothing, and (the coordinator) holding `Arc<dyn Any>`
  executor/compensator fields that contained `()`. They now return
  `SagaStoreError::NotImplemented`; nothing is persisted. The coordinator's
  `with_executor`/`with_compensator` builders (which accepted unusable `Arc<dyn Any>` values)
  are **removed**. The `lib.rs` maturity table no longer advertises sagas as production. The
  real implementation is planned and tracked in
  [#429](https://github.com/fraiseql/fraiseql/issues/429); the behavioural acceptance suite is
  retained (parked) as its specification.
- **BREAKING (`fraiseql-federation`): `construct_batch_where_clause` is removed
  (M-batch-where-dup).** It was a drifted, weaker duplicate of the production
  `construct_where_in_clause`: it interpolated key values as string literals and returned an
  empty (WHERE-less, full-table) clause when no conditions matched. It had no production
  caller; use `construct_where_in_clause`, which binds values as parameters and fails closed
  (`1 = 0`) on empty input. Compound-key coverage was ported to the canonical builder.
- **BREAKING (`fraiseql-observers`): the `sms`, `push`, `search`, and `cache` observer action
  types now fail loud instead of fabricating success (H24).** Their dispatch handlers
  delegated to stub actions that returned `success: true` and sent nothing — an observer
  configured with `type = "sms"` reported success on every event while delivering no SMS.
  `ActionConfig::validate()` now rejects these types with `ObserverError::UnsupportedActionType`
  at config-load time (a misconfigured observer refuses to start), and the dispatcher returns
  the same error at execution time. The fabricating stub types (`SmsAction`, `PushAction`,
  `SearchAction`, `CacheAction` and their `*Response` types) are **removed** from the public
  API. The `ActionConfig` enum variants are retained so existing configs still deserialize and
  receive a clear error. Real transports are tracked in
  [#428](https://github.com/fraiseql/fraiseql/issues/428).

## [2.7.0] - 2026-06-13

### Security

- **Complexity validator no longer pins a worker on crafted fragment spreads (H4, DoS).**
  The depth/complexity analyzer re-walked every fragment spread with no memoization, so a
  ~1 KB query with N chained fragments each spread `b` times forced `b^N` recursive walks —
  the audit's 31-fragment / branch-2 construction pins a Tokio worker for ~88 s, and because
  the full metric was computed *before* any limit comparison, the configured depth/complexity
  limits never got a chance to reject it (the validation step itself was the DoS, and the
  opt-in `TimeoutLayer` cannot preempt synchronous CPU-bound work). Each fragment's
  depth/complexity/alias contribution is now resolved exactly once and memoized by name, with
  fragment cycles detected and treated as over-limit (rejected, never recursed into) and an
  over-long spread chain capped as before — making validation linear in document size
  regardless of fragment topology. The same pass also closes a companion alias-amplification
  bypass: the old alias counter scored fragment spreads as 0, so aliases hidden inside a
  fragment spread many times never counted toward `max_aliases`; each spread now contributes
  the fragment's own alias count per occurrence. No configuration change; depth, complexity,
  and alias metrics are unchanged for all non-pathological queries.
- **REST `?select=` parser no longer panics on multi-byte UTF-8 or unbounded nesting
  (H17, H18; `rest` feature).** The parser walked a `Vec<char>` by character position but
  then byte-sliced the original `&str` with those positions, so any multi-byte UTF-8
  character before a slice boundary panicked with "byte index N is not a char boundary" —
  `GET /<resource>?select=%C3%A9` (decodes to `é`) aborted the request task (H17). Separately,
  a local `let mut depth = 1` inside the embedded-resource branch shadowed the recursion-depth
  parameter, so the recursive call always received `1` and the `MAX_PARSE_DEPTH` guard never
  fired; `?select=a(a(a(…)))` recursed without bound and a deep value overflowed the worker
  stack, aborting the **whole process** (SIGSEGV) (H18). The parser now translates character
  positions to byte offsets before slicing (no desync at any site) and propagates the true
  recursion depth so the nesting guard rejects over-deep input. A proptest asserts the parser
  returns a `Result` — never panics — over arbitrary UTF-8.
- **Error/log/audit paths no longer panic when truncating user-controlled text (H20).**
  Six display paths truncated strings with a fixed byte offset (`&s[..N]`), which panics when
  the cut lands inside a multi-byte UTF-8 character. The live ones were the query-timeout
  handler (`format!("{}...", &query[..100])`, duplicated across the anonymous and
  authenticated executors) — an attacker sends a slow query with a multi-byte char at byte
  99–100 so the timeout handler *itself* panics — and the syslog audit-export path, where a
  caller could place a multi-byte char at byte 200 to abort (and so suppress) their own audit
  record. A new `utils::text::truncate_at_char_boundary` / `truncate_for_display` helper
  truncates on character boundaries; the copy-pasted timeout snippet and the SQL-logger,
  query-trace, and error-formatter truncations now route through it. The same sweep fixed two
  stragglers of the class: the API-key `Authorization: ApiKey …` prefix check (`raw_key[..7]`
  on an attacker-controlled header) now compares on bytes, and the SQL logger's 2000-byte cut
  is char-safe. No behavioural change for ASCII input.
- **Panicking PostgreSQL/Arrow/GCS code paths now fail loud (H34, H38, L-gcs-expect).** Three
  remotely- or environment-triggerable panics are converted to errors:
  - **PostgreSQL `data` column (H34).** `execute_raw`, `execute_raw_with_session`, and the relay
    pager extracted the JSONB `data` column with `Row::get`, which panics on SQL NULL or a
    non-JSONB type — a backing view projecting NULL `data` (e.g. via a LEFT JOIN) turned a query
    into a request-path panic. PostgreSQL was the only backend that aborted here; all three sites
    now go through a shared helper that returns `FraiseQLError::Database` (naming the column and a
    bounded slice of the query) for both the NULL and the type-mismatch case.
  - **Arrow Flight `limit = 0` (H38).** A client ticket with `limit = 0` produced `batch_size = 0`
    and `slice::chunks(0)` panics in the authenticated `do_get` handler. `execute_optimized_view`
    now rejects `limit = 0` fail-loud with `InvalidArgument`, the client-derived batch size is
    clamped to `[1, 10_000]`, and every chunk loop is routed through one helper that floors the
    size at 1 — so no call site (present or future) can pass a zero chunk size.
  - **GCS JWT clock (L-gcs-expect).** `create_gcs_jwt` used `.expect()` on
    `SystemTime::duration_since(UNIX_EPOCH)`; it now returns `FraiseQLError::File` instead of
    panicking if the system clock is before the UNIX epoch.
- **`GET /auth/v1/authorize` is now rate-limited per IP (H25, DoS).** `social_authorize` carried
  a `RateLimiters` field it never consulted, and the endpoint matched none of the path-based
  rate rules — so each request inserted a `CSRF` state into the bounded in-memory store, and a
  single IP at ~17 req/s could keep it full, making the store reject all new states (500) and
  denying social login for everyone. The handler now checks the shared `auth_start` limiter on
  the transport-peer IP before touching the store and returns 429 (with `Retry-After`) when
  exceeded.
- **Wire protocol caps single-message size (M-wire-msg-cap, memory-exhaustion DoS).**
  `decode_message` validated only a *lower* length bound, and DataRow column values carry no
  per-column cap — so a malicious/compromised peer (or a non-TLS MITM) could declare a length up
  to ~2 GiB and force the connection read buffer to grow that large before any per-field cap ran.
  A `MAX_MESSAGE_LEN` (256 MiB) bound is now checked right after the length is read (a fatal
  `InvalidData`, ahead of the incomplete-body path), and the connection read loop refuses to
  buffer past that bound (`WireError::Protocol`). The broader malformed-vs-incomplete decode-error
  distinction (H42) lands in the wire-protocol phase.
- **Relay `node(id:)` now enforces row-level authorization (H2, IDOR).** The global
  object lookup `node(id: …)` resolved any type by opaque id while applying none of the
  backing query's `requires_role` / RLS / `inject_params` gates, so a leaked node id
  returned the row with no access control — an authenticated low-privilege user could
  read role-gated types or other tenants' rows, and an anonymous caller could read any
  registered type. The node path now enforces all three gates for the resolved type and
  fails closed: an anonymous lookup of an RLS-/inject-/role-gated type returns "not
  found" (null) instead of the raw row, and an authenticated lookup ANDs the RLS /
  `inject_params` filter onto the id. Relay connection pagination carried the same
  latent fail-open — an RLS-configured deployment silently dropped the RLS filter for
  anonymous callers, leaking every row — now also fails closed. **Behavioral change:**
  in deployments that configure RLS, anonymous `node(id:)` and anonymous relay
  pagination of protected types now return nothing / error rather than leaking rows.
- **Federation `_entities` now fails closed for gated entity types (M-fed-entities-rls).**
  The `_entities` resolver resolved entities by `__typename` while applying none of the
  backing query's `requires_role` / RLS / `inject_params` gates, so an anonymous caller
  under an RLS-configured deployment, or any caller requesting a role-gated type, could
  resolve protected entities by id. The path now denies (403) when: row-level security is
  configured and the request is unauthenticated; a requested type's backing query
  declares `requires_role` the request does not hold; or a requested type is
  `inject_params`-scoped (tenant/owner) and the request is unauthenticated — denials run
  before any SQL. When the request **is** authenticated, `inject_params`-scoped types are now
  row-filtered at the resolver (see the next entry); an app-level `rls_policy` `WhereClause`
  remains under the federation *trusted-gateway* assumption. The existing field-level
  fail-closed guard (deny when the schema declares any policy-gated field) is retained.
  **Behavioral change:** anonymous `_entities` resolution of RLS-/inject-gated types, and any
  `_entities` resolution of role-gated types without the role, now error rather than returning
  the entity.
- **Federation `_entities` now applies per-row tenant/owner scoping to authenticated requests
  (M-fed-entities-rls follow-up, C1b/R1).** Closing the `_entities` per-row gap left by the
  fail-closed C1b gate: for an authenticated caller, the resolver no longer resolves
  `inject_params`-scoped entity types "under the trusted-gateway assumption" (i.e. with no
  per-row filter). The runtime now composes the backing query's `inject_params` (tenant/owner
  scoping) into a columnar predicate — `"tenant_id" = $N` — and ANDs it onto the key `IN`
  lookup, and threads the caller's session variables onto the resolver's connection so
  `current_setting()` DB-native row-level security is enforced (the federation counterpart of
  the #329 connection-affine RLS fix). A direct `_entities` hit with arbitrary ids is therefore
  scoped to the caller's tenant/owner instead of resolving every requested row. The predicate is
  built as a native-column equality (never a JSONB `data->>` path), so it composes onto the
  columnar entity table; an app-level `rls_policy` `WhereClause`, which targets the JSONB view
  shape, is **not** composable onto that table and remains a documented trusted-gateway
  limitation. **Behavioral change:** in a multi-tenant deployment, an authenticated `_entities`
  request now returns only the caller-scoped rows for `inject_params`-scoped types; a foreign
  tenant's id resolves to `null`.
- **Admin-plane endpoints now enforce mandatory auth + admin scope (H5, H6).** The OIDC
  middleware (`oidc_auth_middleware`) defers to the validator's global `required` flag,
  which governs only the anonymous data plane — so any deployment that allowed anonymous
  GraphQL silently un-authed the admin routers too (H5). The observer admin API was also
  authenticated but not authorized: any valid end-user token could read observer
  `actions[].headers` (webhook bearer secrets) and drive DLQ retry-all / delete / observer
  mutation (H6). Two net-new middlewares fix this independently of the global flag:
  `admin_auth_middleware` (valid token **and** `fraiseql:admin` scope) now gates the
  observer admin API and the design-audit API; `required_auth_middleware` (valid token,
  any scope) now gates the introspection, schema-export, and schema-metadata endpoints so
  that "require auth" actually rejects anonymous callers. Endpoints already configured
  with `*_require_auth = false` keep their explicit open-mount behavior. As defense in
  depth (R8), observer read/write responses now redact webhook secret values in
  `actions[].headers` (`[REDACTED]`) so secrets never travel in a response body.
- **Storage object overwrites now require ownership (H9, B4 — overwrite IDOR).** The
  upload path checked only bucket-level write permission (`can_write`, satisfied by any
  authenticated user), never the existing object's owner — so user B could clobber user
  A's object data by writing to its key (`metadata::upsert` preserved A's `owner_id` on
  conflict, but the bytes were overwritten). Both write doors are affected: `PUT
  /storage/v1/object/{bucket}/{key}` (H9) and `POST /storage/v1/presign/{bucket}/{key}`
  with `operation=upload` (B4 — a presigned PUT that overwrites a foreign object). Both
  now load any existing object and gate on a new `can_write_object` check: creating a new
  object still needs only authentication, but overwriting an existing one requires owner
  match or the admin role (mirroring `can_delete`). A non-owner overwrite returns `403`;
  anonymous callers always return `401` (no object-existence oracle). **Behavioral
  change:** uploads that overwrite an object owned by another user now fail instead of
  silently replacing its contents.
- **Arrow Flight `BulkExport` is now fail-closed behind a table allow-list (H39).** The
  Flight `BulkExport` ticket ran `SELECT * FROM "<table>"` for any client-supplied table
  with no allow-list and no per-user RLS filtering (the `SecurityContext` was only logged),
  so an authenticated Flight client could dump any table. `FraiseQLFlightService` now
  carries a `bulk_export_allowed_tables` allow-list (`None` by default = `BulkExport`
  disabled); `execute_bulk_export` returns `permission_denied` unless the requested table
  was explicitly opted in via the new `with_bulk_export_tables(...)` builder. The
  misleading documentation on `execute_optimized_view` (which claimed per-user RLS was
  applied) and `execute_bulk_export` is corrected to state plainly that these raw-SQL
  Flight paths apply **no** per-user RLS filtering and must be gated by configuration / the
  underlying view. **Behavioral change:** Arrow Flight `BulkExport` is disabled until an
  operator allow-lists specific tables.
- **Realtime broadcast endpoint now requires the admin plane (M-broadcast).** `POST
  /realtime/v1/broadcast` — which pushes an arbitrary event to every connected client — was
  mounted with no authentication whenever a broadcast manager was configured. It is now
  gated by `admin_auth_middleware` (valid token **and** `fraiseql:admin` scope), consistent
  with the design-audit API, and **fails closed**: with no OIDC validator configured to
  authenticate the admin plane, the endpoint is not mounted at all. **Behavioral change:**
  broadcasting now requires an admin-scoped token, and deployments without an OIDC validator
  no longer expose the broadcast endpoint.
- **Introspection now hides role-gated mutations (M-introspection-mut).** The introspection
  endpoint filtered role-gated *types* and *queries* out of its response (enumeration-hiding)
  but emitted the *mutations* list unfiltered, leaking the name and return type of every
  `requires_role` mutation to any caller — including anonymous ones. Mutations are now subject
  to the same `requires_role` filter, so a caller never sees a mutation it could not invoke.
- **Storage admin role decollided from the generic `"admin"` (M-storage-scope).** The storage
  RLS evaluator treated any role literally named `"admin"` as a full-access storage admin, and
  the server maps an OIDC token's `scopes` verbatim into a user's storage roles — so any token
  carrying an unrelated `admin` scope (a common scope name) silently gained read/overwrite/delete
  on every object in every bucket. The bypass role is now the explicit, storage-namespaced
  `fraiseql:storage:admin` (exported as `fraiseql_storage::STORAGE_ADMIN_ROLE`), and the static
  `storage_token` admin grant was updated in lockstep. **Behavioral change:** a generic `admin`
  role/scope no longer confers storage admin; grant the explicit `fraiseql:storage:admin` scope
  instead.
- **Legacy / unauthenticated storage mounts now fail closed (M-storage-legacy).** Two storage
  mount paths previously served an unauthenticated API: the legacy backend mount (which has *no*
  RLS evaluator) mounted with no auth layer when `storage_token` was unset — world-readable and
  world-writable — and the hardened RLS mount served an anonymous-only API when neither
  `storage_token` nor an OIDC validator was configured. Both now refuse to mount (logging a
  `SECURITY` error) unless an authentication mechanism is configured. **Behavioral change:** a
  storage deployment with no `storage_token` and no OIDC validator no longer exposes the storage
  routes at all.
- **Relay-enabled executors apply the same introspection filtering as non-relay ones
  (L-relay-inaccessible).** The relay constructor (`new_with_relay`) built its introspection
  responses without the federation `@inaccessible` field filter that the non-relay constructor
  applies, leaving the two paths free to diverge. Both constructors now build introspection
  through a single shared helper so a relay executor can never expose an `@inaccessible` field
  in `__type`/`__schema` that the non-relay path would hide (defense-in-depth).
- **Multi-tenant subscriptions fail closed on the tenant gate (M-tenant-ws-failopen).** The
  `WebSocket` subscription matcher only filtered events when *both* the subscription and the
  event carried a tenant id; a subscriber with no tenant id matched **every** tenant's events,
  and a tenant-scoped subscriber still received untagged events. In multi-tenant deployments
  (`security.multi_tenant = true`) the gate now requires both sides to carry the *same* tenant —
  a missing tenant on either side never matches. Single-tenant deployments keep the permissive
  behavior (tenant ids are typically absent), so they are unaffected. **Behavioral change:** in
  a multi-tenant deployment a subscription that does not resolve a tenant id now receives no
  events, and events without a tenant id are not delivered to tenant-scoped subscribers.
- **Suspended tenants are rejected on the subscription `WebSocket` path (M-tenant-ws-suspended).**
  Tenant suspension (`TenantStatus::Suspended`) returned 503 on the GraphQL data plane but was
  not consulted for subscriptions, so a suspended tenant could still open subscriptions and keep
  receiving events. The subscription path now consults the tenant registry through a new
  `TenantStatusSource`: a new subscription whose resolved tenant is suspended is rejected with a
  `TENANT_SUSPENDED` error, and event delivery to a connection whose tenant becomes suspended
  mid-stream is paused (re-checked per event).
- **Per-tenant concurrency quotas are now enforced (M-quotas).** `TenantQuota.max_concurrent`
  was configurable and a per-tenant concurrency semaphore existed, but the GraphQL request path
  never acquired a permit, so the limit was silently ignored. The handler now acquires a
  concurrency permit (held for the duration of the request) after resolving the tenant executor,
  for explicitly-keyed registered tenants; exceeding the limit returns HTTP 429 Too Many Requests
  (previously a tenant-dispatch `RateLimited` collapsed to 403). Requests with no explicit tenant
  key (the default executor) are unlimited, as before.
- **Per-tenant per-second rate limiting is now enforced (M-quotas, RPS follow-up).**
  `TenantQuota.max_requests_per_sec` was configurable but had no enforcement primitive and was
  silently ignored. Each tenant now carries a fixed one-second-window rate limiter (the audited
  `KeyedRateLimiter` from `fraiseql-auth`), and the GraphQL request path checks it at the same
  chokepoint as the concurrency permit — for explicitly-keyed registered tenants only. Exceeding
  the configured requests-per-second returns HTTP 429 Too Many Requests (reusing the C7
  `RateLimited` → 429 dispatch mapping); the default executor and tenants without a per-second
  quota are unaffected. Enforcement requires the default-on `auth` feature (which provides the
  limiter); a `--no-default-features` build parses `max_requests_per_sec` but logs a warning at
  registration that it is not enforced. The limiter is per-process, so an *N*-replica deployment
  enforces *N* × the configured rate — configure a distributed backend for true global limiting.
- **MySQL stored-procedure mutation path is now parameterized (C1, critical).**
  `CALL` statements on the MySQL backend bound arguments by inline string-escaping
  that doubled single quotes only and left backslashes untouched; under MySQL's
  default SQL mode a GraphQL mutation argument like `\', …; -- ` could break out of
  the string literal and execute injected SQL (the driver negotiates
  `MULTI_STATEMENTS`). Both call paths (`execute_function_call` and the Change-Spine
  outbox variant) now bind arguments as prepared-statement parameters
  (`CALL fn(?, …)`) and the inline escaper is removed. Affects every published
  release with the MySQL backend.
- **Webhook `body_template` values are JSON-escaped (H11).** Observer webhook bodies
  were built by substituting entity-field values into a string template and
  re-parsing the result, so an attacker-controlled string field (a username,
  comment, …) could break out of its JSON string and inject or override keys in the
  HMAC-signed (`X-FraiseQL-Signature-256`) payload. String values are now
  JSON-escaped into their surrounding string context; typed (number/bool) slots and
  plain-text bodies are preserved. The Slack and email paths were already safe.
- **Aggregation, federation, full-text, and relay SQL paths hardened against
  injection (H1, H3, H41, and latent M-/L- sites).**
  - GROUP BY dimension aliases — echoed verbatim from GraphQL variable JSON keys
    into the SELECT list — are validated as `[_A-Za-z][_0-9A-Za-z]*` at parse time,
    independent of the compile-time dimension allowlist (H1).
  - Federation `_entities` resolution binds key-field values as dialect-native
    parameters instead of single-quote-escaping them (unsafe on MySQL), validates
    key/field identifiers, and never selects `@inaccessible` / `@external` fields
    (H3, M-fed-select-list); the federation `escape_sql_string` helper is removed.
  - Full-text search `language` (regconfig) is validated against `[a-z_]+` in
    `WhereOperator::validate()` before it reaches `plainto_tsquery` in the published
    `fraiseql-wire` crate (H41).
  - The SQL Server relay ORDER BY builders validate order-by field names before
    interpolating them into `JSON_VALUE` paths (M-relay-orderby), and the row-view
    DDL codegen skips field names that are not safe identifiers (L-row-views).
- **Removed a dead, dialect-incomplete tenant-filter helper.** The unused
  `TenantEnforcer::enforce_tenant_scope_sql` (string-concatenation tenant filter
  with incomplete escaping) was deleted; the parameterized AST-based
  `enforce_tenant_scope` is the supported path (L-tenant-enforcer).
- **Advisory-gate hardening (Phase 02).** `make audit` / `make security` no longer
  fail on a clean tree: `.cargo/audit.toml` and `deny.toml` ignore lists are now
  kept in lockstep by `tools/check-audit-lockstep.sh` (wired into both targets and
  the Dagger ShellGates). A new `tools/check-deadlines.sh` fails the build once an
  accepted-advisory deadline in `deny.toml` lapses, and the Dagger security leg now
  runs `cargo audit` alongside `cargo deny`. The rustls-webpki advisories
  (RUSTSEC-2026-0098/0099/0104, behind the opt-in `aws-s3` feature) had their
  acceptance deadline extended to 2026-09-01: a spike confirmed no aws-config
  feature selects rustls 0.23 over the legacy rustls-0.21 connector, so the
  migration is tracked as Phase 12 (aws-stack bump).
- **Token revocation is now enforced on every request, and revoke-all actually
  revokes (H8, M-revoke-all).** Revocation was write-only: `POST /auth/revoke[-all]`
  recorded revoked tokens, but the OIDC auth middleware validated the JWT and decoded
  its `jti` **without ever consulting the revocation store**, so a revoked token kept
  working until its natural `exp` — logout, compromise response, and admin force-logout
  were silent no-ops (H8). The middleware now checks the revocation store after token
  validation on every authenticated route (data plane *and* admin plane) and rejects
  with 401. Separately, `revoke-all` was inert across all three backends: `revoke`
  records no `sub`, so the old `revoke_all_for_user` (a `sub`-keyed delete on
  in-memory/Postgres, a phantom-namespace `SCAN` on Redis) always affected 0 rows
  (M-revoke-all). `revoke-all` now records a per-user *epoch* and the request path
  rejects any of that user's tokens whose `iat` is at or before it — catching tokens
  that were never individually revoked (and tokens with no `jti`). New
  `[security.token_revocation] revoke_all_ttl_secs` (default 86400) bounds epoch
  retention; set it above your maximum access-token lifetime. The HS256 auth path is
  unaffected (revocation routes mount only with an OIDC validator).
  **Breaking / behavioral change:** enabling `[security.token_revocation]` now actually
  enforces it — with `require_jti = true` (default) a validated token that lacks a `jti`
  claim is rejected 401 post-validation; set `require_jti = false` to admit jti-less
  tokens (losing per-token revocation, keeping the revoke-all epoch). The
  `POST /auth/revoke-all` response body changed from `{ "revoked_count": N }` to
  `{ "revoked": true }` (the epoch design has no per-token count).
- **REST error responses no longer leak raw database error text (H7).** With error
  sanitization enabled, GraphQL stripped internal detail from `DatabaseError` /
  `InternalServerError` responses, but the REST surface had **zero** sanitization: a
  server fault (undefined function `42883`, `XX000`, a connection error, …) rendered
  `FraiseQLError`'s raw message — schema names, constraint details, SQL fragments —
  verbatim into the `{"error":{"message":…}}` body. (The dedicated sanitization
  middleware meant to cover this was orphaned: never declared in `mod.rs`, never
  layered, and its body-shape matcher did not even recognise the nested REST error
  shape.) REST now applies the **same** sanitization gate as GraphQL at its
  error-rendering site: when `[security.error_sanitization]` is enabled, 5xx bodies
  carry the generic `custom_error_message` (default `"An internal error occurred"`)
  and the raw detail is logged server-side instead. Client-facing 4xx messages —
  validation, auth, not-found, and SQLSTATE 22/23 client-input faults (#413) — are
  intentional and pass through unchanged. The orphaned middleware module was deleted
  (two sanitization layers with divergent body-shape assumptions invite drift).
- **The server now refuses to boot when a field is marked for at-rest encryption,
  instead of silently storing it in plaintext (H12).** Field-level at-rest encryption
  was advertised but never worked end-to-end: the write/mutation path does not encrypt
  (`FieldEncryptionService::encrypt_variables` has no caller), so a field marked
  `encryption` was written to the database in **plaintext** and the read path then
  failed to decrypt it, returning HTTP 500 on every read — and when the `secrets`
  feature was absent the field round-tripped silently in plaintext, so operators
  believed sensitive columns were encrypted at rest when they were not. Rather than
  ship a security control that silently does the opposite of what it claims, the server
  now performs a startup check and **refuses to start** when any compiled-schema field
  declares `encryption`, naming the offending field(s) and how to remove the marker.
  The false "transparently encrypted… decrypted when read back" claims on
  `FieldDefinition.encryption` / `FieldEncryptionConfig` and in the `fraiseql-secrets`
  README were corrected. End-to-end field encryption (write-path call, array/nested
  recursion, `(type, field)` keying, ciphertext versioning, key KDF/zeroize) remains
  unimplemented and is tracked for a future release.
  **Breaking change:** a deployment whose compiled schema marks any field for
  encryption will now fail to start (it was previously 500-ing on every read of that
  field, or silently storing plaintext); remove the `encryption` marker and any
  `[security.field_encryption]` config to boot.
- **Removed dead field-encryption audit logging and its false compliance claims (H13).**
  `fraiseql-secrets` advertised "audit logging — track all secret access for compliance
  (HIPAA/PCI-DSS/GDPR/SOC 2)," but `AuditLogger` was an in-memory `Vec` commented "for
  testing" with no persistence or tracing sink, invoked from nowhere — it audited
  field-encryption operations that, after the H12 fix, cannot occur at all. The dead
  module was deleted and the false at-rest-encryption / audit-logging claims were excised
  from the `fraiseql-secrets` crate docs and README. (This does **not** affect the
  separate, genuinely-wired server/auth audit system configured via
  `[security.audit_logging]`, which continues to record mutations and admin operations.)
- **Security response headers are now sent on every response (M-sec-headers).** The
  `security_headers_middleware` (`X-Content-Type-Options: nosniff`, `X-Frame-Options:
  DENY`, `Strict-Transport-Security`, `Referrer-Policy`, `Content-Security-Policy`,
  `X-XSS-Protection: 0`) existed but was never layered, so none of these headers were
  emitted. It is now applied globally in `apply_middleware`. The headers are set
  *if-absent* so a handler can opt into its own policy — the GraphQL playground sets a
  relaxed CSP for its CDN-loaded IDE assets, which the global strict CSP no longer clobbers.
- **Mutations over HTTP GET are now rejected with 405 (M-get-mutations).** A mutation
  sent via `GET /graphql` was executed with only a log warning, sidestepping the POST-only
  CSRF posture; detection was also an unreliable `mutation` string-prefix match. The GET
  handler now parses the operation and returns **405 Method Not Allowed** for mutations,
  per the GraphQL-over-HTTP spec (queries over GET are unaffected). **Behavioral change:**
  clients that (incorrectly) sent mutations over GET now receive 405 instead of a result.
- **The auth brute-force limiter no longer trusts `X-Forwarded-For` (M-xff-limiter).** When
  `ConnectInfo` was unavailable (some library embeddings), the per-IP failed-auth limiter
  fell back to keying on the attacker-controlled `X-Forwarded-For` header, letting a caller
  rotate it to mint a fresh failure budget per value. The XFF fallback was removed: the
  limiter keys only on the validated transport peer, and when that is absent all callers
  share one bucket (fail-closed, not bypassable). The shipped binary always supplies
  `ConnectInfo`, so its behaviour is unchanged.
- **The server refuses to boot under an enabled server-side `[tls]` config instead of
  serving plaintext while claiming TLS (M-tls-enforce).** FraiseQL does not terminate TLS
  itself — it serves plaintext HTTP and expects a reverse proxy / load balancer / service
  mesh in front. The `[tls]` section was parsed and validated, a rustls `ServerConfig` was
  built from it and then **silently discarded**, the listener kept serving plaintext, and
  startup logged `mtls_required = true` — a server that claimed mutual TLS while doing no
  certificate check at all. The server now **refuses to start** when `[tls].enabled` is set,
  with a message directing operators to terminate TLS at a proxy (or remove `[tls]`). The
  dead server-side TLS plumbing (`TlsEnforcer`, `create_rustls_config`, certificate/key
  loaders) was removed; **database** connection TLS (`[database_tls]`:
  `postgres_ssl_mode`, `redis_ssl`, …) is fully retained.
  **Breaking change:** a deployment that set `[tls]` expecting the server to terminate TLS
  will now fail to start (it never actually terminated TLS — it served plaintext);
  terminate TLS in front of the server and remove `[tls]`.
- **Patched Postgres-protocol denial-of-service advisories.** Bumped `tokio-postgres`
  0.7.17 → 0.7.18 and `postgres-protocol` 0.6.11 → 0.6.12 (semver-compatible) to pick up
  fixes for RUSTSEC-2026-0178 (unbounded SCRAM iteration count → CPU-exhaustion DoS),
  RUSTSEC-2026-0179 (panic decoding a malformed `hstore` value), and RUSTSEC-2026-0180
  (panic on a `DataRow` with fewer fields than columns). Also dropped the now-stale
  `RUSTSEC-2026-0002` (lru) ignore from `deny.toml` / `.cargo/audit.toml`, which no longer
  matches any crate in the tree.

### Added

- **External-write capture for subscriptions (#366).** Uncooperative external
  writes — a raw `INSERT`/`UPDATE`/`DELETE` from psql, a migration, or a
  third-party tool — now reach GraphQL subscribers, without double-emitting for
  writes that already flow through FraiseQL's mutation executor. The executor sets
  a transaction-local marker (`fraiseql.cdc_mediated = 'on'`) at the start of every
  mutation transaction; a shipped, suppressible fallback trigger
  (`core.fn_entity_change_log_capture`) writes a contract-conforming
  `core.tb_entity_change_log` row only when that marker is absent — so an app-path
  write keeps its rich in-transaction outbox row and the trigger no-ops, while an
  external write is captured with a Debezium-style `{op, before, after}` envelope
  and fans out through the existing change-log reader and NATS bridges. The
  triggers are statement-level with transition tables, so a bulk statement captures
  all its rows in a single set-based INSERT (one event per changed row) rather than
  firing per row. Declare which tables feed a type with
  `@fraiseql.type(subscribable_tables=["tb_post"])`; the new
  `fraiseql generate-capture-triggers -s schema.compiled.json | psql "$DATABASE_URL"`
  command emits the self-contained, idempotent install DDL. No new infrastructure:
  plain triggers, no `wal_level=logical`, no replication slots — works on any
  managed PostgreSQL. See `docs/architecture/external-write-capture.md`.
- **Actor model on the Change-Spine envelope (#390).** Every audited operation now
  carries a first-class actor classification — `human_user`, `service_account`,
  `ai_agent`, or `system_job` — derived onto the `SecurityContext` at
  authentication and stamped into the change-log `actor_type` column by the
  in-transaction outbox write. For a delegated agent request (RFC 8693 `act`
  claim), the change-log `acting_for` column records the underlying human's
  public-facing UUID. The tenant lifecycle audit log (`TenantEvent`) gains the same
  `actor_type` / `acting_for_user_id` fields, now populated from the request
  principal at every tenant-admin endpoint (previously the actor was always NULL).
  API-key requests classify as `service_account`. The classification is recorded
  for forensics, not consumed as an authorization input.
- **Change-log reader surfaces the full Change-Spine envelope (#390 follow-up).**
  The observer change-log reader now projects the `actor_type` and `acting_for`
  columns onto `ChangeLogEntry` and the emitted `EntityEvent`, so out-of-session
  consumers (the NATS bridges, CDC fan-out, DLQ handlers) receive the actor
  classification and delegated-human UUID — not just the in-process listener. The
  PostgreSQL, MySQL, and SQL Server NATS bridges are brought to full envelope
  parity in the same pass: they now also carry `tenant_id`, `duration_ms`, and
  `seq` (previously only `user_id` survived the bridge). `EntityEvent`'s envelope
  fields gained `#[serde(default)]` so a consumer can decode an event serialized
  before these fields existed (forward/backward wire tolerance over NATS).
- **Change-log reader surfaces `schema_version` (#377 follow-up).** The observer
  change-log reader now projects the Change-Spine `schema_version` envelope column
  onto `ChangeLogEntry` and the emitted `EntityEvent`, and all three NATS bridges
  (PostgreSQL / MySQL / SQL Server) carry it across the bridge — so out-of-session
  consumers can audit which producer schema version wrote a change (e.g. "which
  schema produced this dead-lettered action"; see
  `docs/operations/zero-downtime-deploys.md`). The listener's row decode was
  converted from a positional tuple to a named `sqlx::FromRow` struct, removing the
  16-column tuple ceiling it had reached and the positional fragility. The field is
  `#[serde(default)]` for NATS wire tolerance.

### Changed

- **BREAKING (change-log contract):** the `acting_for` column is retyped
  `BIGINT → UUID` across the PostgreSQL / MySQL / SQL Server contract DDL to hold
  the delegated human's public-facing UUID (mirroring `tenant_id`). The column
  shipped NULL-by-design in v2.6.0 with no producer, so the migration's guarded
  retype is lossless; re-run migration `08` (and the `09`/`10` variants) to adopt
  it. `doctor --against-db` reports the type drift until a database is re-migrated.

### Fixed

- **gRPC mutations always reported failure.** The gRPC mutation handler read a
  non-existent `status == "success"` column from the `mutation_response` row instead of
  the canonical `succeeded` boolean (see `core::runtime::mutation_result`), so every gRPC
  mutation returned `success = false` regardless of the actual outcome. It now reads
  `succeeded`.
- **REST 204 No Content responses carried a `{}` body.** The REST response renderer wrote
  `{}` for an absent body (`None.unwrap_or(json!({}))`), giving 204 No Content (e.g. a
  `DELETE`) a 2-byte body in violation of the HTTP spec. A `None` body now emits an empty
  body.
- **REST error responses dropped structured `details`.** The REST error renderer wrote
  only `code` + `message`, discarding `RestError.details` — so a 422 validation failure's
  `missing_fields`, and any other structured error detail, never reached the client.
  Errors now render via `RestError::to_json`, preserving `details` (internal-error details
  are still stripped when error sanitization is enabled).

### Documentation

- **Zero-downtime deploy guide (#378).** New `docs/operations/zero-downtime-deploys.md`
  documents rolling, blue-green, and canary deploys behind a load balancer, the
  expand/contract migration discipline, and the in-process primitives FraiseQL already
  provides: in-place atomic schema reload (`SIGUSR1` / `POST /api/v1/admin/reload-schema`),
  the graceful shutdown drain, the `schema_format_version` boot guard, and schema-decoupled
  observer DLQ retry. Establishes that deploy-time version coherence belongs in the deploy/LB
  layer (with [fraisier](https://github.com/fraiseql/fraisier) as the worked example), not in
  per-request dual-schema routing inside the server. Corrects two stale claims in
  `compiled-schema-lifecycle.md` (it asserted "no hot reload" and a non-existent
  `fraiseql_version` major/minor guard).

### Added

- **Change Spine: the mutation executor writes the `core.tb_entity_change_log` outbox row
  in-transaction.** Every successful, state-changing mutation now records exactly one
  change-log row **inside the mutation function's own transaction, on the same connection** —
  a transactional outbox, the first runtime step of the Change Spine. The write is a single
  statement: the function call is wrapped in a `MATERIALIZED` CTE (so a volatile mutation
  function runs exactly once) whose data-modifying CTE INSERTs the row and whose primary query
  returns the function's row unchanged to the caller — no extra connection acquire, atomic with
  the mutation (a crash leaves neither the change nor the log row). The row carries the
  changed-entity columns straight off the `app.mutation_response` row (`object_id`,
  `object_data`, `updated_fields`, `cascade`), the DML verb in `modification_type` (`INSERT` /
  `UPDATE` / `DELETE` / `CUSTOM`, from the mutation's `operation`), `object_type` (the entity
  type, falling back to the GraphQL return type), and a wall-clock `duration_ms` computed on
  the DB clock from the txn-local `fraiseql.started_at` and stamped with
  `extra_metadata.duration_calc_version = 2`. The executor also stamps `tenant_id` (the UUID
  tenant from the request's `SecurityContext` — left NULL for a non-UUID tenant, never aborting
  the mutation) and `commit_time`, while `seq` comes from the table's global sequence default;
  it also stamps the envelope `trace_id` + `trace_context` (#375) and `schema_version` (#377) — see
  the dedicated entries below. `actor_type` / `acting_for` ship as columns but stay NULL pending
  #390. Only an effective change (`succeeded AND state_changed`) is logged — no-ops
  and business-logic failures do not produce a spine event. Implemented for PostgreSQL, MySQL,
  and SQL Server (see the multi-DB outbox-wiring entry below). **Opt-out (default-on):** the write can be
  disabled globally — `[changelog] write_enabled = false` in `fraiseql.toml`, or
  `FRAISEQL_CHANGELOG_ENABLED=false` at runtime — and per endpoint via the compiled-schema
  `MutationDefinition.changelog` flag (serde-defaults to `true`), authored as
  `@fraiseql.mutation(changelog=False)` (Python) or `@Mutation({ changelog: false })`
  (TypeScript). A row is written only when the global switch and the per-mutation flag
  are both on. The contract is documented in `docs/architecture/change-log-contract.md`.

- **Prepared-statement caching on the mutation function-call path — large mutation-throughput
  win.** The PostgreSQL adapter now uses deadpool's per-connection `prepare_cached` for
  `execute_function_call` and its session-affine / change-log variants, so PostgreSQL parses
  and plans each mutation's statement **once per connection** instead of re-parsing it on every
  call. In a 40-worker concurrent benchmark this lifted baseline mutation throughput by roughly
  **+60%** (≈20k→33k RPS on the test box). It is also what makes the in-transaction change-log
  outbox above effectively free: the outbox CTE's ~33% apparent cost was almost entirely
  repeated parse/plan, not the durable write — with caching the outbox penalty collapses to
  within noise on a PK-only table (the residual on the fully-indexed contract table is
  secondary-index maintenance, a write-vs-read tradeoff in the index strategy).

- **Change Spine: multi-DB outbox portability + reader reconcile.** A portable,
  fully-parameterized outbox INSERT builder (`fraiseql_db::changelog::build_changelog_insert_sql`
  over `CHANGELOG_PORTABLE_INSERT_COLUMNS`) emits the contract shape for PostgreSQL / MySQL /
  SQLite / SQL Server, and the contract migration now ships MySQL (`09_*`) and SQL Server
  (`10_*`) DDL variants — so cooperative external producers (and the non-PostgreSQL adapters,
  now wired — see below) write the same shape. The change-log poller's row decoder is reconciled
  to the Trinity column types (`fk_* = BIGINT`, public id = `UUID`, nullable `object_data`); its
  public string-based API is unchanged.

- **Change Spine: live MySQL and SQL Server in-transaction outbox.** The MySQL (sqlx) and SQL
  Server (tiberius) adapters now write the `tb_entity_change_log` outbox row themselves, atomic
  with the mutation — the multi-DB counterpart of PostgreSQL's in-txn CTE. Since neither dialect
  can reference a `CALL`/`EXEC` result set in a following `INSERT … SELECT`, each opens a
  transaction, parses the `mutation_response` row in Rust, and INSERTs the outbox row on the same
  connection before commit (a raised procedure or a failed INSERT rolls back both). `duration_ms`
  / `started_at` are legitimately NULL on these dialects (no request-scoped DB clock); `seq` fires
  from the table default. Wiring against live MySQL 8.3 and SQL Server 2022 surfaced and fixed
  three latent bugs: the MySQL `09_*` DDL gave `id CHAR(36)` no default (the portable INSERT omits
  `id`, like PG/MSSQL); both the `09_*`/`10_*` DDL and the portable INSERT builder emitted the
  reserved word `cascade` unquoted (a syntax error on MySQL and SQL Server) — the builder now
  quotes column identifiers per dialect; and the MySQL `CALL` runs over sqlx's binary protocol
  (the text-protocol `raw_sql` cannot form a `Send` future over `&mut MySqlConnection`), reading
  its result columns by ordinal. SQLite (read-only) and mock adapters keep the no-op default.

- **`fraiseql doctor --against-db` — change-log contract drift check (#380).** Reports drift
  between a live `core.tb_entity_change_log` and the shipped contract: missing columns the
  additive migration will add (warning), app-specific extra columns it leaves untouched
  (warning), and — the one drift it *cannot* reconcile — a pre-existing column with the wrong
  type (failure), e.g. a legacy `object_id text` the contract wants as `uuid` (`ADD COLUMN IF
  NOT EXISTS` no-ops on an existing column and cannot retype it). The expected column set is
  sourced from the single typed contract definition shared with the migration DDL
  (`fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT`). Runs alongside the #409
  PL/pgSQL body-resolution pass under the same `--against-db` flag.

- **Authoring-SDK surface for the per-mutation change-log opt-out.** The Change-Spine
  per-mutation flag can now be set from the authoring decorators —
  `@fraiseql.mutation(changelog=False)` in the Python SDK and
  `@Mutation({ changelog: false })` (or the typed `MutationConfig.changelog`) in the
  TypeScript SDK — instead of hand-editing the compiled schema. Both decorators validate
  the value is a boolean and fail fast at authoring time on anything else, and emit the
  `changelog` key only when it is set, so a schema authored without it keeps logging (the
  compiler serde-defaults `MutationDefinition.changelog` to `true`).

- **Change Spine: the change-log poller surfaces the envelope/perf columns on the observer
  event path.** `fraiseql_observers`'s `ChangeLogListener` now projects three more contract
  columns top-level — `tenant_id` (the public-facing UUID partition stamp), `duration_ms`, and
  `seq` (the monotonic Change-Spine sequence) — onto `ChangeLogEntry`, and carries
  `duration_ms` / `seq` through to the `EntityEvent` it emits. NATS subscribers, the deduped
  executor's `TenantScope`, and the search / Arrow sinks now see the perf and ordering metadata,
  not just the GraphQL `data` JSONB. (The `core.v_entity_change_log` read view already exposed
  these for the #149 GraphQL / #392 perf path; this closes the gap on the Rust event path.) All
  three are contract-nullable and decode as `None` for cooperative external producers that do not
  stamp them.
- **`fraiseql perf` — change-log performance observability (#392).** The first Change-Spine
  consumer. A new CLI command group reads the framework-owned change-log
  (`core.v_entity_change_log`) and turns it into operator forensics. `perf regression-scan`
  flags mutations whose p50 latency regressed between a baseline and a recent window, per
  `(object_type, modification_type)` — never aggregating across modification types (a shift in
  the operation mix can otherwise mask a regression as a false improvement) and comparing only
  rows carrying the current `duration_calc_version` (pre-fix `EXTRACT(MILLISECONDS)` rows are
  excluded, not mixed). `perf explore slowest | null-rate | summary` are ad-hoc reads of the
  slowest mutations, `duration_ms` completeness, and per-operation percentiles. The scan exits 0
  even when it finds regressions (a report, not a gate; `--fail-on-regression` opts into exit 1);
  `--json` emits a stable `findings`/`skipped`/`summary` shape and the human report prints
  greppable `WARN` / `SKIP` lines — the seam the `fraisier` orchestrator schedules against.
  PostgreSQL-only.
- **Change Spine: the change-log `trace_id` is now populated from the request trace (#375).**
  The mutation executor stamps the originating request's W3C trace id — parsed from the inbound
  `traceparent` header onto the `SecurityContext` — into the change-log `trace_id` column, on every
  dialect (it is a plain text column, unlike the PostgreSQL-only `duration_ms`). A change-log row now
  links back to its distributed trace, and the #392 `perf explore slowest` / regression findings
  surface it as the investigation handle. `trace_id` is `NULL` for a request with no trace context
  (e.g. an anonymous mutation, which carries no `SecurityContext`) — a best-effort stamp that never
  aborts the mutation, consistent with `tenant_id`. The full W3C `trace_context` JSONB is also now
  populated — see the dedicated entry below; #375 is fully landed.

- **Change Spine: the change-log `schema_version` is now populated from the compiled schema (#377).**
  The mutation executor stamps the compiled schema's content hash
  (`CompiledSchema::content_hash()`) into the change-log `schema_version` column, on every dialect
  (a plain text column, like `trace_id`). Unlike `trace_id` / `tenant_id`, this is **not** a request
  value but a per-deployment constant — the same hash on every row a given deployment writes — so it
  is computed **once** at executor construction and cached on the `ExecutorContext` rather than
  recomputed per mutation. It is the same content hash that already keys the query cache, the
  `/health` schema digest, and hot-reload diffing, so it changes on any schema change. A change-log
  row now records which deployment produced it, the correctness handle that unblocks #378
  (zero-downtime deploys / DLQ replay: reject a row replayed under a different schema rather than
  corrupt data). `schema_version` is `NULL` only for producers with no compiled schema in scope —
  cooperative external producers (ETL) and the non-PostgreSQL no-op path.

- **Change Spine: the change-log `trace_context` JSONB is now populated — #375 fully closed.**
  Beyond the scalar `trace_id`, the mutation executor now stamps the **full W3C trace context** into
  the `trace_context` JSONB column: the parsed `traceparent`
  (`{version, trace_id, parent_id, trace_flags}`, hex lower-cased) plus the `tracestate` header when
  present. A change-log row therefore carries enough to **re-propagate / reconstruct** the
  distributed trace, not merely link to it. The context is parsed feature-independently from the
  request headers onto the `SecurityContext` (alongside `trace_id`) and written on every dialect —
  JSONB on PostgreSQL, JSON on MySQL, `NVARCHAR(MAX)` on SQL Server. It is `NULL` for a request with
  no well-formed `traceparent` (same gate as `trace_id`), never aborting the mutation. With this, the
  only envelope columns still NULL-by-design are `actor_type` / `acting_for` (#390).

### Breaking

- **The observer admin API and design-audit API now require the `fraiseql:admin`
  scope; introspection / schema-export / schema-metadata now require a valid token
  whenever their `*_require_auth` flag is set.** Previously these admin-plane routes
  were authenticated only by the global OIDC middleware, which let anonymous callers
  through whenever the data plane allowed anonymous queries, and the observer API
  performed no scope check at all. Callers of the observer admin API
  (`/api/observers/*`) and design-audit API (`/api/v1/design/*`) must now present a
  JWT carrying the `fraiseql:admin` scope; tokens without it receive `403`. Tooling
  that reads introspection / schema export / metadata must present a valid token (any
  scope) when those endpoints are configured to require auth. Routes left at
  `*_require_auth = false` are unchanged.

- **Broadcast and storage subsystems now refuse to run unauthenticated (Phase 03 C6).**
  Three privileged surfaces that previously mounted (or admitted callers) without
  authentication now fail closed:
  - `POST /realtime/v1/broadcast` requires a `fraiseql:admin`-scoped token, and is not
    mounted at all unless an OIDC validator is configured (M-broadcast).
  - The legacy storage backend (no RLS) is not mounted unless `storage_token` is set, and
    the hardened storage API is not mounted unless `storage_token` or an OIDC validator is
    configured (M-storage-legacy).
  - The storage admin role is now `fraiseql:storage:admin`, not the generic `"admin"`; OIDC
    callers needing storage-admin must carry the explicit scope (M-storage-scope).
  Deployments relying on anonymous broadcast or anonymous/`admin`-scoped storage must add the
  appropriate auth configuration.

- **Multi-tenant subscription delivery and per-tenant concurrency are now strict (Phase 03 C7).**
  In `security.multi_tenant = true` deployments the subscription tenant gate fails closed: a
  subscription that resolves no tenant id receives no events, and untagged events are not
  delivered to tenant-scoped subscribers (M-tenant-ws-failopen). Suspended tenants can no longer
  open subscriptions or receive further events (M-tenant-ws-suspended). A configured
  `TenantQuota.max_concurrent` is now actually enforced on the GraphQL path and returns 429 when
  exceeded (M-quotas) — previously it was ignored. Single-tenant deployments and tenants without a
  concurrency quota are unaffected.

- **The framework now owns the `core.tb_entity_change_log` write — remove app-side
  hand-rolled inserts.** Before, FraiseQL apps populated the change log themselves, typically
  with a per-mutation-function `INSERT INTO core.tb_entity_change_log …`. The mutation
  executor now writes that row itself, in-transaction, for every successful state-changing
  mutation (see Added, above). **On upgrade, delete the hand-rolled inserts from your mutation
  functions** — otherwise each mutation logs the row twice (one app row + one framework row).
  There is no opt-out flag and no `ON CONFLICT` cutover guard: owning the write *is* the
  feature, and the duplicate-write window closes as soon as the app-side insert is removed.
  External *cooperative* producers (ETL / jobs / sister services writing
  contract-conforming rows directly into the table) remain first-class and are unaffected —
  that is a distinct, supported pattern, not the app double-writing its own mutation output.

- **The observer `EntityEvent.tenant_id` is now the UUID `tenant_id`, not `fk_customer_org`;
  `EntityEvent` also gains `duration_ms` / `seq` (wire-format change).** The change-log poller
  previously copied the internal `fk_customer_org` BIGINT (as a decimal string) into
  `EntityEvent.tenant_id`, collapsing the Trinity pair — so tenant isolation that keys off it
  (the NATS subscription tenant filter, the deduped executor's `TenantScope`) matched on an
  integer that never equals the JWT/RLS tenant. The poller now surfaces the contract's
  public-facing `tenant_id` UUID instead, and `None` when it is NULL (no more `fk_customer_org`
  fallback). **If you filter observer events by tenant, switch your configured tenant
  identifiers from the `fk_customer_org` integer to the UUID `tenant_id`.** Separately,
  `EntityEvent` now serializes two new fields — `duration_ms` and `seq` — with no serde
  default, so a consumer deserializing an `EntityEvent` produced by an older build (e.g. a
  message already resident in a durable NATS stream across a rolling upgrade) must be upgraded
  in lockstep; the change-log table is the source of truth and events are re-derivable, so
  drain the stream or accept the brief gap rather than mixing versions.

### Fixed

- **`fraiseql-server` now compiles with `--features rest,arrow` (unbreaks the
  `server-full` image).** The `#[cfg(feature = "arrow")]` server path builds a
  `Server<PostgresAdapter>` (the Arrow Flight constructor keeps the raw adapter), but the
  multi-tenant runtime wiring (#330) built the per-tenant executor factory only for the
  *cached* adapter type, so `with_tenant_executor_factory` failed to type-check (`E0308`)
  on the arrow path. The factory is now built per build with the adapter type that matches
  the server it is installed on — `PostgresAdapter` for the arrow path,
  `CachedDatabaseAdapter<PostgresAdapter>` otherwise. This was the one feature combination
  no CI leg compiled (preflight runs `--all-features`, which enables `wire-backend` and
  takes a different `cfg` branch), so it had been broken since #330 landed and left the
  `fraiseql-server-full` Docker image — the sole artifact that builds `rest + arrow` —
  stale at `2.4.0`; it ships again from the next release. A `server-rest-arrow`
  feature-matrix combo now guards the build, and the pre-existing arrow-path lint/doctest
  debt the combo surfaced has been cleared.

## [2.5.0] - 2026-06-08

### Security

- **Operation-level authorization — pluggable `Authorizer` (#422).** v2 had only a
  *static* per-operation gate (`requires_role`, an enumeration-hiding role compare) and no
  general, pluggable hook to authorize a whole operation against the principal and its
  input. A new decision-returning `Authorizer` trait (the operation-level counterpart of
  the field-level `FieldAuthorizer`, mirroring the `RLSPolicy` plugin) closes that gap:
  the engine *enforces* but delegates the *decision* to an app-supplied trait object
  (in-process rules, a DB query, or an external service). Register one on `RuntimeConfig`
  via `with_authorizer(…)`; it receives `AuthzRequest { principal, operation, name, input }`
  and returns `Allow` / `Deny { reason }`. Semantics: **fail-closed** — any policy error or
  a `Deny` returns HTTP 403 `FORBIDDEN` and the operation never executes (the underlying
  policy error is not surfaced); the decision **AND-composes** with `requires_role` (both
  must allow, and `requires_role` keeps its enumeration-hiding "not found in schema"
  response — it is *not* routed through the authorizer); and the **anonymous** entry path
  is consulted with `principal: None` rather than blanket-denied, so public operations
  remain expressible. **Path coverage (the security-critical part):** every operation entry
  path is gated — authenticated and anonymous GraphQL (incl. multi-root, where a deny on
  any root fails the whole request before dispatch), MCP, **all REST reads** (GET, count,
  streaming, embedding, bulk-by-filter) at the shared read runner, **all mutations** at the
  universal mutation chokepoint (`execute_mutation_impl`, which also covers the
  anonymous-REST write path that bypasses the GraphQL chokepoints), introspection,
  federation `_entities`, and **subscriptions** at subscribe-time (a deny rejects with a
  `FORBIDDEN` GraphQL-WS error). Because the gate runs *before* the response cache, a warm
  cache never replays an allow past a later deny (no cache bypass needed, unlike the
  per-row field authorizer). **API note:** `AuthzRequest.principal` is
  `Option<&SecurityContext>` (a deliberate divergence from the field authorizer's
  non-optional principal) so the anonymous path is a first-class, explicit case. No
  compiled-schema change. Per-event subscription re-evaluation, federation per-entity-type
  granularity, an `RLSPolicy` argument widening, and a declarative/SDK authoring surface are
  tracked follow-ups. See `docs/guides/operation-authorization.md`.

- **Dynamic field-level authorization — pluggable `FieldAuthorizer` (#423).** v2 had
  only *static* field gating (`field(requires_scope=…)`): it can answer "does this
  principal hold scope X?" but not relational/contextual rules that depend on the
  **row** being resolved, the **principal**, and the **field arguments** (e.g. "show
  `User.email` only to the row's owner or an admin"). A new pluggable, decision-returning
  `FieldAuthorizer` trait (the field analogue of an operation-level authorizer, mirroring
  the `RLSPolicy` plugin) closes that gap. Register one on `RuntimeConfig` via
  `with_field_authorizer(…)`; mark a field policy-gated with `authorize: true` in the
  compiled schema (authored as `field(authorize=True)` → `IntermediateField.authorize`).
  For each selected gated field the engine consults the authorizer per row, passing the
  principal, the **full** row (`parent`), and the field arguments. Semantics:
  **fail-closed** — any policy error or a `Deny { on_deny: Reject }` returns HTTP 403
  `FORBIDDEN` and the value is never served; `Deny { on_deny: Mask }` nulls just that
  field on just that row; and the decision **AND-composes** with the static
  `requires_scope` gate (a field is visible only if both allow). Enforced on the
  authenticated query and mutation paths; **every other projection path
  (unauthenticated query, REST direct, Relay list/`node`, federation `_entities`) fails
  closed** when a policy-gated field could be projected — a missed path cannot leak a
  gated field. Per-row enforcement on Relay/federation, an SDK `@authorize_field`
  authoring surface, and nested-field enforcement are tracked follow-ups (top-level
  fields are enforced today; nested gated fields fail closed). **Compiled-schema format
  note:** `FieldDefinition.authorize` / `IntermediateField.authorize` are new fields;
  unlike the project's usual "plain required field, recompile to migrate" stance for
  compiled-schema additions, this one keeps `#[serde(default, skip_serializing_if = …)]`
  (a deliberate divergence) so `authorize: false` is never serialized — existing golden
  fixtures and the fuzz corpus stay byte-stable and no recompile is forced.

- **Outbound observer webhooks can now be HMAC-signed (#345).** Webhook payloads
  were sent unsigned, so receivers had no way to authenticate them — the
  documented receiver-side verification pattern was not implementable
  end-to-end. Setting `signing_secret_env` on a webhook action (the env var
  *name* holding the secret) now signs the payload with HMAC-SHA256 and attaches
  `X-FraiseQL-Signature-256: t=<unix_ts>,v1=<hex>`, byte-compatible with
  `fraiseql-webhooks`'s `StripeVerifier` (the signature is computed over the
  exact bytes transmitted on the wire, not a re-serialization). If
  `signing_secret_env` is set but the env var is absent or empty, dispatch fails
  loud rather than silently sending an unsigned payload. Settable on
  DB-defined observers and via the `/api/observers` admin API; unset leaves
  delivery unsigned (back-compat).

- **PostgreSQL token-revocation backend implemented (#357).** `[security.token_revocation]
  backend = "postgres"` previously fell back to an in-memory store after a single warning —
  revocations were lost on restart and not shared across replicas, silently breaking the
  cross-replica revocation contract operators expected. The binary now provisions a real
  PostgreSQL-backed store (table `fraiseql_revoked_tokens`, idempotent migration) on the
  PostgreSQL runtime path, so revoked `jti`s persist and are shared across replicas. An
  unrecognised `backend` value is now a hard startup error instead of a silent in-memory
  fallback, and a non-PostgreSQL deployment that requests `backend = "postgres"` warns at
  startup that the backend is unavailable.

- **Failed-login lockout config is no longer silently ignored (#356).** The server
  previously dropped `[security.rate_limiting] failed_login_max_attempts` /
  `failed_login_lockout_secs` on deserialization. The off-the-shelf binary performs no
  first-factor login of its own (OIDC/JWT is validated cryptographically and delegated
  to the identity provider; TOTP MFA is a library-only feature the binary does not
  mount), so it cannot enforce a failed-login lockout. The fields are now captured, and
  tuning them away from the defaults refuses startup in production with an actionable
  message (enforce brute-force protection at the identity provider or edge proxy),
  downgraded to a warning under `FRAISEQL_ENV=development`. Untouched default values
  still boot silently. **Breaking:** a production config that set non-default
  `failed_login_*` values now fails to start until they are removed.

- **PKCE refuses to boot without state encryption in production (#360).** When
  `[security.pkce] enabled = true` but `[security.state_encryption]` is missing or
  disabled, the server now refuses to start in production instead of serving
  `/auth/start` while emitting only a warning — the outbound state token would
  otherwise be the raw, unencrypted lookup key, contradicting the documented "state
  encryption is enforced" posture. Set `FRAISEQL_ENV=development` to downgrade the
  refusal to a warning for local development.

- **JWKS rotation no longer leaves revoked keys cached (#361).** When the OIDC
  provider rotates signing keys, FraiseQL now replaces its JWKS cache with the
  provider's current key set on the next refetch — even when the looked-up `kid` is
  absent — so a token signed by a rotated-out key stops validating once the cache
  refreshes, instead of being trusted until the cache TTL expires. `fraiseql-core`
  embedders can close the window immediately on a known key compromise with the new
  `OidcValidator::invalidate_jwks_cache` (flush) and `refresh_jwks` (eager refetch)
  methods; operators of the off-the-shelf binary can trigger the same via the new
  admin-token-gated `POST /admin/v1/auth/refresh-jwks` endpoint (fail-closed: if the
  provider is unreachable the cache is invalidated anyway). The `jwks_cache_ttl_secs`
  documentation now describes it as the maximum stolen-key replay window once a
  rotation has propagated.

- **Top-level page-size ceiling (#421).** A root query's `first`/`last`/`limit`
  argument is now capped at a configurable maximum (default **1000**) before it
  reaches SQL, closing an unbounded-pagination denial-of-service vector — a single
  query could previously request millions of rows, sizing the database scan, the
  materialized JSONB, and the response buffer with no server-side limit. A request
  exceeding the ceiling is rejected with a validation error. Configure it via
  `[validation] max_page_size` in `fraiseql.toml`, the `FRAISEQL_MAX_PAGE_SIZE`
  environment variable (a number, or `0`/`none` to disable), or
  `RuntimeConfig::max_page_size` for direct `fraiseql-core` embedders. Also fixed
  an integer overflow in the relay `page_size + 1` fetch when pagination is
  unbounded.

- **WebSocket subscriptions now enforce tenant dispatch (#331).** The subscription
  upgrade previously resolved the tenant key with `security_context = None`,
  `domain_registry = None`, and `strict = false` hard-coded — silently dropping JWT
  `tenant_id` precedence, ignoring an installed domain registry, and disabling the
  strict cross-source validation the GraphQL handler applies when RLS is configured.
  A client could carry a JWT for tenant `bar` and still tag its subscription as
  tenant `foo` via an `X-Tenant-ID` header. The handler now extracts the
  authenticated `SecurityContext`, propagates the domain registry, and drives strict
  mode from `schema.has_rls_configured()`, rejecting the upgrade (HTTP 400) on a
  conflicting or invalid tenant key — mirroring the GraphQL handler exactly.

- **Storage list-prefix LIKE-injection (#339).** The `prefix` filter on
  `GET /storage/v1/list/{bucket}` is now matched as a literal string. A client-supplied
  `%` or `_` was previously interpolated into the metadata `LIKE` pattern unescaped,
  letting a caller widen the match and enumerate a bucket's keys (e.g. `prefix=%`
  matched every object). The prefix is now escaped and bound with an explicit `ESCAPE`
  clause.

- **Storage stored-XSS hardening (#337).** Object downloads now always carry
  `X-Content-Type-Options: nosniff` and default to `Content-Disposition: attachment`,
  so an uploaded payload with a client-chosen `Content-Type` (e.g. HTML or SVG) can no
  longer be rendered as active content in the storage origin. A bucket may opt into
  in-browser rendering with the new `BucketConfig::serve_inline` flag, but content
  types browsers execute as active content (`text/html`, `image/svg+xml`, …) stay
  attachments even then.

### Added

- **`fraiseql-cli validate --against-db` — static server↔database mutation-contract
  check (#397).** The server invokes each mutation as `SELECT * FROM <sql_source>(…)` and
  decodes the returned row into `MutationResponse`; both halves of that contract — the
  *call binding* and the *response shape* — were only mirrored by hand between the compiled
  schema and the SQL functions, so every drift surfaced as an opaque runtime 500 (the root
  of the #413/#414 family). `validate --against-db <DATABASE_URL> schema.compiled.json` now
  verifies the contract against a live PostgreSQL **without booting a server or invoking any
  mutation**: for each DB-backed mutation it checks that `sql_source` resolves to exactly one
  function (catching *does not exist* and *is not unique*) whose input arity matches what the
  runtime sends (the positional args — flat, flattened input-object fields, or the
  update-path jsonb payload — plus the trailing injected params), that the update payload
  parameter is `jsonb`, that the trailing parameter names match the inject keys, and that the
  function's result row carries `succeeded` + `state_changed` (both `boolean`, required by
  the decoder) with compatible types for the optional `MutationResponse` columns (`error_class`
  accepts `text` or a project enum). Error-severity findings fail the command (exit 1) for CI
  gating; `--json` emits a machine-readable report. The *behavioural* response invariants
  (`succeeded ⇒ error_class IS NULL`, `http_status ∈ 100..=599`, …) are out of scope — they
  are only observable by invoking the mutation, which would have database side effects.

- **`fraiseql-cli doctor --against-db` — PL/pgSQL body-resolution pass (#409).** PostgreSQL
  defers PL/pgSQL body analysis to runtime, so a migration that changes a function's
  signature silently breaks every *internal* caller until that branch executes — invisible to
  `compile` and to the server-facing check in #397. `doctor --against-db <DATABASE_URL>
  --schemas a,b` resolves every call inside each managed function's body against the live
  catalog (via the [`plpgsql_check`](https://github.com/okbob/plpgsql_check) extension) and
  reports unresolved internal calls as failed doctor checks. It degrades gracefully: when
  `plpgsql_check` is not installed (the common case on managed Postgres), the pass is skipped
  with a `Warn` and an install hint rather than failing.

### Breaking

- **Compiled-schema format: input-object fields now carry `nullable` (#414).** Each
  `InputFieldDefinition` in `schema.compiled.json` gains a `nullable` boolean (mirroring the
  output `FieldDefinition.nullable`), so the runtime can distinguish a required (non-null)
  input field from an optional one — previously a compiled input field carried only `name` +
  `field_type` and requiredness was lost. **`fraiseql-cli compile` emits the new field;
  recompile your schema** to pick up required-input-field enforcement (see Fixed, below). The
  field is serde-defaulted to `true` (nullable) on load, so an older compiled artifact still
  deserialises — it simply enforces nothing until recompiled. Nullability is driven by the
  `nullable` flag the SDK emits, **not** by a `!` suffix in the type string: a hand-written
  compiled schema encoding a required field only as `"field_type": "ID!"` (without
  `"nullable": false`) is treated as optional until recompiled via the SDK.

### Fixed

- **Required input fields are now enforced before the database call (#414).** `fraiseql-cli
  compile` dropped per-field nullability for input-object types, so the runtime could not
  tell a required input field from an optional one: a create mutation that **omitted** a
  non-null input field (or passed explicit `null`) flattened a SQL `NULL` straight into the
  function instead of being rejected. The compiler now carries input-field nullability into
  the compiled schema (see Breaking, above), and the mutation executor rejects an
  omitted-or-explicit-null required (non-null, no-default) input field with a GraphQL
  **validation error** (HTTP 200 + `errors[]`) before any DB round-trip — a clear, actionable
  message in place of relying on a downstream constraint failure (post-#413 those surface as
  HTTP 400, but only after the function runs). Enforcement covers the insert/delete/custom
  **flatten** path at the universal mutation chokepoint. As part of the same lookup fix, a
  **latent camelCase Insert bug** is closed: under `NamingConvention::CamelCase` the flatten
  path looked up input values by the canonical (snake_case) name while clients send camelCase
  keys, so values silently became `NULL`; fields are now matched by their GraphQL surface
  name. GraphQL introspection now reports a required input field as `NON_NULL`. **Not**
  covered (tracked follow-ups): update-path three-state inputs (an omitted field still means
  "leave unchanged"), the gRPC mutation path (binds proto fields directly, bypassing the
  chokepoint), query/filter inputs (optional by design), input-object-field **kind** +
  list-element nullability in introspection, and applying an input field's default for an
  absent value.

- **Client-input DB errors now return HTTP 400, not 500 (#413).** When a PL/pgSQL
  mutation raised on **client input** — a malformed value that fails a cast (e.g.
  `"not-a-uuid"` → `uuid`, SQLSTATE `22P02`) or an integrity-constraint violation
  (not-null / unique / foreign-key / check, class `23xxx`) — the server returned
  **HTTP 500 / `DATABASE_ERROR`**, because every `FraiseQLError::Database` was mapped
  to `INTERNAL_SERVER_ERROR` regardless of SQLSTATE. HTTP-aware clients and test
  harnesses treat 5xx as a server fault to retry/alert on, not a 4xx to surface to the
  user. The server now classifies a `Database` error by its SQLSTATE: class **`22`**
  (data exception) → **HTTP 400 / `BAD_USER_INPUT`**, class **`23`** (integrity
  constraint) → **HTTP 400 / `CONSTRAINT_VIOLATION`**; every other class, an absent
  SQLSTATE, and connection-pool errors stay **HTTP 500 / `DATABASE_ERROR`**. The PG
  message is preserved in the structured error. Applied to **both** transports — the
  GraphQL mapper (`from_fraiseql_error`) and the REST/bulk mapper (`RestError::from`),
  which classify via one shared predicate so they cannot drift. **Client-visible
  behaviour change:** these specific cases move from 500 to 400. (Per-subclass
  `23505 unique_violation → 409 Conflict`, surfacing the SQLSTATE in the error
  extensions, and the gRPC `Code::Internal` path are tracked follow-ups.)

- **Observer DLQ CLI fabricated data; now talks to the real server API (#341).** The
  `fraiseql-observers dlq` subcommands (list/show/retry/retry-all/remove/stats)
  returned hard-coded JSON fixtures — synthetic items, invented retry counts and
  stats — so the CLI confidently reported state that did not exist. They now call
  the server's observer admin API over HTTP and render the real response, or fail
  loud: a non-2xx status (e.g. a 404 from `remove` on a missing item) or an
  unreachable server surfaces as an error with a non-zero exit, never a synthetic
  success. New global args `--base-url` (default `http://localhost:8000`) and
  `--admin-token` (sent as `Authorization: Bearer`) target the server. Two new
  server endpoints back the CLI: `DELETE /api/observers/dlq/{id}` (remove) and
  `GET /api/observers/dlq/stats` (aggregate stats). Mock-era filters the server API
  does not support (`--observer`/`--after`/`--by-observer`/`--by-error`/`--dry-run`)
  now emit a warning rather than being silently honored.

- **Observer email action reported success without sending (#349).** `EmailAction`
  was a stub that always returned success, so a dead email integration showed green
  metrics while silently dropping every message. It now sends real email over SMTP
  via `lettre` (rustls, no OpenSSL): configure `[observers.runtime.email]`
  (`host`/`port`/`from`/`tls` = `start_tls`|`tls`|`none`, with credentials supplied
  via the `username_env`/`password_env` environment-variable *names*). SMTP failures
  are classified — permanent (5xx, bad recipient, auth rejected) go straight to the
  DLQ, transient (connection refused, timeout, 4xx greylisting) are retried per
  policy. When SMTP is **not** configured the action fails loud (permanent) instead
  of faking success, so a misconfigured email integration is always surfaced. The
  `[observers.runtime.email]` block is strict (`deny_unknown_fields`): a typo or a
  literal-credential key fails the parse. The failure path (a refused send is a
  loud, classified error) is covered without infra; the happy path is covered
  end-to-end by a MailHog SMTP sink bound into the `integration(observers)` CI
  leg — a test sends through `lettre` and asserts the message arrives.

- **Observer transport selection was silently ignored; NATS ran on PostgreSQL (#350).**
  The off-the-shelf binary never read `[observers.runtime.transport]` /
  `FRAISEQL_OBSERVER_TRANSPORT`, so selecting `transport = "nats"` quietly ran on
  PostgreSQL LISTEN/NOTIFY with a false "running on NATS" posture. The runtime now
  honors the selection: PostgreSQL drives the existing change-log listener, while
  NATS `JetStream` and the in-memory transport run through the library's
  `EventTransport` stream — a non-Postgres selection can never fall through to the
  PG listener. A selection this binary cannot run (NATS without the `observers-nats`
  feature, or no broker URL) refuses to boot in production (downgraded to a warning
  under `FRAISEQL_ENV=development`, which runs on PostgreSQL), and a configured NATS
  transport whose broker is unreachable fails startup rather than silently coming up
  without it. Configure via `[observers.runtime.transport]` (`transport = "postgres"
  | "nats" | "in_memory"`) with `[observers.runtime.transport.nats]` for the broker
  URL and JetStream settings; NATS requires a binary built with `--features
  observers-nats`.

- **DLQ retry could double-fire the action under concurrent requests (#344).**
  `POST /api/observers/dlq/{id}/retry` read the item, released the lock, then
  re-dispatched and removed it — so two concurrent retries (or a per-item retry
  racing `retry-all`) both dispatched the action, turning at-least-once delivery
  into at-least-twice. Retries now go through an atomic claim (single-lock
  remove-and-return): exactly one caller dispatches per claim, the loser gets
  404; `retry-all` drains via the same claim. A failed redispatch re-inserts the
  item (cap-bypassing, so a DLQ that refilled to capacity during the claim
  cannot silently drop the just-failed item) with its `attempts` incremented.

- **Observer DLQ ignored `max_dlq_size`; failed retries silently destroyed (#343).**
  The `fraiseql-server` binary's in-memory dead letter queue grew without bound
  — `max_dlq_size` was a documented setting the binary never honored, a memory
  DoS amplifier under sustained action failures. It now enforces the cap with
  the same policy as the `fraiseql-observers` library (drop-newest + a `warn!`
  with matching fields + an overflow counter), enforced atomically under the
  items mutex. The overflow counter is surfaced as `dlq_dropped` on
  `GET /api/observers/delivery/health`. Configure via
  `[observers.runtime] max_dlq_size` (default `None` = unbounded, for
  back-compat). Separately, `mark_retry_failed` previously deleted the failed
  item outright, destroying the audit trail; it now keeps the item, increments
  its `attempts`, and records the latest error — items leave the DLQ only on
  success or an explicit operator delete.

- **Observer runtime routes mounted at the wrong prefix (#340).** The observer
  runtime-health and reload endpoints were `merge`d at the router root, so
  `/api/observers/runtime/health` and `/api/observers/runtime/reload` returned
  **404** while the handlers were instead reachable at `/runtime/health` /
  `/runtime/reload`, shadowing any user routes there. Both are now `nest`ed under
  `/api/observers` like the other observer routers. **Breaking (path move):**
  clients calling the root `/runtime/*` paths must switch to
  `/api/observers/runtime/*`.

- **Cross-bucket object collisions (#336).** Storage backend operations
  (upload / download / delete / presign) now scope the object key by bucket
  (`{bucket}/{key}`). Two objects with the same key in different buckets previously
  mapped to the same backend object, so one upload could overwrite or shadow another
  and a delete in one bucket could remove a different bucket's bytes. Object metadata
  already keyed on `(bucket, key)`; the backend store now matches.

- **Storage uploads capped below the per-bucket limit (#338).** The storage router now
  applies its own request-body limit, sized to the largest configured `max_object_bytes`
  (or 100 MiB when a bucket is unlimited), overriding the server-wide
  `max_request_body_bytes` (default 1 MiB) and axum's 2 MiB extractor default for storage
  routes only. Previously a bucket's `max_object_bytes` was unreachable and larger uploads
  failed with a generic 413. Very large objects should still use presigned
  direct-to-backend uploads.

- **Storage routes unreachable from the `fraiseql-server` binary (#334).** The
  off-the-shelf binary now wires a `[storage.<name>]` TOML section into a mounted
  `/storage/v1/*` route group (object upload / download / delete, list, presign) at
  startup. Previously `ServerConfig` had no `storage` field, so serde silently dropped the
  section and every storage path returned **404** even though the library API existed. The
  section name is the logical bucket; optional `access` (`"private"` default /
  `"public_read"`), `max_object_bytes`, `allowed_mime_types`, and `serve_inline` set the
  bucket policy. Authentication uses the configured OIDC validator (per-user RLS) and/or a
  `storage_token` bearer treated as a full-access admin; with neither set, only
  `public_read` buckets are reachable (read-only). Object storage via the binary is
  **PostgreSQL-only** (the object-metadata repository requires PostgreSQL), and **v1
  supports a single backend** — configuring more than one `[storage.<name>]` is a startup
  error. `[files.<name>]` sections are parsed but not yet wired (a startup warning is
  logged).

- **Suspended tenant now returns HTTP 503 + `Retry-After` (#332).** The GraphQL
  handler mapped every error from per-tenant executor dispatch to HTTP 403,
  collapsing a suspended tenant (`ServiceUnavailable { retry_after }`) and an
  unknown tenant key (`Authorization`) onto the same status and dropping the
  retry hint. Dispatch errors are now mapped by variant: an unknown key stays
  403 Forbidden, while a suspended tenant returns 503 with a `Retry-After`
  header carrying the registry's retry value (60s), matching the documented
  suspend/resume contract.

- **Multi-tenant runtime now wired into the `fraiseql-server` binary (#330).** The
  per-tenant executor runtime (registry, `X-Tenant-ID` / JWT `tenant_id` / Host
  dispatch, the `/api/v1/admin/tenants/*` lifecycle API, suspend/resume, and the
  explicit-deny 403 for an unregistered tenant key) was implemented only as a
  library API; the off-the-shelf binary never installed it, so the admin tenant
  endpoints returned `404 multi-tenant mode not enabled` and an explicit
  `X-Tenant-ID` was silently served by the default executor. Enable it with
  `[tenancy.runtime] enabled = true` in `fraiseql.toml`: the binary installs the
  registry (seeded with the default executor), an in-memory tenant audit log, the
  domain registry, and — on PostgreSQL — the executor factory so
  `PUT /api/v1/admin/tenants/{key}` provisions a tenant with its own connection
  (and schema, in `tenancy.mode = "schema"`). `PostgresAdapter` now implements
  `FromPoolConfig`. Runtime provisioning is PostgreSQL-only; dispatch to
  pre-registered tenants works on any adapter.

### Changed

- **Breaking (observer config layout, #342):** the server's observer **runtime**
  tuning moved from the flat `[observers]` table to a dedicated
  `[observers.runtime]` sub-table: `poll_interval_ms`, `batch_size`,
  `channel_capacity`, `auto_reload`, `reload_interval_secs`, and the
  `[observers.pool]` table (now `[observers.runtime.pool]`). The same
  `fraiseql.toml` is consumed by both `fraiseql compile` and `fraiseql-server`;
  the compiler owns the `[observers]` top-level keys (`backend`/`handlers`/…) and
  rejected server-tuning keys placed there, so a shared file could never carry
  both. With the relocation, `fraiseql compile` tolerates `[observers.runtime]`
  and the server reads it. Two fail-loud guards replace the previous silent
  swallow: a server-tuning key left at the flat `[observers]` level now fails
  startup with a migration message naming the key and its new home, and an
  unrecognised key under `[observers.runtime]` (e.g. a typo) fails to parse
  instead of being ignored. Move any server-tuning keys under
  `[observers.runtime]` to upgrade.

- **Breaking (runtime behavior, #421):** clients requesting more than 1000 rows in
  a single page now receive a validation error by default. Raise
  `[validation] max_page_size`, set `FRAISEQL_MAX_PAGE_SIZE`, or set it to `0`/`none`
  to restore the previous unbounded behavior.

- **Breaking (storage backend layout, #336):** objects are now stored under
  bucket-prefixed backend keys (`{bucket}/{key}`). Deployments that wrote objects via
  the `fraiseql-storage` library routes before this release must relocate existing backend
  objects under the new prefix. Earlier releases' off-the-shelf `fraiseql-server` binary
  did not mount these routes (#334 wires them in this release), so only deployments that
  used storage through the library API before upgrading are affected.

- **Storage downloads default to `Content-Disposition: attachment` (#337).** Buckets
  that need in-browser rendering must opt in with `BucketConfig::serve_inline = true`.

- **Breaking (tenant-key alphabet, #333):** the `X-Tenant-ID` header validator is
  tightened to `[a-zA-Z0-9_]` with a 56-character cap (derived from PostgreSQL's
  63-character identifier limit minus the `tenant_` schema prefix), matching the
  schema-mode DDL helpers. Hyphenated keys (e.g. `acme-corp`) and keys of 57–128
  characters — previously accepted at dispatch but silently rejected at schema-mode
  provisioning — are now rejected uniformly, including at tenant registration
  (`PUT /api/v1/admin/tenants/{key}`). Deployments using hyphenated tenant keys in
  row-mode must migrate to underscores.

## [2.4.0] - 2026-06-04

### Added

- **Multi-database runtime support for `fraiseql-server` and `fraiseql run`
  (#327).** The server binary and the CLI's `run` command now dispatch on the
  `database_url` scheme at startup and construct the matching adapter:
  `postgresql://` (always available), `mysql://`, `sqlite://`, or
  `sqlserver://`. Non-PostgreSQL adapters are gated behind new Cargo features
  on `fraiseql-server` (`mysql`, `sqlite`, `sqlserver`) and `fraiseql-cli`
  (which cascade-enable them on `fraiseql-server` when `run-server` is also
  on). Build with e.g. `cargo install fraiseql-server --features mysql,sqlite`.
  Pointing the binary at a URL whose scheme matches an adapter that was not
  compiled in fails fast at startup with a clear `--features <name>` rebuild
  hint, instead of producing an opaque driver error from inside `tokio-postgres`.
  Two intentional constraints:
  1. **SQLite is read-only.** `SqliteAdapter` deliberately does not implement
     `SupportsMutations`. Starting the server against a `sqlite://` URL with a
     schema that declares any mutations fails at startup with a diagnostic
     naming the first three offending mutations.
  2. **Observers (LISTEN/NOTIFY) remain PostgreSQL-only.** Arrow Flight, the
     observer-pool initialisation, and relay-pagination auto-detection are
     skipped for the non-PostgreSQL adapter paths and are tracked as separate
     follow-ups. The `arrow` Cargo feature is silently no-op on non-PG paths.
  A new module `fraiseql_server::url_guard` exposes the `DatabaseScheme` enum,
  `parse_database_url`, and `guard_sqlite_mutations` for downstream tooling
  that needs to mirror the dispatch logic.
- **Entity change log over GraphQL — opt-in pull-based event consumption (#149).**
  Set `[changelog] expose = true` (requires `[observers]`) and the compiler injects
  read-only `EntityChangeLog` / `TransportCheckpoint` types, a cursor-paginated
  `entity_change_logs` query, a `transport_checkpoint` point lookup, and an
  idempotent `upsert_transport_checkpoint` mutation — all backed by views the new
  migration `07_create_changelog_views.sql` installs. Sidecar consumers (AI
  scoring, search-index sync, audit dashboards) can now poll the observer
  change-log over the same GraphQL endpoint as the rest of the API — same auth,
  audit logging, and rate limiting — instead of opening a side-channel PostgreSQL
  connection. Cursor pagination uses the standard generic filter machinery
  (`where: { pk_entity_change_log: { gt: $cursor } } orderBy limit`), numeric and
  gap-free. Access is gated by configurable `read_role` / `write_role`; denied
  callers receive `"not found in schema"` (enumeration-prevention). This also adds
  `MutationDefinition.requires_role` with runtime enforcement. See
  `docs/guides/changelog-graphql.md` and `examples/changelog-sidecar/`.
- **`fraiseql generate-client typescript` — typed TypeScript clients from a
  compiled schema (#291).** A new `fraiseql-codegen` crate turns a
  `schema.compiled.json` into a consumer-side client that *calls* a FraiseQL API:
  interfaces for every type, typed query/mutation functions, a relay
  `Connection<T>`, relationship metadata, and a tiny `fetch`-based runtime client
  with zero dependencies. This is distinct from `fraiseql generate <language>`,
  which emits server-side *authoring* code fed back into the compiler. Two
  deliberate, GraphQL-correct design choices set it apart from naive schema-to-TS
  tools: (1) result types are **selection-scoped** — each type contains exactly
  the leaf fields (scalars, enums, `__typename`) the generated default document
  fetches, so the type never claims relationship fields it did not retrieve; and
  (2) mutations are typed as **result unions discriminated by `__typename`** (with
  an `isErrorResult` type guard and a `status` field on `@fraiseql.error` types),
  matching the actual wire contract rather than a synthetic response wrapper.
  Every generated file carries a `schema-hash` header for CI staleness detection.
  The `fraiseql-codegen` crate also exposes the generator programmatically
  (`fraiseql_codegen::client::typescript::generate`) for IDE extensions,
  scaffolders, and build plugins. See `docs/guides/typed-clients.md` and
  `examples/typescript-client/`.

- **FreeBSD (`x86_64-unknown-freebsd`) is now a CI-enforced compile target (#148).**
  A new `freebsd-cross-check` job cross-compiles the workspace (default
  features) and the full `fraiseql-server` feature surface for FreeBSD on
  every PR, using a FreeBSD `base.txz` sysroot + `clang` on the existing Linux
  runners — no FreeBSD VM or extra infrastructure. A dependency audit confirmed
  no Linux-specific source assumptions (the one `/proc/self/limits` read is
  already `#[cfg(target_os = "linux")]`-gated; `notify` selects its kqueue
  backend on BSD). Two optional features are intentionally out of cross-check
  scope because they have no Linux→FreeBSD cross path and must be built natively
  on FreeBSD: the Deno edge-functions runtime (`fraiseql-functions/runtime-deno`
  → `v8`) and the SQL Server backend (`tiberius` → `openssl-sys`). Compile-time
  only — runtime testing on a real FreeBSD host remains deferred pending user
  signal. No engine changes.

### Fixed

- **Azure Blob (`azure-blob`) and Google Cloud Storage (`gcs`) backends now
  honour the configured `endpoint` URL (#326).** Previously the `endpoint`
  field on `StorageConfig` was silently ignored for these two backends, which
  hardcoded `*.blob.core.windows.net` / `storage.googleapis.com` into every
  request — so the Azurite and fake-gcs-server emulators could not be used for
  local development or CI. Both backends now route through the configured
  endpoint (matching the existing S3 behaviour), enabling emulator round-trips.
  Real-cloud Azure/GCS deployments are unaffected: the endpoint defaults to the
  production hostname when not specified. `AzureBackend` and `GcsBackend` gain
  additive `new_with_endpoint` constructors (and `AzureBackend` an additive
  `create_container_if_missing`); the existing `new` constructors are unchanged.

- **Session variables now reach mutation SQL functions and RLS policies (#329).**
  Before this release, `current_setting('app.x', true)` inside a mutation
  function, an RLS-protected view, a relay-paginated list, or an aggregate
  always returned NULL: `PostgresAdapter::set_session_variables` ran
  `SELECT set_config(..., true)` on a pooled connection in its own autocommit
  transaction — transaction-local *and* on a different connection than the
  subsequent operation. Session variables are now applied transaction-locally
  on the **same connection** as the operation. Applications that worked around
  this by passing tenant/user ids as mutation arguments via `inject_params` can
  continue to do so, or now rely on session variables.

- **Update mutations re-case the input payload to the schema's canonical field
  names (#400).** With `naming_convention = "camelCase"`, the Update path forwarded
  the GraphQL input object to the SQL function verbatim, so a `camelCase` surface
  delivered `camelCase` keys (`{ "fullName": ... }`) that a `snake_case` function
  reading `payload->>'full_name'` / `jsonb_populate_record` could not see — silently
  writing NULLs (or failing NOT NULL constraints). The payload is now re-cased to
  the canonical names before it reaches the function, recursing into nested input
  objects and arrays of input objects. The mapping is driven by the input type's
  per-field map (not a lossy regex), so acronyms and intentional names are preserved;
  `Preserve` schemas, unknown input types, and unmatched keys are untouched. The
  Insert/Delete paths were already correct (positional args).

- **Mutation success responses now project nested typed-object fields like queries
  do (#410).** The success arm projected the returned entity with a flat mapping
  keyed by the selection's `camelCase` names, so it could not read the `snake_case`
  entity JSONB and dropped (or failed to recurse into) nested typed-object fields —
  a mutation selecting `{ thing { id billingAddress { postalCode } } }` lost
  `billingAddress` entirely, while the same selection over a query returned it.
  Mutation success **and** error responses now flow through a single canonical
  entity projector that mirrors the query path exactly (`snake_case` source keys,
  `camelCase` surface output, depth-aware recursion into nested objects), so a
  mutation's payload and a query over the same entity return an identical shape.
  This also removes a latent acronym drift between the SQL and Rust projectors,
  which now share one `to_snake_case` definition. As part of the same unification,
  mutation result selections now resolve named fragment spreads and evaluate
  `@skip` / `@include` directives before projection, exactly as the query path
  does — so factoring mutation fields into a fragment (or guarding them with a
  directive) now behaves identically to a query.

- **Mutation error fallback now detects `__typename` inside inline fragments
  (#419).** When a mutation's error outcome has no matching error type declared
  in the return union, the response carries just `__typename` (plus the synthetic
  `status`), and only when the client selects `__typename`. That selection scan
  was top-level only, so a client that nested `__typename` inside an inline
  fragment — `... on SomeError { __typename }` — was silently denied it, even
  though #410 already resolves named fragment spreads and `@skip` / `@include`
  on this same path. The scan now recurses into inline fragments, reusing the
  `selections_contain_field` helper the query projector already uses.

- **Aliased query fields now read from their source JSONB column (#418).** The
  query SQL projector derived a field's JSONB key from its *response* key
  (`to_snake_case` of the alias), so an aliased field like `myName: fullName`
  generated `data->>'my_name'` and read the wrong (nonexistent) column —
  returning null where the un-aliased query worked. `ProjectionField` now
  carries a `source` (the GraphQL field name that drives the JSONB key) distinct
  from `name` (the output/response key): the column is read from `source` and the
  value emitted under `name`. The mutation projector was already correct after
  #410.

### Changed

- Upgraded the RustCrypto hashing stack jointly (#300): `sha1 0.10 → 0.11`,
  `sha2 0.10 → 0.11`, `hmac 0.12 → 0.13`, and `pbkdf2 0.12 → 0.13` (the latter
  forced by the wire SCRAM PRF). These all ride `digest 0.11` / `crypto-common
  0.2` and cannot be mixed with the `digest 0.10` generation, so they move in
  lockstep. Call sites were updated to the new API: `KeyInit` is now imported
  for `Hmac::new_from_slice`, and digest outputs are hex-encoded via
  `hex::encode` (the new `hybrid-array` `Output` no longer implements
  `LowerHex`). No public API changed. The `cargo deny` skip for the transitive
  `sha1 0.10.6` (pinned by `sqlx-mysql`) was re-added.
- `DatabaseAdapter` gains `execute_function_call_with_session`,
  `execute_with_projection_arc_with_session`,
  `execute_where_query_arc_with_session`, and
  `execute_parameterized_aggregate_with_session`; `RelayDatabaseAdapter` gains
  `execute_relay_page_with_session`. All have default implementations that
  delegate to the existing methods, so custom adapter implementors need not
  change anything (#329).

### Security

- **MCP tool calls now enforce row-level security and authentication.** Pre-v2.4.0 the MCP (Model Context Protocol) transport built a GraphQL query from the tool call and ran it through the *unauthenticated* executor path (`Executor::execute`), bypassing every protection the HTTP GraphQL endpoint applies via `execute_with_security`: RLS `WHERE`-clause injection, session-variable binding, and `@inject` JWT resolution. On a multi-tenant deployment with RLS configured, any MCP client therefore received rows across **all** tenants, regardless of the `[mcp] require_auth` flag — which until now only gated whether the HTTP endpoint was *mounted*, never whether an individual tool call carried a validated identity.

  The fix threads an optional `SecurityContext` through `mcp::executor::call_tool` and makes it **fail closed**: when no security context is present and the compiled schema has an RLS policy configured *or* `require_auth = true`, the tool call is refused with an authentication error instead of running unfiltered. When a context is present the call is routed through `execute_with_security`, so RLS, session variables, and `@inject` apply exactly as they do for HTTP GraphQL. Over the HTTP transport the `Authorization: Bearer` token is now extracted from the request and validated against the configured OIDC validator per call (mirroring the gRPC handler). The stdio transport carries no per-request credentials, so under RLS or `require_auth` it is governed by the same fail-closed policy — to use stdio MCP unauthenticated, disable `require_auth` and do not configure RLS (development only).

- **Query-complexity scorer is now overflow-safe (fail-closed).** The AST complexity scorer in `graphql/complexity.rs` computed `1 + nested * multiplier` with unchecked `usize` arithmetic, and the pagination multiplier (client-controlled `first`/`limit`/`take`/`last`, clamped to ≤100) compounds multiplicatively per nesting level. A crafted deeply-nested query with pagination args reaches ≈100^depth, overflowing `usize`: in release builds (no `overflow-checks`) the score *wrapped* to a small value and slipped under `max_query_complexity`; in overflow-checked builds (debug/test, and `cargo fuzz`, whose `complexity.rs` target asserts "must never panic") it *panicked*. The scorer now uses `saturating_add`/`saturating_mul`, so an overflowing query saturates to `usize::MAX` and is always rejected (`QueryTooComplex`), never wraps under the limit nor panics. Severity is low for FraiseQL specifically — its view/table-view execution returns the full denormalised entity as one JSONB read, so GraphQL nesting is projection rather than join fan-out and a "bypassed" deep query is cheap to run — but the wrap/panic is a genuine robustness defect. (A follow-up will add clamping of the *top-level* `first`/`limit` row count, which is the actual cost lever in FraiseQL.)

- **`POST /auth/revoke` and `POST /auth/revoke-all` are now authenticated** (#358, FW-21 class). In v2.3.x and earlier, both routes were mounted with no auth middleware, so any unauthenticated client could revoke any harvested JWT (by `jti`) or wipe every active session for any user (by `sub`). The handlers used `jsonwebtoken::dangerous::insecure_decode` to extract the `jti` from a body-supplied token without any proof-of-possession, so the attack required nothing beyond a network path to the server. Affected anyone running `[security.token_revocation] enabled = true`.

  The fix has three parts:

  1. The revocation router is now mounted behind `oidc_auth_middleware` — unauthenticated requests get `401 Unauthorized` before reaching the handler. If `[security.token_revocation]` is configured without a corresponding `[auth]` OIDC validator, the routes are *not* mounted at all and a startup warning is emitted, rather than mounting them open.

  2. `POST /auth/revoke` no longer trusts a token submitted in the body. It revokes the `jti` of the bearer token used to authenticate the request — surfaced as a new `SessionJti` request extension populated by the auth middleware. The body's `token` field is still accepted on the wire for compatibility but is ignored. This closes the residual attack where an authenticated alice could `insecure_decode` a body token claiming `sub: "alice"` but carrying a victim's `jti`.

  3. `POST /auth/revoke-all` now requires the caller's authenticated `sub` to match `body.sub`, unless the caller holds the `admin` scope. Cross-user revocation requests return `403 Forbidden` with a `caller_sub`/`target_sub` warning logged for incident response.

- **Webhook dispatch INFO logs no longer leak URLs, headers, or rendered bodies** (#346). Pre-v2.4.0 `WebhookAction::execute` emitted four INFO lines on every dispatch — full URL (including any query-string secrets or embedded credentials), full headers debug-formatted (including any `Authorization: Bearer ...` operators put in the observer `headers` map), the raw `body_template`, and the full rendered event body as JSON. Centralised log aggregators ingested and retained the payload for every dispatch, exposing bearer tokens (reuse → same access as the framework) and PII rows (customer email, shipping address, payment refs) for the retention window.

  The fix: URL / headers / body are demoted to DEBUG (URL, redacted headers, body template) and TRACE (rendered body). INFO now carries only delivery metadata — `action_type`, `event_id`, `host` (no path/query), `status_code`, `duration_ms`. Two new helpers ship: `redact_secret_headers` masks any header whose name contains (case-insensitive) `authorization`, `api-key`, `cookie`, `secret`, or `token` — false-positives (over-masking) are accepted, false-negatives (printing a real bearer) are not. `url_host_only` extracts the host via `reqwest::Url::parse` so even DEBUG-level URL logs strip userinfo / path / query / fragment when needed.

- **Storage `POST /storage/v1/presign/{bucket}/{*key}` now consults `StorageRlsEvaluator`** (#335). Pre-v2.4.0 the handler lacked the `Option<Extension<StorageUser>>` parameter present on every other storage handler and called neither `state.rls.can_read` / `can_write` nor `state.metadata.get`. Any anonymous client could `POST /storage/v1/presign/<bucket>/<key>` with `{"operation":"download","expires_in_secs":86400}` and receive a 24-hour-valid presigned GET URL for any object in any bucket — including `BucketAccess::Private` buckets owned by other users. With `"operation":"upload"`, the same anonymous client received a presigned PUT URL that overwrote arbitrary objects, bypassing bucket-level `max_object_bytes` and `allowed_mime_types` (those checks live in `put_handler`, not in the S3 presigned URL).

  The handler now mirrors `put_handler` / `get_handler`: `operation = "download"` loads the metadata row and consults `state.rls.can_read` (missing objects yield `404`, denied access yields `403`); `operation = "upload"` consults `state.rls.can_write(bucket)` (denied access yields `401`). The RLS gate runs *before* any S3 work, so unauthorised callers do not observe whether the object exists. Known limitation documented inline: bucket-level `max_object_bytes` and `allowed_mime_types` are still not enforced via the S3 presigned PUT URL (S3 cannot encode those constraints in a vanilla presigned URL); operators must restrict presigned uploads to trusted users via RLS or route through `PUT /storage/v1/{bucket}/{*key}` instead.

- **`[auth_hs256]` now requires `audience` to be set** (#359). The HS256 shared-secret testing path is the most likely place for two services to share a signing key (test fixtures, internal service meshes, monorepo CI); pre-v2.4.0 it accepted any token whose `aud` matched the unset (`None`) configuration — i.e., any token from any service. A token minted for service A was accepted by service B, exactly the cross-service token-confusion attack the v2.3 S40 OIDC hardening closes for the OIDC path. `Hs256Config::validate` now returns an error when `audience` is `None`, called from `build_hs256_auth` at server startup with a clear actionable message. Mirrors `OidcConfig::validate` exactly.

- **`FRAISEQL_OBSERVERS_ALLOW_INSECURE` bypass is refused in production environments** (#347). Pre-v2.4.0 the env var disabled every outbound SSRF guard (scheme allowlist, private-IP blocklist, DNS-rebinding defence) in observer dispatch — `validate_outbound_url`, `dns_resolve_and_check`, `executor::dispatch::validate_url_ssrf` — with a `std::sync::Once` warn-on-first-use that was easy to miss in streaming log aggregators. Combined with #348 (anonymous observer install), this was a one-step path to AWS metadata-service credential exfiltration: install an observer pointing at `http://169.254.169.254/latest/meta-data/iam/security-credentials/<role>`, wait for the next mutation.

  The fix centralises the bypass policy in a new `fraiseql_observers::insecure_guard` module. The check now refuses the bypass when ANY production-marker env var is set:

  - `KUBERNETES_SERVICE_HOST` (automatic in any K8s pod).
  - `FRAISEQL_ENV=production` (case-insensitive, also accepts `prod`).
  - `FRAISEQL_PROFILE=production` (case-insensitive, also accepts `prod`).

  When the bypass is refused in production, a structured `ERROR` is logged once per process and a `WARN` is emitted at every outbound dispatch (so operators see the bypass-attempt at every dispatch, not just once at startup). When the bypass is honored in dev, a `WARN` is emitted on every dispatch — the `std::sync::Once` warn-once is gone.

- **Observer admin API now requires authentication** (#348, FW-21 class). All four observer HTTP routers — `observer_routes` (CRUD), `observer_changelog_routes`, `observer_runtime_routes` (`/runtime/health`, `/runtime/reload`), and `observer_dlq_routes` (`/api/observers/dlq/*`) — were previously mounted with no auth middleware. Handlers used `OptionalSecurityContext` (which returns `None` on anonymous calls) or no auth extractor at all, so any unauthenticated client could:

  - `POST /api/observers` — install an attacker-controlled webhook observer pointing at any URL (combined with #347, a one-step path to AWS metadata-service credential exfiltration).
  - `PATCH /api/observers/{id}` — silently re-route an existing observer to an attacker URL.
  - `DELETE /api/observers/{id}` — wipe an observer.
  - `POST /runtime/reload` — denial-of-service against the observer runtime.
  - `GET /api/observers/{id}` — read bearer-token secrets stored in `actions[].headers`.
  - `POST /api/observers/dlq/retry-all` — replay queued events through whatever URL the (now attacker-controlled) observer points at.

  All four router nests now mount behind `oidc_auth_middleware` via `route_layer`. If the `observers` feature is enabled but `[auth]` is not configured (no OIDC validator available), the HTTP admin API is *not* mounted and a `WARN` is logged at startup. The in-process observer runtime — triggers, dispatch, DLQ retention — is unaffected; only the HTTP control plane is gated. Affected anyone running the `observers` feature.

- **Tenant-scoped reads through `CachedDatabaseAdapter` now bypass the result
  cache when session variables are configured (#329)**, until the cache key is
  extended to include a hash of the applied session variables (tracked as a
  follow-up). Before this release the cache key was likewise not
  session-variable-aware, but the bug masked any actual leak by making session
  variables invisible to RLS policies.

### Breaking changes

- **`POST /auth/revoke` request body changed.** The `token` field is now `Option<String>` and ignored. Clients that previously submitted a body token will continue to receive `200 OK`, but the revocation now targets the *authentication* token, not the body token. Update any flow that depended on revoking an arbitrary harvested token via this endpoint — there is no longer such a primitive.

- **`POST /auth/revoke` and `POST /auth/revoke-all` now require a valid bearer token.** Anonymous calls return `401 Unauthorized`. Update any internal tooling that called these endpoints unauthenticated.

- **`[auth_hs256]` refuses to boot without `audience`.** Deployments using HS256 auth with no `audience` will fail startup with an actionable error message. Set `audience = "..."` in the `[auth_hs256]` section of `fraiseql.toml` to your API identifier. There is no compatibility shim — the cross-service token-confusion attack the fix closes (#359) is not acceptable in a "warn-and-continue" mode.

- **Token revocation requires `[auth]` to be configured.** If `[security.token_revocation] enabled = true` but no OIDC validator is present, the revocation routes are skipped at startup (with a `WARN` log) rather than mounted open. Configure `[auth]` in `fraiseql.toml` to restore the routes.

- **Observer admin HTTP API requires `[auth]` to be configured.** If `[auth]` is absent, `/api/observers/*`, `/runtime/health`, and `/runtime/reload` are not mounted (with a `WARN` log at startup) rather than mounted open. Any internal tooling that called these endpoints unauthenticated must now present a valid bearer token. Reverse-proxy auth (mTLS or a bearer-token gate) is no longer the only line of defence.

- **`DatabaseAdapter::set_session_variables` has been removed (#329).** It applied `set_config(..., true)` on a pooled connection in its own autocommit transaction — transaction-local *and* on a different connection than the subsequent operation — so it never reached the operation (the bug this release fixes), and the executor no longer calls it. Custom `DatabaseAdapter` implementors that overrode it should delete the override; any direct caller should switch to the connection-affine `*_with_session` methods (`execute_function_call_with_session`, `execute_where_query_arc_with_session`, `execute_with_projection_arc_with_session`, `execute_parameterized_aggregate_with_session`, and `RelayDatabaseAdapter::execute_relay_page_with_session`), which apply session variables on the same connection as the operation.

- **Mutation response shape now matches the query contract (#410, #400).** Mutation success and error responses are projected by the same engine as queries, which changes three things for clients that relied on the old behaviour. (1) **`__typename` is returned only when selected** — it is no longer auto-injected into every mutation response; add `__typename` to your selection set if you depend on it (this matches the GraphQL spec and the query path). (2) **Nested typed-object fields are now projected** — previously dropped or returned as a verbatim sub-blob, they are now recased and subset to the selection, so clients that hand-rolled per-mutation key recasing must drop it or they will double-convert. (3) **Nested fields inside both success and error responses are now subset to the selection** rather than returned in full. There is no compatibility flag — fix-forward. The `Executor::execute_mutation` signature also changed its last parameter from `&HashMap<String, Vec<String>>` (flattened per-type field names) to `&[FieldSelection]` (the result selection set); pass `&[]` for no field filtering.

### Documentation

- **Added a FreeBSD deployment guide (#148):** `docs/guides/freebsd-deployment.md`
  walks operators through the Jails + ZFS + Caddy stack — building or
  cross-compiling the binary, a two-Jail (API + network-isolated DB) layout
  with a nullfs-mounted Postgres Unix socket, ZFS-clone multi-tenancy, and a
  per-feature FreeBSD support/limitations table.

- **Documented the federation-subgraph pattern for non-SQL mutations (#170).**
  Operations that can't be expressed as PL/pgSQL (AI/ML, payments, external
  services, long-running jobs) are handled with a federation subgraph rather
  than runtime async handlers in core. ADR-0010 is marked **Rejected** with the
  rationale and alternatives considered; a decision guide
  (`docs/guides/non-sql-mutations.md`) covers when to use SQL vs federation vs
  neither; and a runnable example (`examples/async-jobs-subgraph/`) ships a
  self-contained Rust + `async-graphql` subgraph composed alongside a FraiseQL
  schema. Docs and a new example crate only — no engine changes.

### Known limitations

The docs-overhaul audit on 2026-05-29/30 surfaced the following issues
that are **NOT fixed** in this release and remain open for triage. Pin
your usage accordingly:

**Silent-no-op TOML wiring (config looks honored but isn't):**

- #330 — multi-tenant runtime not wired into the `fraiseql-server` binary
- #334 — `[storage.<name>]` / `[files.<name>]` not auto-wired by the binary
- #340 — observer `/runtime/*` mounted at root instead of `/api/observers/runtime/*`
- #341 — DLQ subcommands return hard-coded mock JSON instead of reading the runtime DLQ
- #342 — `[observers]` TOML schema diverges between `fraiseql-cli` and `fraiseql-server`
- #350 — `FRAISEQL_OBSERVER_TRANSPORT` ignored even with `observers-nats`
- #356 — `failed_login_max_attempts` / `failed_login_lockout_secs` dropped by server runtime
- #357 — `[security.token_revocation] backend = "postgres"` silently downgraded to in-memory
- #360 — PKCE routes mount without `[security.state_encryption]` (warn-and-continue, not refuse)
- #361 — JWKS hot-rotate stolen-key replay window: `detect_key_rotation` only warns

**Functional bugs:**

- #331 — WebSocket subscription endpoint drops JWT `tenant_id`
- #332 — suspended tenant returns 403, not 503 + `Retry-After: 60`
- #333 — tenancy header validator and schema-mode validator disagree on tenant-key shape
- #336 — storage bucket name dropped before backend call — cross-bucket key collisions
- #337 — storage stored XSS surface (uploads with attacker `Content-Type`, no `nosniff`)
- #338 — global 1 MB `DefaultBodyLimit` silently caps every storage upload
- #339 — LIKE-pattern injection in `StorageMetadataRepo::list` prefix arg
- #343 — `InMemoryDlq` is unbounded; documented `max_dlq_size` cap silently ignored
- #344 — DLQ retry handlers race; concurrent retries double-fire the webhook
- #345 — webhook payloads are not signed; receivers cannot detect forged events
- #349 — `ActionConfig::Email` observers report success without sending email
- #270 — additional follow-up tracking (see GitHub for details)

These will be addressed in 2.4.x / 2.5.0; tracking on GitHub.
A follow-up runbook with per-issue fix shapes lives at
`/tmp/fraiseql-deferred-bugs-2026-05-30/runbook.md` (local) for the
next agent to pick up cold.

### Known follow-ups (#329)

- Relay `node(id:)` lookups, partial-period aggregate UNION branches, and gRPC
  mutations do not yet thread a `SecurityContext`/session-variable config, so
  `current_setting()`-backed RLS is not configured on those paths. Each call
  site is annotated in the source.

## [2.3.2] - 2026-05-28

### Fixed

- **`cargo publish` for `fraiseql-server`, `fraiseql-cli`, and `fraiseql` (umbrella)** — `crates/fraiseql-server/build.rs` ran `npm install` and `npm run build` inside `crates/fraiseql-server/studio/`, populating `studio/node_modules/` (~45 MB) and `studio/dist/` during cargo's verify step. Cargo correctly flagged this as `Source directory was modified by build.rs during cargo publish` and refused to publish on the v2.3.1 release run (CI run 26516845920). `build.rs` now stages the Studio package into `$OUT_DIR/studio/` and runs npm/esbuild there, so the source tree is no longer touched. The `[package].exclude` for `studio/{node_modules,dist,.cache,.npm,*.log}` is added as defensive insurance against stale on-disk copies leaking into the `.crate` tarball. **This is the first v2.3.x release where `cargo install fraiseql-server` actually works** — v2.3.0 and v2.3.1 were tagged but never published successfully via automation. (Crates.io's `fraiseql-server@2.3.0` was published manually outside the workflow.)

- **`fraiseql-functions` and `fraiseql-storage` missing from release automation** — both crates are publishable (`publish = true` / unset) and `fraiseql-functions` is a mandatory dep of `fraiseql-server`, but neither appeared in `release.yml`'s publish job. Both are now published alongside the other 13 crates on tag push, in correct topological position (`fraiseql-storage` in Tier 2, `fraiseql-functions` in a new Tier 6.5 between observers and server).

### Changed

- **`release.yml` validate-release dry-run now covers every publishable crate** (15 total) instead of only `fraiseql-error`. The packaging-rules failure that blocked v2.3.0 and v2.3.1 publish for `fraiseql-server` would have been caught here. Timeout bumped to 30 minutes to accommodate the longer loop.

### Migration

Consumers currently pinned to `fraiseql-server = "2.3.1"`, `fraiseql-cli = "2.3.1"`, or `fraiseql = "2.3.1"` will get a `cargo install` failure (those versions are not on crates.io). Upgrade to `2.3.2`. Pins to `fraiseql-server = "2.3.0"` continue to resolve but are missing the #316 axum-0.8 startup-panic fix.

## [2.3.1] - 2026-05-27

### Fixed

- **Server panic at startup on observer router mount** (#316, #317) — the axum 0.7 → 0.8 migration left one path-capture literal at the old `:listener_id` syntax (`crates/fraiseql-server/src/observers/routes.rs:128`, `/checkpoint/:listener_id`). axum 0.8 hard-panics at `Router::route` build time on the old syntax, so any deployment that mounted the observer changelog router crashed before binding the listener. The literal is now `{listener_id}` and the panic site is gone.

### Added

- **Router-construction tests** (#317) — `observer_routes`, `observer_runtime_routes`, `observer_dlq_routes`, `observer_changelog_routes`, and `rbac_management_router` each have a `#[tokio::test]` that constructs the router (see `crates/fraiseql-server/src/observers/tests.rs::router_construction` and `crates/fraiseql-server/src/api/rbac_management/tests.rs::router_construction`). axum's path-capture validation runs inside `Router::route`, so the same bug class would now surface in `cargo test`, not at first server boot.

- **`axum-route-syntax-check` CI gate** (#317) — `tools/check-route-syntax.sh` greps for `:param` literals inside `.route(...)` calls across `crates/` and `examples/`. Combines a single-line regex with a load-bearing multi-line `awk` pass that catches `.route(\n  "...",\n  handler\n)` calls (the v2.3.0 bug literal was invisible to a single-line regex). Wired as a job in `.github/workflows/ci.yml`; `make lint-routes` runs it locally.

- **`release-smoke` workflow** (#317) — `.github/workflows/release-smoke.yml` boots `fraiseql-server` (release profile) against the `docker/e2e/` fixtures on `release/*` branches and `v*` tags and asserts `/health` responds within ~30s. Catch-all for the "code compiles, server panics on boot" bug class — covers every router constructor the binary actually mounts, not just the ones unit-tested individually.

## [2.3.0] - 2026-05-25

*v2.3.0 supersedes the abandoned 2026-05-14 release attempt — see commit history for the revival. Migration guide for adopters: `docs/migration/v2.2-to-v2.3.md`.*

### Added

- **LTree ID-based operators** (#250) — `descendantOfId` and `ancestorOfId` WHERE operators
  that resolve an entity's ltree path from its UUID before performing hierarchical comparisons.
  Supports self-referencing hierarchies (`path <@ (SELECT path FROM t WHERE id = $1)`) and
  cross-table hierarchies via FK semi-joins. Configured via `[hierarchies]` in `fraiseql.toml`
  with `table` and `path_column` settings. Includes field-level `hierarchy` annotation and
  compile-time validation. PostgreSQL-only (MySQL/SQLite/SQL Server return `Unsupported`).
  (`de05e4252`, `91d92f376`, `b83ca0957`, `8ec7c7617`, `229542276`, `a8d638dc9`, `2be493440`, `3ae032a1d`)

- **JWT nested claims extraction** (#246) — `Claims::email()` and `Claims::name()` accessor
  methods that normalize nested JWT claim formats (Azure AD `{"value": "..."}`, OIDC
  `{"given": "...", "family": "..."}`, arrays) into flat strings. `GET /auth/me` now
  returns top-level `email` and `display_name` fields, and RLS session variables support
  `jwt:email` and `jwt:name`/`jwt:display_name` mappings.
  (`75fbd24be`, `cccb19fc7`, `f012f2e03`, `06a03ba28`)

- **Partial-period aggregates** — UNION ALL dispatch for aggregate queries spanning period
  boundaries, with `TemporalGrain` and `PartialPeriodConfig` schema model additions and
  lower-bound date extraction from WHERE clauses. (`727b68829`, `784a09f89`, `773029355`,
  `bd25bf471`, `6d683dbd8`, `91ac77ab7`)

- **Storage API** (`fraiseql-storage` crate) — S3/local/Azure/GCS storage backends with
  RLS-enforced tenant isolation, file transforms (resize, watermark, format conversion),
  and access control routes mounted on the server. Ported from the Phase 8 platform
  integration; see Phase 12 in the roadmap. (`00ddccb83`, `3fb958715`)

- **Functions trigger system** (`fraiseql-functions`) — `after:mutation`, `before:mutation`,
  `after:storage`, cron, and HTTP trigger types with a `TriggerRegistry` for dispatch.
  WASM host bindings for function execution, WASI support, host op wiring with `SqlExecutor`
  injection, sandbox + concurrency limiter, function secrets (AES-256-GCM), and WASM module
  cache for cold-start optimization. (`11d0e3442`, `db0b65166`, `de162ed9d`, `9c6aaecba`,
  `88d8fc040`, `aa23821d2`, `d36cf1bfb`, `f462fada3`, `37a563fc3`, `6743ad290`, `a76b3e747`,
  `d228dc05e`, `18a310661`)

- **Realtime subsystem** — WebSocket server with subscription protocol, event delivery
  with RLS, broadcast observer, `CronScheduler` for periodic tasks, presence manager with
  room tracking and heartbeat eviction, broadcast channels with REST publish endpoint, and
  CDC `ObserverRuntime` wired into `EventBridge`. Tenant-aware CDC filtering via
  `fk_customer_org`. (`f6dd7e419`, `8b0e78402`, `ed23497bc`, `6ca949577`, `dde8e41f1`,
  `aded85a27`, `4d9639fc8`)

- **Subsystems builder** — `ServerSubsystems` builder pattern with `ExtendedCompiledSchema`
  loader and config validation for composing server capabilities. (`aded85a27`)

- **Auth extensions** (Phase 13) — unified multi-provider social login (Google, GitHub, Apple,
  Microsoft), account linking (same email → same user), magic links / email OTP, TOTP MFA
  with recovery codes, anonymous session signup, and phone-auth SMS OTP. (`b7fb91413`,
  `cd5c594f4`, `d57036537`, `a88b69a19`, `d4879ca6a`, `97a554b81`, `41791f0a0`)

- **Tenancy hardening** (Phase 15) — `TenancyConfig` and `TenancyMode` plumbing, compile-time
  `@tenant_id` row-isolation guard, schema-isolation DDL and `search_path` management,
  suspend/resume lifecycle with admin scope guard, tenant-aware rate limiting and quotas,
  tenant audit trail, and tenant cross-source consistency validation. (`aec9753ff`,
  `6808942ed`, `ed14d8f50`, `c21f78a6f`, `0c2fb55c7`, `9b1fe5c56`, `d1fa0d089`, `8675b43b3`)

- **Schema migrations CLI** (Phase 14) — schema migrations & evolution support via
  `fraiseql-cli`. (`1158be090`)

- **Studio admin dashboard** (Phase 18) — SPA shell with embedded assets at `/studio`,
  admin API schema + health endpoints, data browser backend, auth/storage/realtime/functions/
  metrics backend endpoints, frontend wired to all admin API endpoints. (`6b66e56ad`,
  `0768881a6`, `f4838058a`, `84e6cca47`, `3d2039890`, `53ebbd18a`)

- **Studio metrics endpoint** — `GET /admin/v1/metrics/summary` wired to live
  `MetricsCollector` with real-time latency percentiles and cache hit rate.

- **CLI `setup` command** — generates mutation helper functions (`mutation_response` type,
  `fn_mutation_success` / `fn_mutation_error` SQL functions). (`1c3497e9e`)

- **Observer management** — changelog handlers, DLQ handlers, and shared DLQ state
  across hot-reload cycles. (`3b04c3241`)

- **`DatabaseAdapter::on_schema_reload()`** — adapters react to schema hot-reload
  events (e.g. clear caches). Default no-op for backwards compatibility.

- **PostgreSQL usage persistence backend** — `UsageAggregator` stores mutation counters
  in `fraiseql_usage_counters` table with automatic background flush lifecycle.
  (`5bf080663`, `a0ddffa03`)

- **`[usage]` TOML configuration section** — `ServerConfig.usage: Option<UsagePersistenceConfig>`.

- **REST transport wiring** — `[rest]` TOML section now parsed and compiled
  through the full pipeline (merger → intermediate → compiled schema). Server
  mounts read-only REST query router behind `rest` feature flag. Based on
  PR #229 by @magick93. (`bd98715e4`, `d97924802`, `fe6456854`)

- **Admin query-stats endpoints** (#268) — cross-database query performance
  observability via `GET /api/v1/admin/query-stats`, `GET .../query-stats/{queryid}`,
  and `POST .../query-stats/reset`. Backed by `pg_stat_statements` (PostgreSQL),
  `performance_schema` (MySQL), and `sys.dm_exec_query_stats` (SQL Server). Graceful
  no-op on SQLite. Prometheus gauges: `fraiseql_db_query_exec_seconds`,
  `fraiseql_db_query_calls`, `fraiseql_db_query_mean_exec_seconds`,
  `fraiseql_db_cache_hit_ratio`. Grafana dashboard panel added. (`2f6104d99`, `deb586efb`,
  `396ab5508`, `38562a0d3`, `1cfae166a`)

- **Native aggregation column support** — `native_measures` for flat column
  aggregation without JSONB extraction, and `native_dimension_mapping` for
  GROUP BY column resolution on views with native SQL columns. (`95db4f9b9`, `f7245960e`)

- **Wire protocol network operators** — `isMulticast`, `isLinkLocal`,
  `isDocumentation`, `isCarrierGrade` network filter operators; `isPrivate` / `isPublic`
  consolidated into boolean-value pattern. (`20bb709f3`, `3f4bcfc63`)

- **camelCase operator normalization** — WHERE clause operator names now accept
  camelCase form (e.g. `startsWith`) and normalize to snake_case internally. (`37dc02312`)

- **Independent admin-route auth toggles** — `metadata_require_auth`,
  `schema_export_require_auth`, `playground_require_auth`, and `subscription_require_auth`
  config options decouple each admin/inspection surface from the global `require_auth`
  default. (`02081b700`, `c3286bb60`, `c2f8304ed`, `fdba1d06c`)

- **Federation mTLS** — defence-in-depth mTLS support for federation subgraph connections.
  (`0e5175371`)

- **Schema integrity** — SHA-256 content hash wired into `schema.compiled.json` for
  startup-time integrity verification. (`a27d8f1c5`)

- **Cargo-fuzz target for wire JSON parse path** — covers every variable/row JSON payload
  reaching the engine. [F030] (`2763ca296`)

- **Property tests for runtime entry points** — 9 property tests covering `parse_query`,
  `QueryMatcher::match_query`, and `extract_root_field_names`. [F031] (`fcee0374b`)

- **Crate-level READMEs** — 16 workspace crates now declare `readme = "README.md"` so
  crates.io and docs.rs landing pages render the overview. Three missing READMEs added
  (`fraiseql-functions`, `fraiseql-storage`, `fraiseql-test-utils`). [F032]
  (`7fd709d97`, `494bf086a`, `d69d1fdbc`, `9cb46eccf`)

### Security

- **S33**: auth input caps + `reload_schema` path-traversal guard. (`5f0e76806`)
- **S34**: resource bounds on auth flows. (`2b11e0371`)
- **S35**: quality & observability polish on the auth path. (`ff09fd270`)
- **S36**: session security hardening. (`694b74b56`)
- **S37**: PKCE hardening. (`2aaf5cd89`)
- **S38**: SCRAM / auth key-material zeroization. (`6e476c46a`, `4f9fad1e1`)
- **S39**: redirect URI and auth-code input hardening. (`1059d0368`)
- **S40**: JWT claims hardening. (`9a8a31c15`)
- **S41**: JWT algorithm hardening. (`e123528b6`)
- **S42**: JWT header injection defence. (`b26bfd523`, `5f4265eae`)
- **S43**: IPv6 literal parsing in wire connection strings (RFC 3986 bracket notation).
  (`39b625a89`)
- **S44**: Federation saga table double-prefix fix (`tb_tb_` → `tb_`) + `cleanup_all`
  visibility restriction. (`57c15b286`)
- **S45–S48**: real peer-IP forwarding via `PeerIp` extractor for GraphQL rate limiting,
  `AuthorizationDenied` audit event for SOC 2 compliance logging, Vault backend rotation
  atomicity with per-secret `DashMap` locks, and admin bearer-token brute-force protection.
  (`4e3b680c3`)
- **Vault hardening** — body-size guards and `Debug` redaction on the secrets backend.
  (`17cf97a96`)
- **Cache RLS isolation guard** — additional guard ensuring cache lookups cannot
  cross-leak between security contexts. (`226d0de36`)
- **Subscription tenant isolation** — WebSocket subscriptions now enforce tenant
  isolation end-to-end. (`9639fd894`)
- **HTTP allowlist defaults** — `fraiseql-functions` outbound HTTP now denies by default;
  hosts must be explicitly allowlisted. (`f49885cbf`)
- **RLS enforcement on aggregate/window paths** — closes a gap where aggregate and
  window queries could bypass row-level security. (`f7d5e77a8`)
- **Redact bearer token in `AuthRequest` Debug output.** [F010] — manual `Debug`
  emits `Some("<redacted>")` / `None`. (`1dbf83119`)
- **Redact tokens in `AuthCallbackResponse` / `AuthRefreshResponse` Debug.** [F045]
  (`47c478768`)
- **Zeroize `Secret` buffer on drop.** [F012] — `Secret`'s `Drop` impl now scrubs the
  underlying heap allocation; previously `Debug` was redacted but the plaintext lingered
  in freed pages. (`eda6db593`)

### Fixed

- **Hot-reload cache rebind** — query cache cleared on schema reload, resolving a
  stale-cache bug.
- **fraiseql-storage compile errors** — corrected compile-time failures from the v2.2.0
  federation work.
- **`platform_e2e_test` repaired** — 9 platform E2E tests pass reliably after a race
  condition fix.
- **OIDC enrichment compatibility** — works without the observers feature enabled.
- **CLI SBOM metadata** — falls back to workspace `Cargo.toml` when crate-level
  metadata is unavailable. (`b7486e794`)
- **3 broken doctests in `traits.rs` and `PostgresAdapter`** — repaired. (`185822222`)
- **Federation HTTP retry source chain** — `execute_with_retry` now threads the most recent
  `reqwest::Error` into `FraiseQLError::Internal { source }` instead of stringifying it
  away. [F025] (`500859a48`)
- **Observer job-worker panics propagated** — `execute_batch` now logs panics at `error!`
  with `worker` and `error` fields and increments `fraiseql_observer_job_failed_total`
  (when the metrics feature is enabled). [F014] (`d1c89be6e`)
- **Cron task error chain logged** — cron-task error log now adds `error.debug` and
  `error.chain` fields walking `std::error::Error::source()`. [F047] (`7f99fe498`)
- **Response-cache key serialization errors propagated** — `compute_response_cache_key`
  now returns `Result<u64>` and bubbles serialization failures as `Validation` errors
  instead of `unwrap_or_default()` colliding distinct argument trees onto the empty-string
  key. [F044] (`cf3a202cd`)
- **Per-query execution log demoted from `info` to `debug`.** [F041] (`ef8bc4119`)
- **`FraiseQLError` doctest references** — rewritten to enumerate three real variants
  (`Parse`, `Validation`, `Database`) with a `#[non_exhaustive]` explanatory comment.
  [F016] (`bc9df7dc2`)
- **`IntoResponse for FraiseQLError` catch-all arm** — `into_response`, `status_code`, and
  `error_code` matches now carry a documented catch-all arm so a future
  `#[non_exhaustive]` variant addition defaults to a safe generic 500 rather than failing
  to compile silently. [F055] (`39078b202`)
- **`Auth` / `Webhook` / `Observer` source-chain preservation** — `#[source]` annotation
  added to the three boxed-payload variants so `err.source()` walks the subsystem-error
  chain instead of returning `None`. [F049] (`bc0ed8e25`)
- **`FraiseQLError::Storage` ownership rustdoc** (later collapsed by the F050 deletion).
  [F051] (`686322bd6`)
- **OAuth/token race conditions in tests** — drain tokio task before cancel in token-refresh
  and lease-renewal tests. (`379919faa`, `faca53b82`)

### Changed (breaking)

- **Error taxonomy consolidation** — `FraiseQLError` is now the single root error type for
  the workspace. The parallel HTTP-shaped `RuntimeError` enum has been deleted from
  `fraiseql-error`, along with five vestigial shadow domain enums
  (`fraiseql_error::{AuthError, WebhookError, NotificationError, IntegrationError,
  ObserverError}`) that had zero production call sites. Subsystem error vocabularies
  (`fraiseql_auth::AuthError`, `fraiseql_webhooks::WebhookError`,
  `fraiseql_observers::ObserverError`) now compose into the canonical taxonomy via owned
  `From<X> for FraiseQLError` impls (sqlx pattern); the new variants are
  `FraiseQLError::{Auth, Webhook, Observer, File}`. `FileError` itself is retained (9
  production call sites) and is now a `#[from]` variant of `FraiseQLError`. The
  `impl IntoResponse` in `fraiseql_error::http` now wraps `FraiseQLError` directly
  (was: `RuntimeError`), and `IntoHttpResponse` bridges `Result<T, FraiseQLError>`. The
  umbrella crate `fraiseql` no longer re-exports `RuntimeError`, `AuthError`, or
  `WebhookError`; use `FraiseQLError` (via `fraiseql::FraiseQLError` or
  `fraiseql::prelude::*`) instead. (`ffd3124e9`, `dd1c9b80f`, `230d4d238`)
  **Migration:** see `docs/migration/v2.2-to-v2.3.md` and `DEPRECATIONS.md`.

- **`ServerError::RuntimeError` renamed to `ServerError::Engine`** — the variant wraps
  `fraiseql_core::error::FraiseQLError` (the engine error), not the now-deleted
  `fraiseql_error::RuntimeError`. The old name was a misnomer. The `#[from]` semantics
  are unchanged: any `FraiseQLError` bubbles up as `ServerError::Engine` automatically.
  (`65491c2a9`)
  **Migration:** `sed -i 's/ServerError::RuntimeError/ServerError::Engine/g' **/*.rs`.

- **`FraiseQLError::Storage` removed; storage failures now use
  `FraiseQLError::File(FileError::*)`** [F050]. The 118 call sites in `fraiseql-storage`
  and `fraiseql-functions` that used to construct `FraiseQLError::Storage { message, code }`
  have been migrated to typed `FileError` variants, eliminating the `code: Option<String>`
  string-discriminator anti-pattern. Eight new `FileError` variants cover the
  backend-classification space:

  | New variant | HTTP status | Replaces |
  |---|---|---|
  | `FileError::PermissionDenied { message, source }` | 403 | `Storage { code: Some("permission_denied") }` |
  | `FileError::IoError { message, source }` | 500 | `Storage { code: Some("io_error") }` |
  | `FileError::InvalidKey { message }` | 400 | `Storage { code: Some("invalid_key") }` |
  | `FileError::NotImplemented { message }` | 500 | `Storage { code: Some("not_implemented") }` |
  | `FileError::Unsupported { message }` | 500 | `Storage { code: Some("not_supported"/"unsupported") }` |
  | `FileError::SizeLimitExceeded { message, limit, actual }` | 500 | `Storage { code: Some("size_limit_exceeded") }` |
  | `FileError::MimeTypeNotAllowed { message, mime }` | 500 | `Storage { code: Some("mime_type_not_allowed") }` |
  | `FileError::Backend { message, source }` | 500 | catch-all for `Storage { code: None }` (~67 sites: HTTP / SDK failures, config-validation errors, sqlx database errors) |

  Existing `FileError::NotFound` reused for `Storage { code: Some("not_found") }`.
  **Observable HTTP changes** (two refinements):
  1. `FraiseQLError::File(FileError::NotFound)` now returns 404 globally (was 400). This
     aligns the global status code with what the local `storage_error_response` and
     `fraiseql-server::file_error_response` routes already returned for backend
     not-found cases.
  2. `FraiseQLError::File(FileError::InvalidKey)` returns 400 (was 500 under
     `Storage { code: Some("invalid_key") }`). The previous 500 was a bug: a
     caller-supplied bad key is user-fixable and 400 is the semantically correct status.

  Every other status code is preserved: `storage_error_response` still routes
  `NotFound` → 404, `PermissionDenied` → 403, everything else → 500 exactly as before,
  only by matching on typed variants instead of the `code` string. Source-chain
  preservation is a net improvement: reqwest, AWS SDK, sqlx, std::io errors that were
  previously stringified via `format!("backend error: {e}")` now flow through
  `source: Some(Box::new(e))` so `Error::source()` chain walkers and `tracing`'s
  error-chain instrumentation see the underlying type.
  (`4c86d2e0d`, `ed80df821`, `aa7d59712`, `44432234f`, `acec7e435`, `76288f3ab`)
  **Migration:** downstream callers that matched on `FraiseQLError::Storage { .. }`
  must migrate to `FraiseQLError::File(FileError::*)`. See `docs/migration/v2.2-to-v2.3.md`
  for the `code`-string-to-variant table.

- **`ViewName(Arc<str>)` newtype propagated through cache invalidation APIs** [F028, F037] —
  `DatabaseAdapter::invalidate_views`, `DatabaseAdapter::invalidate_list_queries`,
  `QueryResultCache::invalidate_views`, `QueryResultCache::invalidate_list_queries`,
  `ResponseCache::invalidate_views`, and `CachedDatabaseAdapter::invalidate_views` now
  take `&[ViewName]` instead of `&[String]`. Cache internal storage (`accessed_views`,
  `view_index`, `list_index`) migrated accordingly. View names are now promoted from
  `String` to `Arc<str>` once at the `put` boundary and reused across every reference,
  reducing per-cache-write allocations. (`4bf9a58b1`, `e760033ce`)
  **Migration:** adopters with custom adapter impls update the trait method signatures;
  `ViewName::from(&str)` is a one-line conversion at the call site.

- **`execute_with_projection_arc` takes `&ProjectionRequest<'_>` instead of 6 positional
  arguments** [F043] — adapter trait method signature consolidated into a borrowed struct
  with field order mirroring `SELECT … FROM … WHERE … ORDER BY … LIMIT … OFFSET`. The
  struct is intentionally NOT `#[non_exhaustive]` (a missing field is a hard compile error
  by design). (`83725aed8`)
  **Migration:** override the trait method by constructing a struct literal.

- **`KeyedRateLimiter` is generic over `<C: Clock = SystemClock>`** [F018] — the boxed
  `Box<dyn Fn() -> u64 + Send + Sync>` clock has been replaced with a `Clock` trait. A
  blanket impl on `F: Fn() -> u64 + Send + Sync` keeps closure ergonomics for tests, and
  `SystemClock` is a zero-sized type so default-clock production limiters are now `Clone`.
  (`3dca6bd67`)
  **Migration:** code naming the type explicitly (`KeyedRateLimiter` in a struct field)
  may need `KeyedRateLimiter<SystemClock>` to type-check.

- **`extract_root_field_names` returns `impl Iterator<Item = &str>` instead of `Vec<&str>`**
  [F020]. (`dffa25762`)
  **Migration:** add `.collect::<Vec<_>>()` at the two call sites that need a `Vec`.

- **`InMemoryRateLimiter`, `TrustedDocumentStore`, `KeyedRateLimiter`, federation
  `ConnectionManager`, and observer `entity_type_index` migrated to lock-free reads**
  [F006, F007, F008, F013, F048]. All five maps were previously `Arc<Mutex<HashMap>>`
  or `Arc<RwLock<HashMap>>` on read-hot paths and now use `DashMap` (four of them) or
  `ArcSwap<HashMap>` (the observer index, F056) so request-hot reads no longer block on
  a central lock. Per-key atomicity is preserved via `DashMap::entry()` where the
  previous code held the outer lock across a read-modify-write. The
  `TrustedDocumentStore::resolve` / `document_count` / `replace_documents` methods drop
  their `async` signature (no remaining await suspension). The two stricter contracts
  are also restored:
  - Observer `entity_type_index` (F056) uses `ArcSwap<HashMap>` for **snapshot
    atomicity** — readers always observe a fully-populated generation, never a
    partially-rebuilt index during reload.
  - `KeyedRateLimiter` (F057) enforces its `max_entries` cap **strictly** on the
    insert path under a serialising guard — `len()` never exceeds the cap at any
    observable instant, even under sustained concurrent burst.

  The remaining four maps (F006, F007, F008, F013) use plain `DashMap` and document
  per-key best-effort atomicity in the field rustdoc; these are correct under their
  stated contracts. (`c5c946fb3`, `4b3e542b3`, `6f79c711e`, `3cda8124f`, `1ebae1f61`)
  **Migration:** none for callers; behaviour change is internal.

- **`parking_lot::Mutex` replaces `tokio::sync::Mutex` for synchronous critical
  sections** [F019] — `MemoryApqStorage::entries` and
  `ListenerHandle::last_heartbeat` switched to `parking_lot::Mutex<HashMap<…>>` and
  `parking_lot::Mutex<Instant>`. `ListenerHandle::update_heartbeat` is no longer
  `async`. Three sites that hold their lock across `.await` were intentionally left on
  `tokio::sync::Mutex`. (`bb95ef8e9`)
  **Migration:** none unless calling `update_heartbeat` directly — drop the `.await`.

- **Lifecycle `tokio::spawn` tracked via `JoinSet`** [F021] — server lifecycle spawns
  (SIGUSR1 handler, usage-persistence flush, Arrow Flight gRPC server, trusted-docs
  reloader, PKCE cleanup) are now collected into a per-server `tokio::task::JoinSet`
  that `serve_with_shutdown` aborts and drains under the configured shutdown timeout.
  Per-request spawns (subscription event handlers, request middleware) are NOT migrated.
  (`19bfd826c`)
  **Migration:** none for downstream callers; shutdown behaviour is observably more
  graceful.

- **`MetricsCollector` counters flattened to bare `AtomicU64`** [F009] — 28 individual
  `Arc<AtomicU64>` fields replaced with plain `AtomicU64`. `MetricsCollector` no
  longer derives `Clone`; production wiring already wraps in `Arc<MetricsCollector>`.
  Call-site syntax (`metrics.queries_total.fetch_add(…)`) is unchanged. (`f5ddaa59e`)
  **Migration:** any code holding `Arc::clone(&metrics.queries_total)` becomes a
  borrow of the parent `Arc<MetricsCollector>`.

- **Arrow Flight multi-batch responses streamed via bounded `mpsc::channel(4)`** [F011]
  — 4 multi-batch `service.rs` sites converted to a producer task feeding a
  `tokio_stream::wrappers::ReceiverStream` so the consumer's `poll_next` exerts
  backpressure on the producer. Single-element response sites stay on
  `stream::iter(vec![one])`. (`0077a3eb1`)
  **Migration:** none for callers; output stream shape preserved.

- **`ParsedQuery.source: String` is now `Arc<str>`** [F042] — `ParsedQuery::clone()`
  drops its deep string copy in favour of an atomic ref-count bump. The wire form of
  the serde representation is unchanged (custom `serialize_with` / `deserialize_with`
  preserves backward-compatible JSON). (`bab30d351`)
  **Migration:** code that reads `parsed.source` and required `&String` semantics may
  need `&*parsed.source` to get `&str`.

- **`QueryMatcher` builds the variables map once per request** [F005, F024] — the
  matcher used to convert variables twice (once for directive evaluation, once for
  `QueryMatch::arguments`). Folded into a single `variables_to_map` conversion.
  (`38c6e705b`)
  **Migration:** internal change — the wider `QueryMatch` borrowed-arguments
  refactor was deferred (lifetime ripple too wide); signatures unchanged.

- **`ValidationRule::Pattern { pattern: String }` → `Pattern { pattern: CompiledPattern }`**
  [F003] — regex compilation now happens once at construction (or at
  `schema.compiled.json` deserialisation) rather than on every validation call.
  Invalid patterns surface at schema load instead of degrading silently per request.
  (`dd4393d06`)
  **Migration:** downstream code constructing `ValidationRule::Pattern` directly must
  build a `CompiledPattern` from the source string; a `From<String>`-style helper is
  provided.

- **`QueryParam`'s `to_sql_param` helper deleted; `as_sql_param_refs` centralises the
  borrow pattern** [F036] — `QueryParam` already implemented `ToSql`; the boxed-dyn
  conversion was redundant. (`c9b599e15`)
  **Migration:** code calling `to_sql_param(&p)` should use the existing borrowed
  pattern `.iter().map(|p| p as &(dyn ToSql + Sync)).collect()` or the new helper
  `as_sql_param_refs(&[QueryParam])`.

- **Wire-crate clippy allows reorganised into groups** [F053] — moved 2 test-bleed
  allows (`unreadable_literal`, `explicit_iter_loop`) into per-module `#![allow]`
  inside `mod tests` blocks; removed 2 no-longer-firing allows from the crate level
  entirely; grouped the remaining 15 crate-level allows under two commented headers
  ("Wire-protocol cast suppressions" and "Crate-wide style preferences"). Added
  `make lint-gate-wire` enforcing both the count cap and "no test-bleed lints at
  crate level". (`897a2188a`)
  **Migration:** none for callers; build / lint shape only.

- **Workspace clippy strictly denies `panic`, `unreachable`, `print_stdout`,
  `print_stderr`, `dbg_macro`, `todo`, `unimplemented`, `mem_forget`,
  `lossy_float_literal`, `semicolon_if_nothing_returned`, `undocumented_unsafe_blocks`,
  and `missing_assert_message`** at the workspace `[lints.clippy]` level. The
  `nursery` and `cargo` lint groups are promoted from `warn` to `deny`. Three crates
  (`fraiseql-error`, `fraiseql-wire`, `fraiseql-storage`) additionally deny
  `clippy::indexing_slicing` at the crate root as the Q4 pilot. Workspace-wide
  `indexing_slicing` rollout is planned across v2.3.x; see `FOLLOW_UPS.md` for the
  per-crate rollout plan (13 crates remaining). Three pilot crates were refactored
  with no API surface change: `fraiseql-error` (`levenshtein_distance` rolling
  buffer), `fraiseql-wire` (private `Cursor<'a>` decoder helper), `fraiseql-storage`
  (`serde_json::Value::get()` + slice-`.get()` patterns). (`bb5347e82`, `ace13741e`,
  `e6567fb98`, `4d2c5d17b`, `0a829c2ff`, `04154688d`, `f20fc7717`, `280ff100c`,
  `cfe739c71`, `e514bbf25`, `4a6c94664`, `3c3e16089`)
  **Migration:** downstream crates that opt into the workspace lint table inherit
  these denials; if any external code triggers them, hoist the allow to the
  offending function or module with a `// Reason:` comment.

- **`CompiledSchema::from_json` takes a `strict_integrity: bool` second argument** —
  the canonical schema-load entry point now accepts a strict-integrity flag that
  rejects schemas whose hash does not match the embedded integrity manifest. Re-exported
  via `fraiseql::CompiledSchema` and `fraiseql_core::prelude::CompiledSchema`.
  **Migration:** existing call sites pass `false` for backward-compatible behaviour
  (`CompiledSchema::from_json(json, false)`); set `true` to opt into the new
  integrity check. Surfaces under the schema-integrity hardening landed in v2.3.

- **`fraiseql_cli::schema::intermediate::operations::IntermediateSqlSourceDispatch`
  and `fraiseql_core::schema::SqlSourceDispatch` removed** — both `pub` structs
  belonged to a schema-shape intermediate that was superseded by the v2.3 dispatch
  model. Adopters using the CLI-as-library to introspect schema intermediates, or
  pattern-matching on `QueryDefinition.sql_source_dispatch`, must migrate to the
  new dispatch types.
  **Migration:** see the schema-compilation overhaul in `docs/architecture/compiler.md`.
  If you depended on the removed types, file an issue describing your use case so
  the equivalent v2.3 entry point can be documented.

- **`fraiseql_core::security::oidc::providers::MeEnrichmentConfig` removed** —
  this `pub` struct used to configure the OIDC `/auth/me` claim-enrichment behaviour
  via the Rust API. The OIDC enrichment refactor in v2.3 replaced it with a TOML-driven
  configuration path; programmatic enrichment configuration is no longer supported.
  **Migration:** move claim-enrichment configuration into `fraiseql.toml` under
  `[auth.oidc.providers.<name>.me_enrichment]`. The TOML schema is documented under
  the Auth extensions Phase 13 entry above.

- **`#[non_exhaustive]` rollout to public DTOs (`RelayPageResult`,
  `SqlProjectionHint`, `OrderByClause`, `ActionResult`, `CacheStatus`, `EventKind`)**
  — six public DTOs received `#[non_exhaustive]` so future field additions don't
  break adopters. Each type also gained a `new(...)` constructor so the struct-literal
  pattern can be replaced mechanically. `RelayPageResult` and `ActionResult` are
  returned by public traits (`RelayDatabaseAdapter`, `ActionExecutor`) downstream
  implementations satisfy — those impls must use the new constructors. (`dbc9e0afc`,
  `e2b9944d2`, `3d8c4bce6`)
  **Migration:** replace struct-literal construction with the typed `new()` constructor:
  `RelayPageResult::new(rows, total_count)`, `SqlProjectionHint::new(database, projection_template, estimated_reduction_percent)`,
  `OrderByClause::new(field, direction)`, `ActionResult::new(...)`. Existing pattern
  matches gain a `_` arm.

### Changed

- **Lock-free read paths across `fraiseql-auth`, `fraiseql-server`,
  `fraiseql-federation`, `fraiseql-core`** — five rate-limiter / store / index maps
  migrated to `DashMap`, removing serialised reads on the request hot path (see the
  five-finding bullet under "Changed (breaking)" for breakdown). Hot-path reads no
  longer block on a central lock under concurrent load. [F006, F007, F008, F013, F048]

- **GraphQL parsing on the request hot path** — the validator no longer re-parses the
  query body; `parse_graphql_document(&str)` is exposed and `RequestValidator::validate_query_doc`
  accepts a pre-parsed `Document<'_, String>`. The HTTP handler parses once and feeds
  the same AST into validation and matching. [F001] (`b94abc592`)

- **Response cache hit returns an `Arc::unwrap_or_clone` instead of a deep clone** of
  the cached JSON value. [F002] (`15fd10a48`)

- **`compute_response_cache_key` uses a reused scratch `Vec<u8>` and
  `serde_json::to_writer`** — per-argument `String` allocations on the cache-key path
  removed; errors propagate as `Validation` instead of silently colliding. [F044, F004]
  (`cf3a202cd`)

- **`extract_root_field_names` returns `impl Iterator`** — one allocation removed per
  call. [F020] (see "Changed (breaking)" entry above for the API shape change)

- **Federation HTTP retry preserves the source chain** on the final error rather than
  stringifying it. [F025] (`500859a48`)

- **Tracing on the response-cache lookup path** — `event = "hit"|"miss"|"disabled"`
  structured fields under target `fraiseql::cache::response`. [F040] (`ec9015e26`)

- **`OnceLock<Regex>` replaced with `LazyLock<Regex>`** in `cache/uuid_extractor.rs`.
  [F027] (`ccd25ee97`)

- **`compute_response_cache_key` and `validate_query` extracted helpers** — pure
  refactors that do not change behaviour but reduce duplication. [F023] (`cf3a24c2e`)

- **Workspace dependency consolidation** — `redis`, `chrono`, `dashmap`, `uuid`, `url`
  moved to `[workspace.dependencies]`; the four per-crate `redis` declarations and
  multiple per-crate raw declarations replaced with `workspace = true`. `dashmap`
  workspace version bumped from `6.0` to `6.1` to match the version the resolver was
  already picking. `fraiseql-functions` `reqwest` declaration aligned with the
  workspace rustls-tls posture (drops native-tls / openssl-sys from the dependency
  tree). [F015, F033, F034] (`8278defdc`, `a0e37c15d`, `23d4a18ea`)

- **`cargo ci` alias and `make ci` target** — chains the strict workspace clippy gate
  with `nextest run --workspace --all-features`. [F035] (`d04068d34`)

- **`mold` linker opt-in documented** — `.cargo/config.linker.example.toml` template
  added; the in-tree `.cargo/config.toml` stays commented for CI compatibility.
  [F022] (`598231ae4`)

- **Cargo production dependencies** — non-breaking bumps across the workspace.
- **GitHub Actions** — checkout v4→v6, setup-java v4→v5, setup-go v5→v6,
  upload-artifact v6→v7, setup-uv v5→v7 across 35 workflow files.
- **Pre-commit hooks** — markdownlint-cli v0.48.0, actionlint v1.7.12,
  `stages: [push]` → `stages: [pre-push]` for pre-commit v4.
- **`UsageAggregator.backend`** upgraded to `RwLock<Arc<dyn UsageBackend>>` for
  runtime backend swapping.
- **`UNSUPPORTED_OPERATION` API error code** now maps to HTTP 501 (Not Implemented)
  instead of 500.
- **CVE-related dependency bumps** — `rmcp` 0.16→1.4 (CVE-2026-42559), fuzz
  `jsonwebtoken` 9→10 (CVE-2026-25537), `thrift` removed from default Parquet build
  (CVE-2026-43868 feature-gated). (`cd81b00b4`, `1ab380f58`, `dc9c88bbe`)
- **Newtype wrappers for domain identifiers** — additional newtypes introduced and
  prelude unified to chain exports across crates. (`e70162117`, `158a46a0d`)
- **Construction patterns standardised** — public DTOs gain `new()` constructors with
  builder support; `#[non_exhaustive]` added to `CacheStatus` and `EventKind`.
  (`dbc9e0afc`, `e2b9944d2`, `3d8c4bce6`)

### Known Limitations Update

- **Pool Pressure Monitor** — confirmed that neither `deadpool-postgres` nor
  `bb8-postgres` (as of 2026-05) support runtime pool resizing. The
  `PoolPressureMonitor` remains in recommendation-only mode.
- **Q4 workspace `indexing_slicing` rollout is in progress** — three pilot crates
  (`fraiseql-error`, `fraiseql-wire`, `fraiseql-storage`) deny the lint at the crate
  root; the remaining 13 crates are scheduled across v2.3.x point releases. See
  `FOLLOW_UPS.md` for the per-crate hit-count table and rollout order.

### Deferred to v2.4

- **`F031` property tests cover no-DB executor entry points only** — the full
  `Executor::execute` end-to-end pipeline (RLS composition, projection, cache
  warm/cold) needs a mock `DatabaseAdapter` and is deferred. See `FOLLOW_UPS.md`.

## [2.2.0] - 2026-05-02

### Fixed

- **Native column support in aggregation `WHERE`, `GROUP BY`, and `ORDER BY`**.
  Aggregation queries on views with both native SQL columns and a JSONB `data` column
  now correctly reference native columns directly (`"col"`) instead of using JSONB
  extraction (`data->>'col'`). This enables btree index usage and fixes the PostgreSQL
  error `column "v_foo.data" must appear in the GROUP BY clause`
  (fraiseql/fraiseql-python#337). All four database dialects are covered.

### Changed (breaking)

- **Mutation response format consolidated** — the versioned `schema_version`
  dispatch has been removed. `app.mutation_response` is now a single canonical
  format with typed, column-per-concern fields (`succeeded`, `state_changed`,
  `error_class`, `entity`, `cascade`, etc.). The old v1 string-status parser,
  the v2 version-dispatch shim, and the `MutationOutcome::Error.status` string
  field are all gone. `MutationOutcome::Error` carries a typed
  `error_class: MutationErrorClass` directly.

  **Why:** FraiseQL has no external consumers yet — we are the sole users.
  Neither v1 nor cascade were ever used in production. Collapsing to a single
  greenfield format removes ~300 lines of dead-weight parsing and version
  negotiation, giving future users a clean starting point with no migration debt.

### Added

- **Multi-tenancy support** — per-tenant executor isolation with lock-free reads.
  Each tenant gets its own compiled schema and database connection, dispatched via
  `X-Tenant-ID` header, JWT `tenant_id` claim, or Host-header domain registry.
  Management API: `PUT/DELETE /api/v1/admin/tenants/{key}` (upsert/remove),
  `GET /api/v1/admin/tenants` (list), `GET /api/v1/admin/tenants/{key}/health`,
  `PUT/DELETE /api/v1/admin/domains/{domain}`, `GET /api/v1/admin/domains`.
  ArcSwap-based hot-reload: in-flight requests complete on the old executor while
  new requests use the updated schema. Single-tenant mode is unaffected (zero overhead
  when multi-tenancy is not configured). Security: explicit-but-unregistered tenant
  keys return 403 Forbidden, never the default tenant's data.

- **Three-state update semantics for CRUD mutations** (#221, `29a2c4da8`).
  Update mutations now distinguish between absent (field not mentioned),
  explicit null (set to NULL), and value (set to new value) via the GraphQL
  variable-omission convention. CRUD naming configuration added to
  `fraiseql.toml`.

- **`computed=True` field marker for CRUD input exclusion** (#222). Python SDK
  (`e6dab114e`), TypeScript (`0ebc702f2`), Java (`e62cf9b86`), C#, Dart,
  Elixir, F#, PHP, Ruby (`ccb9607a4`) SDKs all support `computed` fields that
  are excluded from generated CRUD input types (e.g. `created_at`,
  `updated_at`).

- **`not_found` error status for mutations** (`d6392732d`). Mutation responses
  can now return a `not_found` status distinct from generic failure, enabling
  clients to distinguish "entity does not exist" from other error conditions.

- **Session variables injected before read queries** (#218, `45be17e34`).
  `set_config()` session variable propagation now applies to read queries, not
  only mutations, so RLS policies on SELECT can reference `current_setting()`.

- **Cross-SDK parity CI** (`118bf496d`, `2660603bd`). Cross-SDK generators and
  CI jobs added for Java, Ruby, Dart, C#, F#, Rust, PHP, and Elixir SDKs.

- **Apollo Federation 2 — full directive set** (`d78611a94`). `service_sdl.rs`
  now emits all 7 field-level directives (`@external`, `@requires`, `@provides`,
  `@shareable`, `@inaccessible`, `@override`, `@extends`) with correct `extend type`
  syntax for `is_extends: true` types. `@link` import list is complete. Python and
  TypeScript SDKs expose `FieldConfig(external=, requires=, provides=, shareable=,
  inaccessible=, override_from=)` with validation matching spec rules.

- **Federation constraint validation** — `fraiseql federation check` validates
  `@key` field existence, `@override(from:)` non-empty subgraph name, `@requires`
  target field existence, and `@provides` consistency. Unknown-subgraph overrides
  are reported as errors when `--against` is supplied.

- **Federated subscription passthrough** — `SubscriptionForwarder` proxies
  subscriptions to the owning subgraph via the `graphql-transport-ws` WebSocket
  protocol. SSRF protection applied on all remote URLs. Remote subscription field
  ownership tracked via `remote_subscription_fields` on `FederationMetadata`.

- **Federation plan visualization** — `GET /admin/v1/federation/plan?query=...`
  returns the cached query plan as JSON, enabling gateway debuggability.

- **Prometheus federation metrics** — `fraiseql_federation_subgraph_latency_seconds`
  histogram and `fraiseql_federation_entity_resolution_total` counter wired in
  `fraiseql-federation/src/observability.rs`.

- **Mutation audit tracing** — the runtime emits a structured
  `tracing::info!(target: "fraiseql::mutation_audit", ...)` event after every
  successful mutation, carrying `tenant_id`, `entity_type`, `operation`, and
  `duration_us`. Consumed by the in-process `MutationAuditLayer`.

- **Usage aggregation store** — `MutationAuditLayer` subscribes to audit events
  and maintains per-tenant, per-period, per-entity-type counters in a lock-free
  `DashMap`. Exposed via `GET /api/v1/admin/usage?tenant_id=…&period=…`.

- **Schema metadata endpoint** — `GET /api/v1/schema/metadata` returns the
  compiled schema's version, entity count, query count, mutation count, and
  field-level security metadata (required scopes, deny policy, deprecated status)
  in a stable JSON envelope.

- **`fraiseql schema metadata` CLI subcommand** — prints or JSON-outputs the
  compiled schema's security metadata; `fraiseql federation check --json` flag
  emits structured JSON errors for CI pipelines.

- **Structured CLI error output** — non-zero-exit CLI errors now emit a JSON
  envelope `{"error": "…", "code": "…", "details": {…}}` when `--json` is passed,
  enabling machine-readable CI integration.

### Fixed

- **`inject_params` now respects `native_columns`** (#219, `bdc00905f`).
  Injected parameters (e.g. tenant isolation via `inject: { tenant_id:
  "jwt:org_id" }`) previously always used JSONB extraction
  (`data->>'col' = $N`). When the column exists as a native column on the
  backing view, the query now emits `col = $N::type` instead, enabling
  B-tree index usage.

- **Python SDK CRUD `sql_source` no longer adds spurious `fn_` prefix**
  (`c07e12875`). Auto-generated `sql_source` from `crud=True` mutations
  dropped the `fn_` prefix that was incorrectly prepended.

### Changed

- **Vendored `graphql-parser` removed** (`a9221463c`, `36615f6e1`). The
  in-tree vendored copy and drift tooling have been removed; the workspace
  now depends on the upstream crates.io release.

- **3 patched CVEs removed from `.trivyignore`** (`d85a3822b`).
  CVE-2025-14104 (util-linux), CVE-2025-6141 (ncurses), and CVE-2024-56433
  (shadow-utils) now have Debian fixes; next image rebuild picks them up.

---

## [2.1.6] - 2026-04-14

### Added

- **Session variables via PostgreSQL `set_config()`** (#97). The executor now
  propagates per-request session variables (`user_id`, `tenant_id`, roles, and
  arbitrary custom attributes from `SecurityContext`) into the PostgreSQL session
  via `set_config(name, value, is_local=true)`, so RLS policies and SQL functions
  can read `current_setting('fraiseql.user_id')` etc. without a separate round-trip.
- **Schema naming-convention support for GraphQL operations** (#216). The
  compiler accepts an explicit naming convention (camelCase / snake_case) for
  generated query, mutation, and subscription operation names, so authoring
  languages with different conventions emit a consistent GraphQL surface.
- **Nested relation filters via automatic FK resolution** (#196). Where-clause
  inputs can now traverse foreign-key relations (e.g. `where: { post: { author:
  { name: { eq: "..." } } } }`) and the compiler resolves the join path from
  FK metadata rather than requiring an explicit subquery. `c2ae22ef5` further
  simplifies the nested path to a multi-segment path.
- **HS256 auth mode exposed for integration testing** (#217). Server
  configuration accepts an HS256 shared-secret auth mode alongside the existing
  OIDC/JWKS path, so test harnesses can mint tokens locally without a mock
  identity provider.

### Changed

- **Removed dead Cargo features**: `cors`, `database`, and `rich-filters`
  features that were defined but no longer wired to any code have been removed
  from the workspace.
- **`fraiseql-server` CLI now uses `clap`** (#213). `fraiseql-server` and
  `fraiseql run` share a `ServerArgs` definition; `clap` is feature-gated in
  `fraiseql-cli` so the `fraiseql run` ergonomics are preserved for embedding.
- **`__typename` detection moved to `ResultProjector`** (#212). Detection is
  consolidated at the projection layer and the executor gains a
  `federation_mode` switch so Apollo Federation subgraphs produce
  `__typename`-annotated payloads without duplicated detection logic.
- **`orderBy` SQL generation rewritten as a shared builder** (#211). A shared
  builder fixes a cache-key bug (previously colliding on same fields with
  different directions) and emits type-aware SQL casts so ordering by
  `NUMERIC`/`TIMESTAMPTZ` columns produces correct comparisons.
- **Mutation error projection unified via `ProjectionMapper`** (#215). The two
  divergent mutation-result and error-union projection paths were consolidated
  onto a single mapper; behaviour is preserved but the code path is now shared.

### Fixed

- **Mutation error-union inline fragments, array fields, and selection
  filtering** (#214). Inline fragments on error unions, array fields inside
  mutation payloads, and nested selection filtering all projected incorrectly
  in specific shapes; all three now round-trip through `ProjectionMapper`.
- **`__typename` filtered from SQL projection; `orderBy` snake_case keys
  accepted** (`d9c415fff`). `__typename` is a GraphQL-layer concern and must
  never appear in the SQL SELECT list; `orderBy` now accepts snake_case keys
  in addition to the camelCase form.
- **Issues #206–#209** (`74c9d8d21`): `orderBy` regression on composite types,
  stray `__typename` in SQL, `--config` CLI flag lookup, and array-field
  projection edge cases.
- **Issues #195–#204** (`6a024c3d4`): projection types for scalars behind
  nullable wrappers, camelCase key preservation through the executor, and
  input-object round-tripping in mutation arguments.
- **SDKs: snake_case → camelCase auto-conversion** (`ca9e76b29`). Python, Ruby,
  and Dart authoring SDKs now auto-convert snake_case field names to the
  camelCase form the compiler expects, matching the behaviour of the
  TypeScript and Go SDKs.
- **SDK manifests aligned to 2.1.6**: Dart, Elixir, Go, Java, PHP, Ruby, C#
  (`FraiseQL` + `FraiseQL.Tool`), F#, and Rust authoring SDK version strings
  bumped to match the workspace release.

### Performance

- **Eliminated `serde_json` string round-trip in executor** (#153). All executor
  methods now return `serde_json::Value` directly instead of serializing to `String` and
  immediately deserializing again on every request. Touched 26 files across
  `fraiseql-core`, `fraiseql-server`, and `fraiseql-arrow`.

- **Parsed-query AST cache on `Executor`** (#153). Repeated identical query strings skip
  the full lexer + recursive-descent parse. A lock-free `moka` cache keyed by xxHash64 of
  the query string returns an `Arc<(QueryType, Option<ParsedQuery>)>` in nanoseconds. Only
  successful parses are cached; errors are never stored. Capacity: 1 024 distinct query
  strings.

- **Executor-level response cache** (#156). An optional second cache tier above the
  adapter-level row cache. On a hit, the entire projection + RBAC + envelope-wrapping
  pipeline is skipped — only an `Arc::clone`. Keyed by `(query_hash,
  security_context_hash)`; the security hash covers `user_id`, roles, `tenant_id`, scopes,
  and custom `attributes`, so users never see each other's cached data. View-based
  invalidation via a `DashMap` reverse index (O(k), no full-cache scan). Opt-in via
  `ResponseCacheConfig`; disabled by default.

- **TCP_NODELAY + gated compression on GraphQL route** (#157). Enables `TCP_NODELAY` to
  eliminate Nagle-algorithm buffering on response frames. Adds a `CompressionLayer` to the
  GraphQL and REST routers, gated on `compression_enabled` (see *Changed* below).

### Changed (breaking default)

- **`compression_enabled` now defaults to `false`** (was `true` earlier in this release
  cycle). FraiseQL is overwhelmingly deployed behind a reverse proxy (Nginx, Caddy, cloud
  load balancer) that already handles compression — often with brotli, shared across
  upstreams, and with static-asset caching. Framework-level gzip duplicated that work and
  silently cost 3× RPS on TEXT-heavy GraphQL responses under concurrency. Single-binary /
  no-proxy deployments can opt back in with `compression_enabled = true` in `fraiseql.toml`.
- **Compression now skips responses under 1 KiB** when enabled. tiny payloads (short
  GraphQL results, health responses) pay no compressor overhead.

---

## [2.1.5] - 2026-04-12

### Added

- **`GET /auth/me` session-identity endpoint** (issue #193). Frontends using the PKCE cookie
  flow had no way to ask "who am I?" because the JWT is stored in an `HttpOnly` cookie
  inaccessible to client-side script. The new endpoint reflects a configurable subset of the
  validated session's JWT claims as JSON. Enable opt-in via `[auth.me]` in the compiled
  schema:

  ```toml
  [auth.me]
  enabled = true
  expose_claims = ["email", "tenant_id", "https://myapp.com/role"]
  ```

  The response always includes `sub`, `user_id` (alias for `sub`), and `expires_at`. Extra
  fields are included only when listed in `expose_claims` **and** present in the token —
  absent claims are silently omitted, never `null`-padded. No enrichment callbacks, no
  external calls: the endpoint reads only from the already-validated JWT.

  `oidc_auth_middleware` now also accepts tokens from the `__Host-access_token` cookie as a
  fallback when no `Authorization: Bearer` header is present, enabling the middleware to
  protect the new endpoint in browser flows.

  `AuthenticatedUser` gains an `extra_claims: HashMap<String, serde_json::Value>` field,
  populated by the OIDC validation path from a new `#[serde(flatten)] extra` field on
  `JwtClaims`. Custom OIDC claims (e.g. `"email"`, namespaced URL-form claims) that
  previously fell off the floor during JWT validation are now preserved end-to-end.

### Fixed

- **Input types not recognised as valid mutation argument types** (issue #190). The CLI
  schema converter and validator built their known-type sets from object types, interfaces,
  and scalars but omitted input types. A mutation argument declared as a custom input type
  (e.g. `CreateUserInput`) was incorrectly rejected as an unknown type reference. Both
  `SchemaConverter` and `SchemaValidator` now include input types in the valid-type set.

- **Server did not auto-select relay pagination when schema has relay queries** (issue #191).
  `Server::new` does not enable the Relay cursor pagination runtime; operators had to
  explicitly call `Server::with_relay_pagination`. The binary entrypoint now inspects the
  compiled schema at startup and selects `with_relay_pagination` automatically when any query
  carries `relay: true`.

### Changed

- **Relay cursor doc-comments clarified**: the `encode_edge_cursor`, `encode_uuid_cursor`,
  and `encode_node_id` functions now document that base64 is encoding, not encryption — a
  client that decodes the cursor will see the raw integer PK, UUID, or `TypeName:uuid`
  string. The Relay spec requires cursors to be treated as opaque by convention only; no
  cryptographic guarantee is provided.

---

## [2.1.4] - 2026-04-11

### Added

- **Recursive JSONB sub-field projection via `jsonb_build_object`**. Composite fields with
  a `sub_fields` list now emit a nested `jsonb_build_object(...)` instead of returning the
  full JSONB blob, eliminating over-fetching for deeply nested types. Recursion is capped at
  4 levels; deeper fields and list fields fall back to the full-blob path.
  `ProjectionField` gains a `composite_with_sub_fields` constructor and
  `sub_fields: Option<Vec<ProjectionField>>`.

- **APQ (Automatic Persisted Queries) mutation end-to-end test**. Covers the full
  store-on-miss → retrieve-on-hit cycle for mutations, guarding the APQ cache path that was
  previously untested in integration. Adds ADR-0010 documenting the async mutation handler
  design decision.

- **JWT replay counters exposed on Prometheus `/metrics` endpoint**.
  `fraiseql_jwt_replay_rejected_total` and `fraiseql_jwt_replay_cache_errors_total` are now
  registered as Prometheus counters, completing the observability story for JWT replay
  prevention (plan 01). A flaky test assertion on shared `AtomicU64` counters is also fixed.

### Fixed

- **Stale list queries after UPDATE/DELETE targeting a non-first row** (correctness bug).
  `QueryResultCache::put_arc` previously indexed only `result[0]` in `entity_index`. For a
  list query returning N rows, entities at positions 1…N-1 were invisible to
  `invalidate_by_entity`, leaving the stale list result in cache after a mutation. All rows
  are now indexed.

- **Unnecessary point-lookup eviction on CREATE** (performance bug). CREATE mutations called
  `invalidate_views()`, which evicted every cache entry for the view — including
  single-entity point-lookup entries for existing entities that are completely unaffected by
  the newly created row. CREATE now calls `invalidate_list_queries()`, which evicts only
  multi-row list entries via a dedicated `list_index`. Expected cache hit-rate improvement
  under mixed read+write workloads: ~60–70 % → ~85–95 %.

### Changed

- **`CachedResult` struct**: `entity_ref: Option<(String, String)>` replaced by
  `entity_refs: Box<[(String, String)]>` (one entry per row) and `is_list_query: bool`.
  The `invalidate_by_entity` fast path now short-circuits when the entity type has no
  indexed entries, making write-heavy workloads with no cached reads a near-zero-cost no-op.

---

## [2.1.3] - 2026-04-08

### Performance

- **`QueryResultCache` replaced with `moka` W-TinyLFU** (issue #185). Cache reads are now
  lock-free — eliminates hot-key serialisation under high concurrency. View-based and
  entity-based invalidation use O(k) reverse `DashMap` indexes instead of an O(n) full-cache
  scan. `lru` crate usage in the cache module removed. `CachedResult::entity_ids` replaced
  with `entity_ref: Option<(String, String)>`; `CachedResult::hit_count` removed.

- **`Arc<CachedResult>` in cache store eliminates per-hit deep clone.** The moka store
  type changed from `Cache<u64, CachedResult>` to `Cache<u64, Arc<CachedResult>>`. On a
  cache hit, only one atomic reference-count increment occurs; previously `moka::Cache::get`
  deep-cloned the full `CachedResult` value — including the `Box<[String]>` view list — on
  every read.

- **Zero-allocation cache key generation.** `generate_view_query_key` and
  `generate_projection_query_key` replace the previous `format!` + `serde_json::json!` +
  `generate_cache_key` chain on every cache lookup. Parameters are hashed directly via
  ahash with no intermediate `String` or `serde_json::Value` allocations — zero heap
  activity on the hot read path.

- **Short-circuit when cache is disabled removes per-request overhead.** When
  `cache_enabled = false`, `execute_where_query` and `execute_with_projection` skip the
  64-shard lock scan, `CascadeInvalidator` mutex acquisition, and `is_enabled()` check,
  reducing the disabled-cache overhead to a single branch.

### Changed

- **`Server::new` and `Server::with_relay_pagination` now always wrap the database adapter in `CachedDatabaseAdapter`** (issue #184). When `cache_enabled = false` the adapter acts as a zero-overhead passthrough; when `cache_enabled = true` full query result caching is active.
- **`CacheStatus::RlsGuardOnly` deprecated** — the variant is no longer accurate now that `CachedDatabaseAdapter` is always wired. Admin config endpoint returns `active` when `cache_enabled = true`.
- **Startup log updated** — when `cache_enabled = true` the server now logs `"Query result cache: active"` with `max_entries`, `ttl_seconds`, and `rls_enforcement`; when disabled it logs `"Query result cache: disabled"`.

### Fixed

- **`pool_min_size` now pre-warms the connection pool at startup** (issue #183).
  Previously the parameter was silently dropped (`_min_size`); deadpool would lazily
  open connections on the first request, causing high mutation latency under concurrent
  cold-start load. This was the root cause of the 5.5× mutation throughput gap observed
  in benchmarks. After `Server::new` returns, `pool_min_size` live connections are ready.

- **`pool_timeout_secs` is now applied as the deadpool wait and create timeout** (issue #183).
  Previously the parameter was stored in `ServerConfig` but never forwarded to the pool,
  meaning connection acquisition could block indefinitely on pool exhaustion. With a timeout
  set, pool exhaustion now returns an actionable error within `pool_timeout_secs` seconds
  instead of blocking the request indefinitely.

- **`acquire_connection_with_retry` no longer retries on `PoolError::Timeout`** (issue #183).
  A timeout means the pool was genuinely exhausted for the full wait period; retrying would
  only multiply the wait by `MAX_CONNECTION_RETRIES`. Only transient backend/create errors
  are retried with exponential backoff.

- **`cache_enabled = true` now logs a clear startup message** (issue #183).
  Previously the flag silently had no observable effect on query execution (the full
  `CachedDatabaseAdapter` wire-up is a separate future PR). The server now logs whether
  the RLS safety guard is active, making the current semantics visible to operators.

- **Observer pool no longer inherits application pool size** (issue #183).
  Previously `build_observer_pool` used `pool_min_size` / `pool_max_size` from the
  top-level config. The observer runtime needs far fewer connections (LISTEN/NOTIFY
  - metadata queries). New defaults: `min=2, max=5, acquire_timeout=10s`. Configure
  independently via `[observers.pool]` in `fraiseql.toml` — see `DEPRECATIONS.md`.

### Added

- **`PoolPrewarmConfig` struct** (`fraiseql_db::postgres::PoolPrewarmConfig`) — replaces
  the positional `(min_size, max_size)` arguments on `PostgresAdapter::with_pool_config`.
  Carries `min_size`, `max_size`, and `timeout_secs` in a single self-documenting struct.

- **`CacheStatus` enum** (`fraiseql_server::routes::api::admin::CacheStatus`) with variants
  `Disabled`, `RlsGuardOnly`, `Active`. The admin `/api/v1/admin/config` endpoint now
  includes a `cache_status` field with the serialized enum value.

- **`ObserverPoolConfig` struct** (`fraiseql_server::server_config::ObserverPoolConfig`) for
  independent tuning of the observer's dedicated PostgreSQL pool via `[observers.pool]` in
  `fraiseql.toml`.

- **`pool_timeout_secs = 0` is now a validation error.** A zero-second timeout would cause
  every connection acquisition to fail immediately; the server now rejects this configuration
  at startup with a clear error message.

## [2.1.0] - 2026-03-30

First public release of FraiseQL v2 — a compiled GraphQL execution engine that
transforms schema definitions into optimized SQL at build time.

### Added

#### Core Engine (`fraiseql-core`)

- GraphQL-to-SQL compilation engine with build-time schema optimization
- Multi-database support: PostgreSQL (primary), MySQL, SQLite, SQL Server
- Relay Cursor Connections spec: keyset pagination on PostgreSQL, MySQL (v2.1),
  SQL Server (forward v2.0, backward v2.1); `totalCount` via fragment spreads
- Automatic Persisted Queries (APQ) with Redis-backed cache and smart invalidation
- 64-shard LRU result cache with per-entry TTL and cascade invalidation
- Row-level security (RLS): native PostgreSQL RLS or SQL WHERE injection on
  MySQL/SQLite/SQL Server — always AND-ed with application WHERE clauses
- Server-side context injection (`inject={"param": "jwt:<claim>"}`) for
  query/mutation parameter binding from JWT claims
- Typed mutation error variants with scalar field population from JSONB metadata
- `auto_params` inference: list queries automatically gain `limit`, `offset`,
  `where`, and `order_by` parameters unless explicitly overridden
- Domain-specific newtypes: `TypeName`, `FieldName`, `SqlSource`, `RoleName`,
  `Scope` replace bare strings with compile-time type safety
- `FraiseQLError::Unsupported` variant (HTTP 501) for operations not supported
  by the current database backend
- `prelude` module for ergonomic single-import access to common types
- Multi-root query pipelining with parallel execution via `try_join_all`
- AST-based `RequestValidator` replacing the character-scan `ComplexityAnalyzer`
  with correct depth, complexity, and alias-count metrics
- `QueryValidator` wired into `Executor::execute()` for DoS protection without
  requiring `fraiseql-server`

#### Server (`fraiseql-server`)

- Generic `Server<DatabaseAdapter>` with type-safe database swapping
- Graceful schema hot-reload via ArcSwap (zero-downtime config changes)
- PKCE OAuth routes (`/auth/start`, `/auth/callback`) with encrypted state tokens
- OIDC/JWKS authentication with provider error sanitization
- Per-user and per-IP rate limiting with proxy-aware IP extraction and accurate
  `Retry-After` headers; path-specific rate rules for auth endpoints
- Redis backends for PKCE state store (`redis-pkce`) and rate limiting
  (`redis-rate-limiting`) for production clustering
- Cookie security hardening: `__Host-` prefix, RFC 6265 quoting, conservative
  `Max-Age` defaults, `redirect_uri` length cap
- RBAC management API with field-level authorization
- `[server]` and `[database]` runtime configuration via `fraiseql.toml` with
  CLI flags > env vars > TOML > defaults precedence
- CSRF `Content-Type` enforcement and request body size limits
- API key authentication and token revocation
- Admin endpoints: `POST /api/v1/admin/explain` for query analysis,
  `/validate` with real parser errors
- Health check endpoint for load balancers
- Pool pressure monitoring with Prometheus metrics and scaling recommendations
- `PoolPressureMonitorConfig` (replaces deprecated `PoolTuningConfig`)
- Consistent boolean parsing for all `FRAISEQL_*` environment variables

#### Database Adapters (`fraiseql-db`)

- PostgreSQL: full feature support including JSONB fact tables, LISTEN/NOTIFY
  subscriptions, native RLS, window functions
- MySQL: SELECT, mutations, Relay pagination (forward/backward), aggregates,
  field-level encryption, federation; `JSON_UNQUOTE`/`JSON_EXTRACT` for cursors
- SQL Server: SELECT, mutations, Relay pagination (forward/backward), aggregates,
  field-level encryption, federation; SQLSTATE error code mapping (23505, 23502,
  23503, 40001, 22001); `UNIQUEIDENTIFIER` cursor support
- SQLite: read-only queries, aggregates (limited), APQ, RLS via SQL WHERE;
  `execute_function_call` returns `Unsupported` with named function
- Rich scalar type filters (6 of 44 planned types implemented)
- `SupportsMutations` trait (replaces `MutationCapable`)

#### Federation (`fraiseql-federation`)

- Extracted crate (26 files, 10,257 lines) for Apollo Federation v2
- Per-entity circuit breaker with configurable failure thresholds, half-open
  recovery, and success windows
- SAGA transaction support
- Entity type resolution and federated query planning
- `MAX_ENTITIES_BATCH_SIZE = 1_000` guard

#### Wire Protocol (`fraiseql-wire`)

- PostgreSQL wire protocol streaming for fraiseql-wire
- `MAX_FIELD_COUNT = 2_048` in `decode_data_row` / `decode_row_description`
- Property-based tests for protocol encoding round-trips
- Hardened decoder against malformed messages

#### Arrow Flight (`fraiseql-arrow`)

- Apache Arrow Flight data plane for high-throughput data export
- `ArrowDatabaseAdapter` and `ArrowEventStorage` traits
- Event storage, export, and subscription support
- Schema refresh with streaming updates

#### Observers (`fraiseql-observers`)

- Event-driven observer system with NATS backend and enterprise HA
- `CheckpointStrategy` enum: `AtLeastOnce` (fast, idempotent consumers) and
  `EffectivelyOnce` (idempotency key deduplication via `ON CONFLICT DO NOTHING`)
- Storage layer with automatic observer triggering
- Cache backend integration

#### Security (`fraiseql-auth`, `fraiseql-secrets`)

- Audit logging with PostgreSQL and syslog backends
- Field-level encryption-at-rest
- Credential rotation automation with monitoring
- HashiCorp Vault integration with multiple secret backends
- Zeroizing wrapper for sensitive key material
- Constant-time comparison via `subtle` crate
- `OsRng` for all cryptographic nonce generation
- SECURITY.md with vulnerability reporting procedures and compliance profiles
  (STANDARD, REGULATED, RESTRICTED)

#### CLI (`fraiseql-cli`)

- Commands: `compile`, `lint`, `analyze`, `cost`, `dependency-graph`, `generate`,
  `generate-views`, `introspect`, `migrate`, `sbom`, `explain`,
  `validate-documents`
- MCP server integration via `FRAISEQL_MCP_STDIO` env var
- Trusted document store with TOML config and CLI validation
- Decoupled from `fraiseql-server` via `run-server` feature flag — build with
  `--no-default-features` for a pure compile-only binary
- "Did you mean?" suggestions for mutation-not-found and fact-table-not-found errors

#### SDKs (11 languages)

- **Python**: `AsyncFraiseQLClient` with retry, typed error hierarchy, LangChain +
  LlamaIndex integrations; full ruff ruleset, `[tool.ty]` config
- **TypeScript** (`@fraiseql/client`): async HTTP client, typed errors, Vercel AI
  SDK / LangChain.js / Mastra integrations; `noUncheckedIndexedAccess`,
  `no-explicit-any: error`, vitest (282 tests)
- **Go**: HTTP client with retry, typed errors, OpenAI / Anthropic tool converters
- **Java**: `FraiseQLClient`, exception hierarchy, Spring AI + LangChain4j stubs
- **C#**: attribute-driven authoring (`[GraphQLType]`, `[GraphQLField]`),
  `SchemaExporter`, `dotnet tool` CLI, Semantic Kernel integration (103 tests)
- **F#**: dual authoring (attributes + computation expression DSL),
  `SchemaExporter`, `dotnet tool` CLI, Semantic Kernel integration (133 tests)
- **PHP**: `FraiseQLClient` with retry, PSR-18 adapter, OpenAI PHP / Prism
  integrations, `SchemaExporter` + CLI binary
- **Elixir**: compile-time macro DSL (`use FraiseQL.Schema`), `mix fraiseql.export`,
  Dialyzer + Credo strict CI (98+ tests)
- **Ruby**: `FraiseQL::Client` (Net::HTTP), ruby-openai + LangChain.rb integrations
- **Dart/Flutter**: `FraiseQLClient` with `authorizationFactory`, Google Gemini /
  Firebase Vertex AI integration
- **Rust** (`fraiseql-client`): `FraiseQLClientBuilder` with async query/mutate/
  subscribe, Candle ML integration
- All 11 SDKs forward `operationName` in requests
- All 11 SDKs ship GitHub Actions CI workflows (`.github/workflows/`)
- Cross-SDK parity test suite: 1,595 tests across 9 SDKs against golden fixtures

#### Observability

- Prometheus metrics: query latency percentiles, connection pool health, error rates
- Structured JSON logging with correlation IDs
- OpenTelemetry distributed tracing integration
- Pre-built 12-panel Grafana 10+ performance dashboard
- Per-operation metrics and real query EXPLAIN

#### Testing & Quality

- 5,326 passing tests; `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` clean; `cargo deny check` clean
- Criterion benchmark suite: GraphQL parse, cache latency, full-pipeline
- Fuzz harnesses: GraphQL parser, wire protocol, SCRAM auth, schema
  deserialization, SQL codegen
- Property-based testing: 101 properties
- k6 load testing: queries, mutations, mixed workload, auth, APQ scenarios
- E2E pipeline test (`make e2e`): Python authoring → CLI compile → server → SDK
- 34 SQL snapshot tests (WHERE operators, CTE, JSON, FTS, aggregate dialects)
- Docker Compose test infrastructure (`docker/docker-compose.test.yml`) with
  6 CI integration jobs (Redis, NATS, TLS, Vault, observers, server)
- `testcontainers` watchdog for container cleanup on SIGTERM/SIGINT
- 12 operational runbooks; SLA/SLO documentation
- `cargo semver-checks` in CI for API compatibility

#### Configuration & Deployment

- `fraiseql.toml` configuration compiled into `schema.compiled.json` with
  environment variable overrides for production
- Docker multi-stage builds (Alpine base, ~15 MB compressed)
- Kubernetes manifests with Helm charts
- `fraiseql` umbrella crate with feature bundles: `full` (all components),
  `minimal` (core only)
- TLS consolidated to rustls; `native-tls` removed from dependency tree

### Changed

- `ComplexityAnalyzer` replaced by AST-based `RequestValidator` — the old
  character-scan miscounted operation names, argument names, and directive names
  as field selectors
- `QueryMetrics` fields changed: `depth`, `complexity`, `alias_count` replace
  the old `depth`, `field_count`, `score` tuple
- `QueryValidatorConfig` gains `max_aliases` field with presets: permissive=100,
  standard=30, strict=10
- `FRAISEQL_INTROSPECTION_REQUIRE_AUTH` uses consistent boolean parsing (`true`,
  `1`, `yes`, `on` only); non-standard truthy values now log a warning
- `fraiseql-auth`, `fraiseql-webhooks`, `fraiseql-secrets` extracted from
  `fraiseql-server` as independent crates
- Redis crate upgraded 0.25 → 0.28
- `lazy_static`/`once_cell` migrated to `std::sync::LazyLock`
- `std::env::set_var` in tests replaced with `temp_env` crate
- `#[non_exhaustive]` on all public enums (except `DatabaseType`)
- All `#[allow(clippy::...)]` carry `// Reason:` justification comments
- Workspace lint config hardened with explicit `missing_errors_doc` enforcement
- `# Errors` doc sections on all fallible public functions across all crates

### Deprecated

- `PoolTuningConfig` (`fraiseql-server`, since v2.0.1) → use
  `PoolPressureMonitorConfig`; removal target: v3.0
- `observers-full` feature flag (`fraiseql-observers`) → list specific
  sub-features (`nats`, `tracing`, `in-memory`, etc.); removal target: v2.2

### Fixed

- `CachedDatabaseAdapter::cache.put()` argument mismatch: three call sites
  passed 4 arguments to a 5-argument signature, silently breaking cache writes
- Entity-aware cache invalidation: UPDATE/DELETE mutations now call
  `invalidate_by_entity` when `entity_id` is present instead of flushing the
  entire view
- Per-user rate limiting was never called — authenticated requests were limited
  by the shared IP bucket; middleware now extracts `sub` claim and routes through
  per-user token bucket
- Proxy-aware IP extraction: `trust_proxy_headers` option reads `X-Real-IP` /
  `X-Forwarded-For` behind reverse proxies
- `Retry-After` accuracy for path-limited responses (e.g. `/auth/start`)
- Cookie charset safety: `Set-Cookie` values now RFC 6265 quoted-string compliant
- Silent `Set-Cookie` omission on parse failure now returns HTTP 500
- Conservative cookie `Max-Age` default (300 s when OIDC omits `expires_in`)
- OIDC provider error strings no longer reflected to clients (mapped to fixed
  allowlist)
- SQL Server relay backward pagination with custom `order_by` now correctly
  flips all sort directions and restores all custom sort columns
- SQL Server relay `totalCount`: missing/empty `COUNT_BIG` result now surfaces
  as `FraiseQLError::Database` instead of silent zero
- SQL Server SQLSTATE codes corrected: 23505 (unique), 23502 (NOT NULL),
  40001 (deadlock) instead of generic 23000
- UUID cursor validation before SQL Server prevents opaque type-conversion errors
- SQLite `execute_function_call` returns `Unsupported` naming the function
- `null` errors array in Python SDK no longer raises `FraiseQLError`
- Mutation `sql_source` falls back to `operation.table` when None
- Connection pool exhaustion in nested queries
- All rustdoc link warnings resolved (zero `cargo doc --no-deps` warnings)

### Security

- `MAX_VARIABLES_COUNT = 1_000` in `RequestValidator`
- PKCE `code_verifier` length guard
- Discord webhook URL validation
- Rate-limit sliding window overflow protection
- Slack URL SSRF check
- `MAX_FIELD_COUNT = 2_048` in wire protocol decoders
- Unix socket path traversal guard (`validate_socket_dir` rejects `..`)
- Federation SSRF URL parser fix (`reqwest::Url::parse` + IPv6 bracket-strip)
- `MAX_ENTITIES_BATCH_SIZE = 1_000` in federation
- `MAX_JWKS_RESPONSE_BYTES = 1 MiB` in OIDC JWKS fetcher
- `MAX_VAULT_SECRET_NAME_BYTES = 1_024` + Vault SSRF URL-parser fix
- `MAX_MANIFEST_BYTES = 10 MiB` in trusted document store
- `MAX_SERIALIZE_DEPTH = 64` in GraphQL parser `serialize_value_inner`
- GET variables string length capped at `max_get_bytes`
- 19 E2E SQL injection prevention tests
- 27 auth bypass and JWT tampering detection tests
- No internal details leaked in error responses (verified by property tests)
