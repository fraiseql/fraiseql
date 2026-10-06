//! Observer runtime and admission control configuration.

#[cfg(all(test, feature = "observers"))]
mod tests;

#[cfg(feature = "observers")]
use fraiseql_observers::config::{
    BridgeTransportConfig, EmailSmtpConfig, NatsTransportConfig, TransportConfig, TransportKind,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "observers")]
const fn default_observers_enabled() -> bool {
    true
}

#[cfg(feature = "observers")]
const fn default_poll_interval_ms() -> u64 {
    100
}

#[cfg(feature = "observers")]
const fn default_batch_size() -> usize {
    100
}

#[cfg(feature = "observers")]
const fn default_channel_capacity() -> usize {
    1000
}

#[cfg(feature = "observers")]
const fn default_auto_reload() -> bool {
    true
}

#[cfg(feature = "observers")]
const fn default_reload_interval_secs() -> u64 {
    60
}

/// Pool configuration for the observer's dedicated PostgreSQL connection pool.
///
/// The observer pool is separate from the application pool because the
/// LISTEN/NOTIFY connection occupies a persistent slot. Smaller defaults
/// are appropriate since observers need far fewer connections than the app.
///
/// Configure via `[observers.pool]` in `fraiseql.toml`:
///
/// ```toml
/// [observers.pool]
/// min_connections = 2
/// max_connections = 5
/// acquire_timeout_secs = 10
/// ```
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserverPoolConfig {
    /// Minimum number of connections to keep open (default: 2).
    #[serde(default = "default_observer_pool_min")]
    pub min_connections: u32,

    /// Maximum number of connections in the observer pool (default: 5).
    #[serde(default = "default_observer_pool_max")]
    pub max_connections: u32,

    /// Timeout in seconds for acquiring a connection from the pool (default: 10).
    #[serde(default = "default_observer_acquire_timeout")]
    pub acquire_timeout_secs: u64,
}

#[cfg(feature = "observers")]
const fn default_observer_pool_min() -> u32 {
    2
}

#[cfg(feature = "observers")]
const fn default_observer_pool_max() -> u32 {
    5
}

#[cfg(feature = "observers")]
const fn default_observer_acquire_timeout() -> u64 {
    10
}

#[cfg(feature = "observers")]
impl Default for ObserverPoolConfig {
    fn default() -> Self {
        Self {
            min_connections:      default_observer_pool_min(),
            max_connections:      default_observer_pool_max(),
            acquire_timeout_secs: default_observer_acquire_timeout(),
        }
    }
}

/// Server-side observer **runtime** tuning, lives under `[observers.runtime]`.
///
/// The same `fraiseql.toml` is consumed by both `fraiseql compile` (whose
/// `[observers]` schema owns `backend`/`redis_url`/`nats_url`/`handlers`) and
/// `fraiseql-server`. To keep the two schemas from colliding, the server's
/// runtime tuning lives in its own `[observers.runtime]` sub-table (#342):
///
/// ```toml
/// [observers]                 # compiler-owned
/// enabled  = true
/// backend  = "postgresql"
/// handlers = [ ... ]
/// [observers.runtime]         # server-owned
/// poll_interval_ms = 500
/// batch_size       = 100
/// ```
///
/// This struct is **strict** (`deny_unknown_fields`): an unrecognised key under
/// `[observers.runtime]` (e.g. a typo `pol_interval_ms`) fails to parse, so the
/// server refuses to boot with a clear error rather than silently ignoring the
/// setting (the #342 fail-loud contract).
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserverRuntimeSettings {
    /// Poll interval for change log in milliseconds (default: 100).
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// Batch size for fetching change log entries (default: 100).
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Channel capacity for event buffering (default: 1000).
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Auto-reload observers on changes (default: true).
    #[serde(default = "default_auto_reload")]
    pub auto_reload: bool,

    /// Reload interval in seconds (default: 60).
    #[serde(default = "default_reload_interval_secs")]
    pub reload_interval_secs: u64,

    /// Maximum number of entries the in-memory dead letter queue may hold.
    ///
    /// When the DLQ reaches this limit, the newest entry is dropped (the current
    /// failing action is discarded) and a warning is logged. This prevents
    /// unbounded memory growth under sustained action failures, mirroring the
    /// `fraiseql-observers` library policy so the same key means the same thing
    /// in the binary and the embedder.
    ///
    /// Default: `None` (unbounded — matches previous behaviour for
    /// backwards compatibility). Recommended production value: `10_000`.
    #[serde(default)]
    pub max_dlq_size: Option<usize>,

    /// Event transport selection (`[observers.runtime.transport]`).
    ///
    /// Selects how the runtime sources change events: PostgreSQL LISTEN/NOTIFY
    /// (default), NATS `JetStream`, or in-memory. The selection is honored by
    /// [`ObserverRuntime::start`](crate::observers::ObserverRuntime::start);
    /// `FRAISEQL_OBSERVER_TRANSPORT` and the other `FRAISEQL_NATS_*` variables
    /// override the compiled values at boot (see
    /// [`TransportConfig::with_env_overrides`]).
    ///
    /// A configured non-Postgres transport that cannot run (feature not
    /// compiled in, or NATS without a URL) fails loud at boot in production —
    /// the server never silently falls back to PostgreSQL (#350).
    #[serde(default)]
    pub transport: ObserverTransportConfig,

    /// SMTP configuration for the email observer action (`[observers.runtime.email]`).
    ///
    /// Absent (`None`) leaves the email action without a backend: it fails loud
    /// rather than silently dropping messages (#349). When present, the strict
    /// inner [`EmailSmtpConfig`] (host/port/from/TLS/env-backed credentials) is
    /// used to build a real `lettre` SMTP sender.
    #[serde(default)]
    pub email: Option<EmailSmtpConfig>,

    /// Dedicated connection pool configuration for the observer runtime.
    ///
    /// When absent, sensible observer-specific defaults are used (smaller
    /// than the application pool). Operators can set `[observers.runtime.pool]`
    /// in `fraiseql.toml` to tune independently of the main pool.
    #[serde(default)]
    pub pool: ObserverPoolConfig,

    /// Persist the triggering event payload into the
    /// `tb_observer_log.request_payload` column (default: `false`).
    ///
    /// The non-sensitive audit columns (`action_type`, `action_index`,
    /// `response_status_code`, and the `response_payload` outcome summary) are
    /// always written. The request payload can carry PII from the entity row, so
    /// it is only recorded when an operator opts in here; oversized payloads are
    /// truncated to a marker regardless (#468).
    #[serde(default)]
    pub log_payloads: bool,

    /// Redis backend for the `cache`/`invalidate` observer action
    /// (`[observers.runtime.redis]`).
    ///
    /// Absent (`None`) leaves the cache action without a backend. Because this
    /// struct is `deny_unknown_fields`, its absence was not merely a missing
    /// default — the key was **rejected at config load**, so no `fraiseql.toml`
    /// could reach the Redis cache-invalidation transport at all, even though
    /// the transport itself shipped and was tested (#985).
    ///
    /// Declaring a `cache` action without this block is a boot error, and so is
    /// declaring one in a binary built without the `observers-cache` feature:
    /// the operator learns once, at startup, rather than once per event.
    ///
    /// `FRAISEQL_REDIS_URL`, `FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS` and
    /// `FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS` override the declared values at
    /// boot; `FRAISEQL_REDIS_URL` alone supplies the block when none is
    /// declared (#1466).
    #[serde(default)]
    pub redis: Option<ObserverRedisConfig>,
}

/// The event transport (`[observers.runtime.transport]`): what the server runs.
///
/// The library's [`TransportConfig`] also carries `run_bridge`, `run_executors`
/// and a `[bridge]` table, which configure a PostgreSQL → NATS bridge and a
/// bridge-only node. The server always runs the observer executors and never
/// the bridge, so this block refuses those keys rather than accepting values
/// nothing reads (#1496).
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserverTransportConfig {
    /// Where change events come from: `postgres` (default), `nats` or
    /// `in_memory`.
    #[serde(default)]
    pub transport: TransportKind,

    /// The NATS `JetStream` stream and durable consumer
    /// (`[observers.runtime.transport.nats]`).
    #[serde(default)]
    pub nats: NatsTransportConfig,
}

#[cfg(feature = "observers")]
impl ObserverTransportConfig {
    /// The library transport configuration the runtime takes: executors on, no
    /// bridge.
    #[must_use]
    pub fn to_runtime(&self) -> TransportConfig {
        TransportConfig {
            transport:     self.transport,
            run_bridge:    false,
            run_executors: true,
            nats:          self.nats.clone(),
            bridge:        BridgeTransportConfig::default(),
        }
    }
}

/// The Redis backend for `cache` observer actions (`[observers.runtime.redis]`).
///
/// Only what the server uses: the URL and the two connection timeouts. The
/// library's `RedisConfig` also carries `pool_size`, `dedup_window_secs` and
/// `cache_ttl_secs`, which configure event deduplication and action-result
/// caching. The server runs neither, so this block refuses those keys rather
/// than accepting values nothing reads (#1466).
#[cfg(feature = "observers")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserverRedisConfig {
    /// Redis connection URL (`redis://…` or `rediss://…`). Required, here or
    /// through `FRAISEQL_REDIS_URL`.
    #[serde(default)]
    pub url: String,

    /// Timeout for each connection attempt, in seconds (default: 5).
    #[serde(default = "default_redis_connect_timeout_secs")]
    pub connect_timeout_secs: u64,

    /// Timeout for each command's response, in seconds (default: 2).
    #[serde(default = "default_redis_command_timeout_secs")]
    pub command_timeout_secs: u64,
}

#[cfg(feature = "observers")]
const fn default_redis_connect_timeout_secs() -> u64 {
    5
}

#[cfg(feature = "observers")]
const fn default_redis_command_timeout_secs() -> u64 {
    2
}

#[cfg(feature = "observers")]
impl Default for ObserverRedisConfig {
    fn default() -> Self {
        Self {
            url:                  String::new(),
            connect_timeout_secs: default_redis_connect_timeout_secs(),
            command_timeout_secs: default_redis_command_timeout_secs(),
        }
    }
}

#[cfg(feature = "observers")]
impl Default for ObserverRuntimeSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms:     default_poll_interval_ms(),
            batch_size:           default_batch_size(),
            channel_capacity:     default_channel_capacity(),
            auto_reload:          default_auto_reload(),
            reload_interval_secs: default_reload_interval_secs(),
            max_dlq_size:         None,
            transport:            ObserverTransportConfig::default(),
            email:                None,
            pool:                 ObserverPoolConfig::default(),
            log_payloads:         false,
            redis:                None,
        }
    }
}

/// Observer configuration block (`[observers]`).
///
/// This is the **shared-file** view: `[observers]` is owned by the compiler
/// schema (`backend`/`redis_url`/`nats_url`/`handlers`), while the server's
/// runtime tuning lives under [`runtime`](ObserverConfig::runtime)
/// (`[observers.runtime]`). The only top-level key both tools read is
/// `enabled`. Unknown top-level keys are tolerated here so the compiler's own
/// keys do not break server boot (the compiler is the strict gate for the
/// `[observers]` table); the strictness for the server's own settings lives on
/// [`ObserverRuntimeSettings`].
///
/// The `legacy_*` fields are migration traps for the pre-#342 flat layout (see
/// [`misplaced_runtime_keys`](ObserverConfig::misplaced_runtime_keys)).
// Reason (#1337): NO `deny_unknown_fields`, deliberately. `[observers]` is shared with
// the CLI's own `ObserversConfig`, which owns `backend` / `redis_url` / `nats_url` /
// `handlers` in the same table — denying unknown keys here would make the server refuse
// a config file the compiler wrote. `compiler_schema_keys_are_tolerated` in this
// module's tests is that contract, and it fails the moment this attribute is added.
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    /// Enable observer runtime (default: true).
    ///
    /// Shared top-level key — also read by `fraiseql compile`.
    #[serde(default = "default_observers_enabled")]
    pub enabled: bool,

    /// Server-side runtime tuning (`[observers.runtime]`).
    ///
    /// Defaults are applied when the sub-table is absent.
    #[serde(default)]
    pub runtime: ObserverRuntimeSettings,

    // ── Migration traps (pre-#342 flat layout) ──────────────────────────────
    // These keys previously lived directly under `[observers]`; they now live
    // under `[observers.runtime]`. They are captured here ONLY so `load_config`
    // can fail loud with a migration message — never silently ignored, which
    // was the #342 bug. They are not consumed by the runtime and never
    // re-serialized.
    #[serde(default, rename = "poll_interval_ms", skip_serializing)]
    pub(crate) legacy_poll_interval_ms:     Option<toml::Value>,
    #[serde(default, rename = "batch_size", skip_serializing)]
    pub(crate) legacy_batch_size:           Option<toml::Value>,
    #[serde(default, rename = "channel_capacity", skip_serializing)]
    pub(crate) legacy_channel_capacity:     Option<toml::Value>,
    #[serde(default, rename = "auto_reload", skip_serializing)]
    pub(crate) legacy_auto_reload:          Option<toml::Value>,
    #[serde(default, rename = "reload_interval_secs", skip_serializing)]
    pub(crate) legacy_reload_interval_secs: Option<toml::Value>,
    #[serde(default, rename = "max_dlq_size", skip_serializing)]
    pub(crate) legacy_max_dlq_size:         Option<toml::Value>,
    #[serde(default, rename = "pool", skip_serializing)]
    pub(crate) legacy_pool:                 Option<toml::Value>,
}

#[cfg(feature = "observers")]
impl ObserverConfig {
    /// Pre-#342 flat server-tuning keys still present directly under
    /// `[observers]` (they moved to `[observers.runtime]` in v2.5.0).
    ///
    /// Returns the offending key names so the caller can fail loud with a
    /// migration message. Empty in the new layout. The list is returned in a
    /// stable, declaration order for deterministic error messages.
    #[must_use]
    pub fn misplaced_runtime_keys(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.legacy_poll_interval_ms.is_some() {
            keys.push("poll_interval_ms");
        }
        if self.legacy_batch_size.is_some() {
            keys.push("batch_size");
        }
        if self.legacy_channel_capacity.is_some() {
            keys.push("channel_capacity");
        }
        if self.legacy_auto_reload.is_some() {
            keys.push("auto_reload");
        }
        if self.legacy_reload_interval_secs.is_some() {
            keys.push("reload_interval_secs");
        }
        if self.legacy_max_dlq_size.is_some() {
            keys.push("max_dlq_size");
        }
        if self.legacy_pool.is_some() {
            keys.push("pool");
        }
        keys
    }
}

/// Admission control configuration for backpressure limiting.
///
/// Pairs with `crate::resilience::backpressure::AdmissionController`.
/// See [`super::ServerConfig::admission_control`] for wiring instructions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionConfig {
    /// Maximum number of in-flight concurrent requests (semaphore permits).
    ///
    /// Defaults to 500.
    #[serde(default = "default_admission_max_concurrent")]
    pub max_concurrent: usize,

    /// Maximum number of requests waiting for a permit (queue depth).
    ///
    /// When the queue is full, new requests are rejected with 503.
    /// Defaults to 1000.
    #[serde(default = "default_admission_max_queue_depth")]
    pub max_queue_depth: u64,
}

pub(crate) const fn default_admission_max_concurrent() -> usize {
    500
}

pub(crate) const fn default_admission_max_queue_depth() -> u64 {
    1000
}
