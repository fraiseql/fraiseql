//! What a hot-reload can and cannot re-apply.
//!
//! A schema hot-reload swaps the executor. Everything the executor reads per
//! request therefore follows it. Everything a *subsystem* read once at boot does
//! not: the request validator, the rate limiter, the route tables, the observer
//! and source pollers, the cache's per-view TTL map, and the process-global
//! acronym set are all built during startup and are immutable afterwards.
//!
//! Before this gate existed, a reload silently accepted a schema whose boot-frozen
//! sections had changed and reported "Schema reloaded successfully" — so an
//! operator who shortened a cache TTL, tightened `max_query_depth`, or added an
//! acronym got a success message and the old behaviour, until a restart (#782).
//! Half-applying a schema is the worst of the three options; the reload now
//! either applies a schema in full or refuses it and says which section requires
//! a restart.
//!
//! The classification is an **exhaustive destructuring** of [`CompiledSchema`]:
//! adding a field to the compiled schema is a compile error here until it is
//! classified. That is the mechanism; [`boot_frozen_drift`] is only its report.

use fraiseql_core::schema::CompiledSchema;

/// Compare one boot-frozen section of two schemas.
///
/// Uses the serialized form so the comparison needs only `Serialize`, which every
/// compiled-schema field has — no `PartialEq` bound to keep in sync. A
/// serialization failure counts as "differs": a section this gate cannot read is
/// not a section it may wave through.
fn differs<T: serde::Serialize>(current: &T, next: &T) -> bool {
    match (serde_json::to_value(current), serde_json::to_value(next)) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    }
}

/// The per-view cache TTL map a `CachedDatabaseAdapter` derives from a schema.
///
/// It is applied once, by `with_cache_metadata_from_schema` at construction, and
/// the adapter is shared behind an `Arc` afterwards — so a reload cannot change
/// it. Projected out of `queries` (which is otherwise hot) so a TTL edit is
/// refused while an ordinary query edit is not.
fn cache_ttl_projection(schema: &CompiledSchema) -> Vec<(&str, Option<u64>)> {
    let mut projection: Vec<(&str, Option<u64>)> = schema
        .queries
        .iter()
        .filter_map(|q| q.sql_source.as_deref().map(|view| (view, q.cache_ttl_seconds)))
        .collect();
    projection.sort_unstable();
    projection
}

/// Names of the boot-frozen sections that differ between the running schema and
/// a candidate replacement. Empty means the candidate is fully reloadable.
///
/// Each name is the compiled-schema field, which is also the `fraiseql.toml`
/// section an operator would have edited.
#[must_use]
pub fn boot_frozen_drift(current: &CompiledSchema, next: &CompiledSchema) -> Vec<&'static str> {
    // Exhaustive destructuring, deliberately without `..`: a new compiled-schema
    // field must be classified as hot (follows the executor swap) or boot-frozen
    // (requires a restart) before this compiles.
    let CompiledSchema {
        // ── Hot: read per request from `executor.schema()`, so the atomic
        //    executor swap is all that is needed.
        types: _,
        enums: _,
        input_types: _,
        interfaces: _,
        unions: _,
        queries: _, // except the cache-TTL projection below
        mutations: _,
        subscriptions: _,
        directives: _,
        subscribable: _,
        operation_cost_weights: _,
        session_variables: _,
        custom_scalars: _,
        changelog: _, // `write_enabled` is re-derived into RuntimeConfig on reload
        schema_sdl: _,
        schema_format_version: _,
        query_index: _,
        mutation_index: _,
        subscription_index: _,
        // Compile-time only: its effect is already baked into `queries`, which is
        // hot, so a change on its own is inert rather than half-applied.
        hierarchies_config: _,

        // ── Boot-frozen: read once during startup by a subsystem that is
        //    immutable afterwards. Changing one requires a restart.
        security: current_security,
        // The OIDC server client (PKCE login) resolves discovery and is built at
        // boot in `oidc_server_client_from_schema` (#621).
        auth: current_auth,
        validation_config: current_validation,
        observers: current_observers,
        observers_config: current_observers_config,
        sources: current_sources,
        subscriptions_config: current_subscriptions_config,
        mcp_config: current_mcp,
        rest_config: current_rest,
        grpc_config: current_grpc,
        federation: current_federation,
        naming_acronyms: current_acronyms,
        naming_convention: current_naming_convention,
        debug_config: current_debug,
        fact_tables: current_fact_tables,
    } = current;

    let mut drifted = Vec::new();

    // Rate limiter, state encryption, API keys, service accounts, token
    // revocation, the RLS/tenancy declarations and the cache safety gate are all
    // resolved from `[security]` during `Server::schema_subsystems`.
    if differs(current_security, &next.security) {
        drifted.push("security");
    }
    // The OIDC server client is constructed once at boot (discovery fetched then).
    if differs(current_auth, &next.auth) {
        drifted.push("auth");
    }
    // `RequestValidator` is built once in `build_app_state`.
    if differs(current_validation, &next.validation_config) {
        drifted.push("validation_config");
    }
    // Observer runtime and its pollers are started during boot.
    if differs(current_observers, &next.observers) {
        drifted.push("observers");
    }
    if differs(current_observers_config, &next.observers_config) {
        drifted.push("observers_config");
    }
    // Source pollers are spawned from the boot schema in `serve_with_shutdown`.
    if differs(current_sources, &next.sources) {
        drifted.push("sources");
    }
    // The `/ws` mount reads the lifecycle hooks and per-connection limit off the
    // `Server`, which `apply_compiled_config` set at boot.
    if differs(current_subscriptions_config, &next.subscriptions_config) {
        drifted.push("subscriptions_config");
    }
    // Route tables and mounted services are built once, at router construction.
    if differs(current_mcp, &next.mcp_config) {
        drifted.push("mcp_config");
    }
    if differs(current_rest, &next.rest_config) {
        drifted.push("rest_config");
    }
    if differs(current_grpc, &next.grpc_config) {
        drifted.push("grpc_config");
    }
    // The federation circuit breaker is constructed at boot.
    if differs(current_federation, &next.federation) {
        drifted.push("federation");
    }
    // `set_runtime_acronyms` writes a process-global `OnceLock`: only the first
    // call wins, so a reload cannot install a new set. Accepting the schema
    // anyway leaves the runtime's `to_snake_case` JSONB-key resolution
    // disagreeing with the compiled surface — fields silently resolve to null.
    if differs(current_acronyms, &next.naming_acronyms) {
        drifted.push("naming_acronyms");
    }
    // Captured by the boot-mounted REST and gRPC surfaces.
    if differs(current_naming_convention, &next.naming_convention) {
        drifted.push("naming_convention");
    }
    // Copied onto `AppState` at boot.
    if differs(current_debug, &next.debug_config) {
        drifted.push("debug_config");
    }
    // Fed into `CachedDatabaseAdapter`'s fact-table configuration at boot.
    if differs(current_fact_tables, &next.fact_tables) {
        drifted.push("fact_tables");
    }
    // Per-view cache TTLs are baked into the shared `CachedDatabaseAdapter`.
    if cache_ttl_projection(current) != cache_ttl_projection(next) {
        drifted.push("queries[].cache_ttl_seconds");
    }

    drifted
}

/// Refuse a reload whose boot-frozen configuration changed, naming the sections.
///
/// # Errors
///
/// Returns the operator-facing refusal message listing every section that
/// requires a restart.
pub fn check_reloadable(current: &CompiledSchema, next: &CompiledSchema) -> Result<(), String> {
    let drifted = boot_frozen_drift(current, next);
    if drifted.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Schema reload refused: {} changed, and {} read only at startup. \
         A hot-reload swaps the query executor; it cannot rebuild the request validator, \
         rate limiter, route tables, pollers, cache TTL map or the process-global acronym \
         set. Restart the server to apply this schema. (Reloading it anyway would report \
         success while serving the previous configuration.)",
        drifted.join(", "),
        if drifted.len() == 1 {
            "that section is"
        } else {
            "those sections are"
        },
    ))
}
