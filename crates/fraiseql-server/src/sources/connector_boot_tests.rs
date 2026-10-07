//! #1399 — an enabled source's connector is loaded at boot, or the boot refuses.
//!
//! A Model B connector is bound by name and has no function definition, so the loader
//! never loaded it and the scheduler logged a warning and skipped the source: an enabled
//! source silently never ran. Driven through `serve_on_listener`, whose boot prologue is
//! the provisioning every serve entry point shares.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::{path::Path, sync::Arc};

use fraiseql_core::schema::{CompiledSchema, SourceDefinition};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::{
    Server,
    schema::loader::FunctionsConfig,
    server_config::{ServerConfig, SourcesConfig},
};

fn schema_with(source: SourceDefinition) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.sources = vec![source];
    schema
}

fn orders() -> SourceDefinition {
    SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")
}

/// A `functions` section with no function definitions: the issue's shape, where the
/// only code in `module_dir` is the connector.
fn functions_in(dir: &Path) -> FunctionsConfig {
    FunctionsConfig {
        module_dir:  dir.to_path_buf(),
        dlq_store:   None,
        definitions: vec![],
    }
}

async fn boot(
    schema: CompiledSchema,
    functions: Option<FunctionsConfig>,
    scheduler_enabled: bool,
) -> crate::Result<()> {
    let config = ServerConfig {
        cors_enabled: false,
        sources: Some(Box::new(SourcesConfig {
            enabled: scheduler_enabled,
            ..SourcesConfig::default()
        })),
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .expect("Server::new")
        .with_functions_config(functions);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    Box::pin(server.serve_on_listener(listener, async {})).await
}

#[tokio::test]
async fn a_connector_missing_from_module_dir_refuses_the_boot() {
    let dir = tempfile::tempdir().unwrap();
    let message = boot(schema_with(orders()), Some(functions_in(dir.path())), true)
        .await
        .expect_err("an enabled source whose connector cannot load must refuse the boot")
        .to_string();
    assert!(
        message.contains("\"orders\"") && message.contains("pollOrders.{js,ts,mjs,mts}"),
        "{message}"
    );
}

/// The counterweight: the same schema boots once the connector is there, with no
/// function definition for it.
#[tokio::test]
async fn a_connector_present_in_module_dir_boots_with_no_function_definition() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pollOrders.ts"), "export default async () => {};").unwrap();
    boot(schema_with(orders()), Some(functions_in(dir.path())), true).await.unwrap();
}

#[tokio::test]
async fn an_enabled_source_with_no_functions_section_refuses_the_boot() {
    let message = boot(schema_with(orders()), None, true)
        .await
        .expect_err("a connector with nowhere to load from must refuse the boot")
        .to_string();
    assert!(message.contains("\"orders\"") && message.contains("module_dir"), "{message}");
}

/// The schema release-smoke boots with the full platform surface boots here too.
///
/// Release-smoke runs only on `release/*` and tags, so when #1399 made a connector-less
/// source refuse the boot, the fixture's `example_source` broke the smoke and nothing
/// before the release cut could see it. This leg runs on every push. `module_dir` is
/// resolved from the working directory, which in the smoke is the repository root.
#[tokio::test]
async fn the_release_smoke_source_schema_boots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extended = crate::schema::CompiledSchemaLoader::new(
        root.join("docker/e2e/schema.with-source.compiled.json"),
    )
    .load_extended()
    .await
    .unwrap();
    assert!(
        extended.schema.sources.iter().any(|source| source.enabled),
        "the fixture must keep an enabled source, or this boot proves nothing"
    );
    let functions = extended.functions.map(|mut functions| {
        assert!(
            functions.module_dir.is_relative(),
            "the smoke resolves module_dir from the repository root"
        );
        functions.module_dir = root.join(&functions.module_dir);
        functions
    });
    boot(extended.schema, functions, true).await.unwrap();
}

/// A disabled source, or a disabled scheduler, needs no connector.
#[tokio::test]
async fn a_source_that_will_not_run_needs_no_connector() {
    let dir = tempfile::tempdir().unwrap();
    boot(schema_with(orders().disabled()), Some(functions_in(dir.path())), true)
        .await
        .unwrap();
    boot(schema_with(orders()), Some(functions_in(dir.path())), false)
        .await
        .unwrap();
    boot(schema_with(orders()), None, false).await.unwrap();
}
