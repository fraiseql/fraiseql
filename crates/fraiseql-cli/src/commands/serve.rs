//! Development server command (Phase 9 Part 3)
//!
//! Watches schema.json for changes and auto-recompiles

use anyhow::Result;

/// Run the serve command (development server with hot-reload)
///
/// # Arguments
///
/// * `schema` - Path to schema.json file to watch
/// * `port` - Port to listen on
///
/// # TODO (Phase 9 Part 3)
///
/// - Implement file watching with `notify` crate
/// - Auto-recompile on schema.json changes
/// - Hot-reload compiled schema
/// - Integrate with fraiseql-server
pub async fn run(_schema: &str, _port: u16) -> Result<(), anyhow::Error> {
    anyhow::bail!("serve command not implemented yet (Phase 9 Part 3)")
}
