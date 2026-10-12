//! The PostgreSQL connection pool a server's configuration describes.

use fraiseql_core::db::postgres::{PoolPrewarmConfig, PostgresAdapter};

use super::ServerConfig;

/// Open the PostgreSQL adapter `config` describes.
///
/// Its pool bounds (`pool_min_size`, `pool_max_size`, which `FRAISEQL_POOL_MAX_SIZE`
/// overrides), `[database_tls]`, read replicas, the streaming-read share and the vector scan
/// settings.
///
/// `[database_tls]` is consumed *here*, at the site that opens the connections it describes.
/// It used to be read only by a log line inside `serve()`, several hundred lines after the
/// pool had already been built with `NoTls` (#801).
///
/// # Errors
///
/// When `[database_tls]` is invalid, or the pool cannot be opened.
pub async fn build_postgres_adapter(config: &ServerConfig) -> anyhow::Result<PostgresAdapter> {
    let tls = config.postgres_tls().map_err(|e| anyhow::anyhow!(e))?;
    tls.warn_if_unverified();

    tracing::info!(
        pool_min_size = config.pool_min_size,
        pool_max_size = config.pool_max_size,
        pool_timeout_secs = config.pool_timeout_secs,
        postgres_ssl_mode = %tls.effective_mode(),
        "Initializing PostgreSQL connection pool"
    );
    let adapter = PostgresAdapter::with_pool_config(
        &config.database_url,
        PoolPrewarmConfig {
            min_size: config.pool_min_size,
            max_size: config.pool_max_size,
            timeout_secs: Some(config.pool_timeout_secs),
            // Single-tenant server pool: relation resolution follows the server default.
            // Per-tenant schema isolation is installed by `create_tenant_executor`.
            search_path: None,
            tls,
            read_replicas: config.read_replicas(),
            max_streaming_reads: config.pool_max_streaming_reads,
            // `[server] vector_hnsw_iterative_scan` / `vector_ivfflat_iterative_scan`,
            // read through the one accessor so this pool and every per-tenant pool
            // agree (#1116).
            vector_scan: config.vector_scan(),
        },
    )
    .await?;
    tracing::info!("PostgreSQL adapter ready");
    Ok(adapter)
}
