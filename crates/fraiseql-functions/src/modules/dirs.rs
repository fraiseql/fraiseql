//! Where a deployment's modules are: the compiled directories, with the environment's
//! overrides applied.
//!
//! One compiled schema deploys to hosts that keep their modules elsewhere, so each
//! directory can be overridden by an environment variable, which wins when it is set
//! and not blank. The server and `fraiseql functions invoke` both apply this, so the
//! harness runs the modules the server would.

use crate::types::FunctionsConfig;

/// Overrides `[functions] module_dir`.
pub const MODULE_DIR_ENV: &str = "FRAISEQL_FUNCTIONS_MODULE_DIR";

/// Overrides `[functions] shared_dir`, or declares one the compiled schema does not.
pub const SHARED_DIR_ENV: &str = "FRAISEQL_FUNCTIONS_SHARED_DIR";

/// Apply the environment's directory overrides to `config`, reading variables through
/// `get_env` (`|key| std::env::var(key).ok()` in production).
pub fn apply_env_overrides(config: &mut FunctionsConfig, get_env: impl Fn(&str) -> Option<String>) {
    let set = |key: &str| get_env(key).filter(|value| !value.trim().is_empty());
    if let Some(module_dir) = set(MODULE_DIR_ENV) {
        config.module_dir = module_dir.into();
    }
    if let Some(shared_dir) = set(SHARED_DIR_ENV) {
        config.shared_dir = Some(shared_dir.into());
    }
}

#[cfg(test)]
#[path = "dirs_tests.rs"]
mod tests;
