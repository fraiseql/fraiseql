#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Config-coverage drift gate (#612 item M) — the durable half of the config-drift
//! train.
//!
//! This test exists because #612 was a *class* of defect: config keys the CLI
//! `TomlSchema` accepts that no runtime consumer honors (they validate, then do
//! nothing). It walks every leaf key of `TomlSchema::default()` and asserts each
//! one is accounted for in the checked-in [`MANIFEST`] below, which names — for
//! every leaf — the consumer that honors it, or records that it is deliberately
//! **rejected at load** (the honest-loud dispositions from Phase 04/05).
//!
//! Adding a new field to `TomlSchema` (or any nested config struct) that is not in
//! the manifest fails this test at PR time — the drift can no longer reach `dev`
//! unnoticed. Adding the manifest entry is a deliberate, reviewable act that forces
//! the author to state who consumes the key (or that it is rejected).
//!
//! Limitation: `serde` omits `Option::None` and `skip_serializing_if`-empty fields
//! from `default()`, so a subtree that is entirely absent by default is not walked
//! here. The `config_coverage_pin_test.rs` round-trip pins cover the specific
//! CLI↔server shape pairs (#6, #9, 5b) that this leaf-walk cannot reach.

use fraiseql_cli::config::toml_schema::TomlSchema;
use serde_json::Value;

/// Every leaf key-path of `TomlSchema::default()` mapped to the subsystem that
/// consumes it — or the disposition that **rejects** it at load. A leaf not
/// covered here fails the test (a new top-level section or key can't slip in
/// unconsumed); a `.*` entry that matches nothing fails too (stale-owner guard).
///
/// An entry ending in `.*` claims *section ownership*: it covers every leaf under
/// that prefix, and names the one subsystem that owns the whole section. Exact
/// entries are used for the honest-loud **REJECT** keys so their disposition is
/// documented in one place and a removed rejection is caught. A new *section* or a
/// new *top-level key* is always gated; a new leaf under an already-owned section
/// inherits that owner (the CLI↔server shape pairs that a leaf-walk can't verify —
/// #6, #9, 5b — are pinned separately in `config_coverage_pin_test.rs`).
const MANIFEST: &[(&str, &str)] = &[
    // ── Compiled-schema authoring surface (CLI → compiled.json) ──────────────
    ("schema.*", "compiled schema metadata header (SchemaMetadata → compiled.json)"),
    ("types", "user schema data — object types (empty by default)"),
    ("queries", "user schema data — query definitions (empty by default)"),
    ("mutations", "user schema data — mutation definitions (empty by default)"),
    ("crud", "compiler CRUD generation (authoring-time codegen input)"),
    ("hierarchies", "compiler hierarchy generation (authoring-time codegen input)"),
    ("naming_convention", "compiler naming convention (#400/#410) → recasing"),
    (
        "query_defaults.*",
        "compiled query defaults (limit/offset/order_by/where) → runtime executor",
    ),
    ("changelog", "compiled changelog config → server changelog consumer"),
    ("validation", "compiled schema validation config → server ValidationConfig"),
    ("subscriptions", "compiled subscription config → server subscription runtime"),
    ("federation.*", "compiled federation block → server federation (#507/#503)"),
    (
        "auth",
        "server OIDC/JWT auth, reconciled CLI↔server in #612 item 9 (pinned in config_coverage_pin_test)",
    ),
    // ── Authoring-time only (consumed by the CLI, never lowered) ─────────────
    ("debug.*", "CLI/compiler debug flags (compile-time diagnostics)"),
    ("domain_discovery.*", "CLI domain-discovery of schema files (authoring-time)"),
    ("includes.*", "CLI multi-file schema includes (authoring-time loader)"),
    // ── Server runtime surface (compiled → ServerConfig) ─────────────────────
    ("database.*", "server DB pool + connection (ServerConfig database_url / pool_*)"),
    ("server.*", "server runtime bind/cors/tls/timeouts (ServerConfig)"),
    ("rest.*", "fraiseql-server `rest` feature (RestConfig)"),
    ("mcp.*", "fraiseql-server `mcp` feature (McpConfig)"),
    // ── Observers: mixed. Backend/enabled consumed; handlers REJECTED (#8) ───
    (
        "observers.enabled",
        "changelog gate — read by converter (compiled observers_config.enabled)",
    ),
    ("observers.backend", "runtime observer transport backend (redis/nats/postgres)"),
    ("observers.redis_url", "runtime observer redis backend URL"),
    ("observers.nats_url", "runtime observer NATS backend URL"),
    (
        "observers.handlers",
        "REJECTED at load (#8) — not runtime observers; use tb_observer (#631)",
    ),
    // ── Security: mixed. Consumed blocks vs the declared-but-unenforced ones ─
    (
        "security.rate_limiting",
        "server RateLimitingSecurityConfig — rate-limit middleware (#609 CIDRs)",
    ),
    (
        "security.token_revocation",
        "server token_revocation — revoke_all_ttl_secs wired in #612 item 6",
    ),
    ("security.error_sanitization", "server error-sanitization layer"),
    ("security.state_encryption", "server PKCE state encryption"),
    ("security.pkce", "server PKCE OAuth config"),
    ("security.persisted_queries_only", "server persisted-queries-only gate (#379)"),
    ("security.trusted_documents", "server trusted-documents allowlist"),
    ("security.default_policy", "server default authorization policy"),
    (
        "security.enterprise.*",
        "server enterprise security aggregate (rate-limit/pkce/audit/sanitization)",
    ),
    (
        "security.api_keys",
        "server static API-key auth; storage != \"env\" REJECTED at load (#7/#627)",
    ),
    ("security.rules", "REJECTED at load (#4) — declared-but-unenforced authz (#626)"),
    (
        "security.policies",
        "REJECTED at load (#4) — declared-but-unenforced authz (#626)",
    ),
    (
        "security.field_auth",
        "REJECTED at load (#4) — declared-but-unenforced authz (#626)",
    ),
    // ── Accepted-but-unconsumed sections rejected loud at load (Phase 04) ────
    ("caching.*", "REJECTED at load (#1) — no compiled/runtime consumer (#623)"),
    ("analytics.*", "REJECTED at load (#2) — fully inert (#624)"),
    ("observability.*", "REJECTED at load (#3) — use [metrics]/[tracing] (#625)"),
];

/// Recursively collect the dotted paths of every leaf in `value`.
///
/// A leaf is any value that is not a *non-empty* object: scalars, `null`, arrays,
/// and empty objects are terminal. This mirrors how a new scalar/section config
/// key shows up in `default()` serialization.
fn collect_leaves(value: &Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_leaves(v, &path, out);
            }
        },
        _ => out.push(prefix.to_string()),
    }
}

/// True if `path` is covered by a manifest entry (exact match, or a `.*` prefix).
fn is_covered(path: &str) -> bool {
    MANIFEST.iter().any(|(pattern, _)| {
        pattern.strip_suffix(".*").map_or(*pattern == path, |prefix| {
            path == prefix || path.starts_with(&format!("{prefix}."))
        })
    })
}

#[test]
fn every_toml_schema_leaf_has_a_named_consumer() {
    let default_json = serde_json::to_value(TomlSchema::default()).unwrap();
    let mut leaves = Vec::new();
    collect_leaves(&default_json, "", &mut leaves);
    leaves.sort();
    leaves.dedup();

    let uncovered: Vec<&String> = leaves.iter().filter(|p| !is_covered(p)).collect();
    assert!(
        uncovered.is_empty(),
        "TomlSchema has config leaves with no named consumer in the coverage manifest \
         (#612 item M). Add each to MANIFEST in {} naming who consumes it (or that it is \
         rejected at load):\n{}",
        file!(),
        uncovered
            .iter()
            .map(|p| format!("    (\"{p}\", \"<consumer>\"),"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Stale-owner guard: every manifest entry must match at least one real leaf of
/// `TomlSchema::default()`. A section that is renamed or removed leaves its
/// manifest entry matching nothing — this catches that so the manifest cannot rot
/// into claiming consumers for keys that no longer exist.
#[test]
fn every_manifest_entry_matches_a_real_leaf() {
    let default_json = serde_json::to_value(TomlSchema::default()).unwrap();
    let mut leaves = Vec::new();
    collect_leaves(&default_json, "", &mut leaves);

    let matches_a_leaf = |pattern: &str| -> bool {
        pattern.strip_suffix(".*").map_or_else(
            || leaves.iter().any(|l| l == pattern),
            |prefix| leaves.iter().any(|l| l == prefix || l.starts_with(&format!("{prefix}."))),
        )
    };

    let stale: Vec<&str> =
        MANIFEST.iter().map(|(p, _)| *p).filter(|p| !matches_a_leaf(p)).collect();
    assert!(
        stale.is_empty(),
        "manifest entries no longer match any TomlSchema leaf: {stale:?}"
    );
}

/// The gate itself: a config key that is *not* in the manifest is reported as
/// uncovered. This is the deliberately-broken fixture — it proves the drift gate
/// actually catches an unconsumed key rather than silently passing.
#[test]
fn an_unmanifested_key_is_flagged_as_uncovered() {
    assert!(!is_covered("newsection.newkey"), "an unknown section must be uncovered");
    assert!(
        !is_covered("security.brand_new_block"),
        "a new security sub-block must be uncovered"
    );
    // And a genuinely-owned key is covered, so the gate is not vacuously strict.
    assert!(is_covered("schema.name"), "schema.name is owned by the schema.* entry");
    assert!(is_covered("security.rate_limiting"), "rate_limiting is owned");
}
