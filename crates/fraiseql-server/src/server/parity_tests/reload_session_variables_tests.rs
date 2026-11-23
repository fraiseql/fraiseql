//! A hot reload runs the session-variable boot gate: a reload that adds
//! `[session_variables]` on an adapter that applies none is refused, as boot refuses it,
//! and the running schema stays the one being served.

use std::sync::Arc;

use fraiseql_core::schema::{SessionVariableMapping, SessionVariableSource};

use super::{
    NoopRelayAdapter, fully_configured_schema, fully_configured_server_config, write_schema,
};
use crate::Server;

async fn server_on(adapter: NoopRelayAdapter) -> Server {
    Server::new(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(adapter),
        None,
    )
    .await
    .expect("Server::new must boot the fixture schema")
}

fn with_session_variable() -> fraiseql_core::schema::CompiledSchema {
    let mut schema = fully_configured_schema();
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.tenant_id".to_string(),
        source: SessionVariableSource::Jwt {
            claim: "tenant_id".to_string(),
        },
    });
    schema
}

#[tokio::test]
async fn a_reload_adding_session_variables_the_adapter_cannot_apply_is_refused() {
    let server = server_on(NoopRelayAdapter {
        applies_session_variables: false,
    })
    .await;
    let state = server.build_app_state();
    let served = state.executor.load().schema().content_hash();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &with_session_variable()).await;
    let error = state.reload_schema(&path).await.expect_err("boot refuses this schema");

    assert!(
        error.contains("cannot apply session variables"),
        "the boot gate's refusal: {error}"
    );
    assert_eq!(
        state.executor.load().schema().content_hash(),
        served,
        "a refused reload leaves the served schema in place"
    );
}

#[tokio::test]
async fn a_reload_adding_session_variables_the_adapter_applies_is_accepted() {
    let server = server_on(NoopRelayAdapter::default()).await;
    let state = server.build_app_state();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &with_session_variable()).await;
    state.reload_schema(&path).await.expect("the adapter applies session variables");

    assert!(!state.executor.load().schema().session_variables.variables.is_empty());
}
