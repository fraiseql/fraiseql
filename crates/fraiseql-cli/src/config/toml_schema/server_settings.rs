//! Server settings configuration for TOML schema (validation, debug, MCP).

/// The `[mcp]` table: the server's own type, so a key the compiler reads is one the
/// server reads (core owns section types).
pub use fraiseql_core::schema::McpConfig;
use serde::{Deserialize, Serialize};

/// Query validation limits (depth and complexity).
///
/// ```toml
/// [validation]
/// max_query_depth = 10
/// max_query_complexity = 100
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    /// Maximum allowed query nesting depth. `None` uses the server default (10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_depth: Option<u32>,

    /// Maximum allowed query complexity score. `None` uses the server default (100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_complexity: Option<u32>,

    /// Maximum bytes a single read may deliver, measured on the rows returned.
    ///
    /// Depth and complexity score the request; this bounds the answer, which is
    /// the only cost a read from a materialised view actually has. `None` leaves
    /// reads unbounded by size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_response_bytes: Option<u64>,

    /// Maximum rows a top-level `first`/`last`/`limit` argument may request,
    /// guarding against unbounded-pagination denial of service (#421). `None`
    /// uses the server default (1000); the server also honours the
    /// `FRAISEQL_MAX_PAGE_SIZE` environment variable as an override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_page_size: Option<u32>,

    /// The deepest offset a client may page to (#1306): a GraphQL `offset:`, REST
    /// `?offset=` or `?rel.offset=` beyond it is refused before any statement, naming the
    /// relay connection as the path that pages at any depth. `None` (the default) refuses
    /// none; the server also honours `FRAISEQL_MAX_OFFSET` as an override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_offset: Option<u32>,

    /// The most (bucket, entity) cells an aggregate carrying a semi-additive measure forward
    /// may read (#1459): each is one index seek, and a larger request is refused before the
    /// measure is read. `None` uses the runtime default (250,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_semi_additive_cells: Option<u64>,
}

impl ValidationConfig {
    /// Refuse a `max_offset` of 0: it would refuse every offset page, which is not a
    /// ceiling. Leave the key unset for none. Likewise `max_semi_additive_cells` (#1459).
    ///
    /// # Errors
    ///
    /// When `max_offset` or `max_semi_additive_cells` is 0.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.max_offset == Some(0) {
            anyhow::bail!(
                "[validation] max_offset = 0 would refuse every offset page. Set the deepest \
                 offset clients may page to, or leave the key unset for no ceiling."
            );
        }
        if self.max_semi_additive_cells == Some(0) {
            anyhow::bail!(
                "[validation] max_semi_additive_cells = 0 would refuse every aggregate over a \
                 semi-additive measure. Set the most cells such an aggregate may read, or leave \
                 the key unset for the default."
            );
        }
        Ok(())
    }
}

/// Debug/development configuration.
///
/// Controls features that should only be enabled during development or
/// in trusted environments. All flags default to off.
///
/// ```toml
/// [debug]
/// enabled = true
/// database_explain = true
/// expose_sql = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    /// Master switch — all debug features require this to be `true`.
    pub enabled: bool,

    /// When `true`, the explain endpoint will also run `EXPLAIN` against the
    /// database and include the query plan in the response.
    pub database_explain: bool,

    /// When `true`, the explain endpoint includes the generated SQL in the
    /// response. Defaults to `true` (SQL is shown even without
    /// `database_explain`).
    pub expose_sql: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            enabled:          false,
            database_explain: false,
            expose_sql:       true,
        }
    }
}
