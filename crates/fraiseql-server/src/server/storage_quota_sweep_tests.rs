//! #633: every serving state measures tenant storage quotas.
//!
//! `spawn_lifecycle_tasks` is the one call both HTTP serve entry points make, so the sweep
//! it starts is started by both; a server without the tenancy runtime has no tenant to
//! measure and starts none.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::{Server, server_config::ServerConfig};

async fn tasks_started(tenancy_runtime: bool) -> usize {
    let mut config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    config.tenancy.runtime.enabled = tenancy_runtime;
    let mut server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap();
    let state = server.provisioned_app_state().await.unwrap();
    let before = server.lifecycle_task_count();
    server.spawn_lifecycle_tasks(&state);
    server.lifecycle_task_count() - before
}

#[tokio::test]
async fn the_tenancy_runtime_starts_the_storage_quota_sweep() {
    assert_eq!(
        tasks_started(true).await - tasks_started(false).await,
        1,
        "the tenancy runtime adds exactly the storage-quota sweep"
    );
}

#[test]
fn a_zero_sweep_interval_is_refused() {
    let mut config = ServerConfig::default();
    config.tenancy.storage_quota.interval_secs = 0;
    let error = config.validate().unwrap_err();
    assert!(error.contains("[tenancy.storage_quota] interval_secs"), "{error}");
}
