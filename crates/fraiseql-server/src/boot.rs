//! Assembling the PostgreSQL-backed server: the one sequence the `fraiseql-server`
//! binary and `fraiseql run` share.
//!
//! `fraiseql run` used to construct its server with no database pool and none of the
//! binary's `with_*` calls, so every capability that needs a pool (observers, inbound
//! ingestion, cron, sources, RBAC, Postgres session state) was built inert under it, and
//! object storage, token revocation, tenancy and the REST write surface were never
//! attached. Both entry points now build the server here.

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};

use crate::{Server, ServerConfig, capability::REGISTRY, schema::loader::FunctionsConfig};

/// Build the server the binary serves from `config`, `schema` and the request adapter.
///
/// # Errors
///
/// Returns an error when a configured subsystem cannot be built: the observer pool
/// cannot connect, object storage or token revocation cannot be set up, or the server
/// constructor refuses the configuration.
#[allow(clippy::cognitive_complexity)] // Reason: one guarded step per optional subsystem, in boot order
pub async fn build_postgres_server(
    config: ServerConfig,
    schema: CompiledSchema,
    functions: Option<FunctionsConfig>,
) -> anyhow::Result<Server> {
    refuse_functions_this_build_cannot_run(functions.as_ref())?;
    let adapter = Arc::new(crate::server_config::build_postgres_adapter(&config).await?);

    // #487: opt-in fail-fast existence check — every declared `sql_source` (query
    // view / mutation function) must be backed by the database. Default OFF (the
    // boot path is unchanged); when on, an unbacked source fails boot with a
    // precise list instead of surfacing as an opaque per-request 500 later. Runs
    // here, after the adapter exists, where schema + adapter coexist.
    if config.validate_sql_sources {
        use crate::sql_source_check as check;
        let unbacked = check::find_unbacked_sources(&schema, adapter.as_ref()).await?;
        // #1426: present is not enough; the server's own role must be able to use it.
        let unusable = check::find_unusable_sources(&schema, adapter.as_ref()).await?;
        if !unbacked.is_empty() || !unusable.is_empty() {
            let role = check::connected_role(adapter.as_ref()).await?;
            anyhow::bail!("{}", check::format_source_problems(&unbacked, &unusable, &role));
        }
        tracing::info!(
            sources = schema.queries.len() + schema.mutations.len(),
            "sql_source validation passed: all declared sources are backed"
        );
    }

    let db_pool = build_observer_pool(&config).await?;

    // Wire `[storage.<name>]` into a mounted /storage/v1/* route group. Built
    // before the server is constructed (which moves `config`); a None result
    // means no storage was configured. PostgreSQL-only — see build_storage_state.
    let storage_state = crate::server_config::build_storage_state(&config)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(state) = &storage_state {
        tracing::info!(
            buckets = state.buckets.load().len(),
            "Object storage configured; mounting /storage/v1/*"
        );
    }

    // Postgres-backed token revocation (#357): revocation_manager_from_schema defers
    // the "postgres" backend because it needs a database connection, so build it here
    // (where the database URL is available) and install it on the server below.
    let pg_revocation_manager =
        crate::token_revocation::build_postgres_revocation_manager(&config.database_url, &schema)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Multi-tenant runtime provisioning (#330): the per-tenant executor factory is
    // built when `[tenancy.runtime] enabled = true`. Its adapter type MUST match the
    // server's — `CachedDatabaseAdapter<PostgresAdapter>` on every PG constructor
    // since #889 gave the arrow path the same result cache as its siblings. Capture
    // the flag here, before `config` is moved into the constructor.
    let tenancy_runtime_enabled = config.tenancy.runtime.enabled;

    // Captured before `config` is moved into the constructor, for the same reason.
    // Validated already by `build_postgres_adapter`, so this cannot be a new failure.
    let database_tls = config.postgres_tls().map_err(|e| anyhow::anyhow!(e))?;
    // Likewise: the vector-scan settings every tenant pool inherits (#1116), and
    // the read-replica routing policy (#957).
    let tenant_read_replica_policy = config.read_replica_policy();
    let tenant_vector_scan = config.vector_scan();

    // Arrow Flight path: only available with the `arrow` feature, only on PG.
    #[cfg(feature = "arrow")]
    {
        use crate::arrow::create_flight_service;
        // #953: the Upload allow-list is an operator decision, read from
        // `flight_upload_tables`. Empty (the default) leaves Upload disabled.
        let flight_service = create_flight_service(adapter.clone(), &config.flight_upload_tables);
        // `OptimizedView` serves exactly the operator's `flight_views`, nothing else.
        crate::arrow::register_flight_views(&flight_service, &config.flight_views).await;
        if config.flight_upload_tables.is_empty() {
            tracing::info!(
                "Arrow Flight service initialized with real database adapter (Upload disabled: \
                 flight_upload_tables is empty)"
            );
        } else {
            tracing::warn!(
                tables = ?config.flight_upload_tables,
                "Arrow Flight service initialized with real database adapter; Upload is ENABLED \
                 for these tables — clients may INSERT into them without passing the mutation \
                 pipeline's authorizer or field RBAC"
            );
        }
        let server =
            Server::with_flight_service(config, schema, adapter, db_pool, Some(flight_service))
                .await?;
        // #896: the functions section comes from the artifact we loaded, not from a
        // second read of `schema_path` inside the serve path.
        #[cfg(feature = "functions-runtime")]
        let server = server.with_functions_config(functions);
        // Every constructor wraps the adapter in `CachedDatabaseAdapter` (#889), so the
        // tenant factory must produce cached executors to match the server's adapter
        // type — the same expression the non-arrow branch uses.
        // Tenant pools inherit the server's `[database_tls]`, its read-replica
        // routing policy and its vector-scan settings; the registration request body
        // cannot influence any of them (see `make_executor_factory`). It does supply
        // its own replica URLs.
        let tenant_factory = tenancy_runtime_enabled.then(|| {
            crate::tenancy::make_executor_factory::<
                fraiseql_core::cache::CachedDatabaseAdapter<PostgresAdapter>,
            >(
                database_tls.clone(), tenant_read_replica_policy.clone(), tenant_vector_scan
            )
        });
        let server = match storage_state {
            Some(state) => server.with_storage_state(state),
            None => server,
        };
        let server = match tenant_factory {
            Some(factory) => server.with_tenant_executor_factory(factory),
            None => server,
        };
        let server = match pg_revocation_manager {
            Some(manager) => server.with_revocation_manager(manager),
            None => server,
        };
        let server = enable_rest_writes(server);
        #[allow(clippy::needless_return)]
        // Reason: the tail of the arrow build only; the other build continues below
        return Ok(server);
    }

    // Non-arrow PG path: pick the relay-capable constructor when the schema
    // declares relay queries (fraiseql/fraiseql#191).
    #[cfg(not(feature = "arrow"))]
    {
        let has_relay_queries = schema.queries.iter().any(|q| q.relay);
        let server = if has_relay_queries {
            Server::with_relay_pagination(config, schema, adapter, db_pool).await?
        } else {
            Server::new(config, schema, adapter, db_pool).await?
        };
        // #896: the functions section comes from the artifact we loaded, not from a
        // second read of `schema_path` inside the serve path.
        #[cfg(feature = "functions-runtime")]
        let server = server.with_functions_config(functions);
        // Non-arrow path: `Server::new`/`with_relay_pagination` wrap the adapter in
        // `CachedDatabaseAdapter`, so the tenant factory must produce cached executors
        // to match the server's adapter type.
        // Tenant pools inherit the server's `[database_tls]`, its read-replica
        // routing policy and its vector-scan settings; the registration request body
        // cannot influence any of them (see `make_executor_factory`). It does supply
        // its own replica URLs.
        let tenant_factory = tenancy_runtime_enabled.then(|| {
            crate::tenancy::make_executor_factory::<
                fraiseql_core::cache::CachedDatabaseAdapter<PostgresAdapter>,
            >(
                database_tls.clone(), tenant_read_replica_policy.clone(), tenant_vector_scan
            )
        });
        let server = match storage_state {
            Some(state) => server.with_storage_state(state),
            None => server,
        };
        let server = match tenant_factory {
            Some(factory) => server.with_tenant_executor_factory(factory),
            None => server,
        };
        let server = match pg_revocation_manager {
            Some(manager) => server.with_revocation_manager(manager),
            None => server,
        };
        let server = enable_rest_writes(server);
        Ok(server)
    }
}

/// Refuse declared functions when this build has no function runtime.
///
/// # Errors
///
/// When `functions` declares a definition and this build lacks `functions-runtime`.
///
/// The loader refuses a `functions` section this build cannot serve, but `fraiseql run`
/// hands its compiled schema over in memory, past the loader, so the refusal is made
/// again here, where both entry points pass.
pub fn refuse_functions_this_build_cannot_run(
    functions: Option<&FunctionsConfig>,
) -> anyhow::Result<()> {
    let Some(entry) = REGISTRY.iter().find(|entry| entry.meta.name == "functions") else {
        anyhow::bail!("the capability registry has no `functions` entry");
    };
    let declared: Vec<&str> = functions
        .iter()
        .flat_map(|functions| &functions.definitions)
        .map(|definition| definition.name.as_str())
        .collect();
    if entry.compiled_in || declared.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "the schema declares functions this build cannot run ({}): it was built without the \
         `{}` feature, so none of them would ever fire. To run them, {}.",
        declared.join(", "),
        entry.meta.feature,
        entry.meta.remedy()
    )
}

/// Create a dedicated PostgreSQL pool for the observer runtime.
///
/// Observers require their own pool because the LISTEN/NOTIFY connection
/// occupies a persistent slot that must not be shared with request-serving
/// connections (request connections need to be available for concurrent queries).
///
/// The observer pool is configured independently via `[observers.runtime.pool]`
/// in `fraiseql.toml`. When absent, observer-specific defaults are used (smaller
/// than the application pool — observers need far fewer connections).
#[cfg(feature = "observers")]
async fn build_observer_pool(config: &ServerConfig) -> anyhow::Result<Option<sqlx::PgPool>> {
    use std::time::Duration;

    use sqlx::postgres::PgPoolOptions;

    let pool_cfg = config.observers.as_ref().map(|o| o.runtime.pool.clone()).unwrap_or_default();

    tracing::info!(
        min = pool_cfg.min_connections,
        max = pool_cfg.max_connections,
        timeout_secs = pool_cfg.acquire_timeout_secs,
        "Initializing observer PostgreSQL pool"
    );

    let pool = PgPoolOptions::new()
        .min_connections(pool_cfg.min_connections)
        .max_connections(pool_cfg.max_connections)
        .acquire_timeout(Duration::from_secs(pool_cfg.acquire_timeout_secs))
        .connect(&config.database_url)
        .await?;

    Ok(Some(pool))
}

/// No observer runtime without the `observers` feature, so no pool.
#[cfg(not(feature = "observers"))]
#[allow(clippy::unused_async)] // Reason: the same signature as the observers build, so the call site has no cfg
async fn build_observer_pool(_config: &ServerConfig) -> anyhow::Result<Option<sqlx::PgPool>> {
    Ok(None)
}

/// Enable the REST write surface on a server whose adapter supports mutations.
///
/// One helper rather than a copy at each boot path, because "which backends serve REST
/// writes" is precisely the question that drifted out of existence in #865: `rest_router`
/// lost its only production caller while the served `OpenAPI` document went on
/// advertising every write path, so following the published contract earned a 405.
#[cfg(feature = "rest")]
fn enable_rest_writes(server: Server) -> Server {
    server.with_rest_write_surface()
}

/// No-op counterpart for builds without the `rest` feature, where there is no REST
/// transport to mount at all.
#[cfg(not(feature = "rest"))]
const fn enable_rest_writes(server: Server) -> Server {
    server
}
