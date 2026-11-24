//! A multi-root mutation's failing entry is sanitized for the deployment the server was built
//! for, not for the one the process environment declares: the renderer
//! `executor_runtime_config` installs takes the `deployment` it is handed.

use fraiseql_core::{error::FraiseQLError, schema::CompiledSchema};
use fraiseql_guard::deployment::{Deployment, ENV_VAR, PROFILE_VAR};

use super::executor_runtime_config;
use crate::server_config::ServerConfig;

/// The database text a production sanitizer withholds.
const DATABASE_TEXT: &str = "relation \"secret_ledger\" does not exist";

/// The failing root's entry, as a server built for `deployment` renders it while the
/// environment declares `declared`.
fn rendered(deployment: Deployment, declared: &str) -> serde_json::Value {
    temp_env::with_vars(
        [
            (ENV_VAR, Some(declared)),
            (PROFILE_VAR, None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || {
            let rt = executor_runtime_config(
                &CompiledSchema::default(),
                &ServerConfig::default(),
                deployment,
            )
            .expect("valid schema");
            let render = rt.root_error_renderer.expect("a renderer is installed");
            render(&FraiseQLError::Database {
                message:    DATABASE_TEXT.to_string(),
                sql_state:  Some("42P01".to_string()),
                constraint: None,
            })
        },
    )
}

#[test]
fn a_production_server_withholds_database_text_in_a_development_environment() {
    let entry = rendered(Deployment::Production, "development");
    assert!(!entry.to_string().contains("secret_ledger"), "{entry}");
}

#[test]
fn a_development_server_shows_database_text_in_a_production_environment() {
    let entry = rendered(Deployment::Development, "production");
    assert!(entry.to_string().contains("secret_ledger"), "{entry}");
}
