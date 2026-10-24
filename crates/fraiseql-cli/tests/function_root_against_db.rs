//! Live-`PostgreSQL` integration tests for the function-root contract.
//!
//! `compile --database` and `validate --against-db` check the function a function root reads
//! against `pg_proc`: it binds the query's parameters, returns a set, returns the document
//! column as `jsonb`, and is not `VOLATILE`. These fixtures are real functions, so the volatility
//! and parameter counts are the catalog's, not a test's idea of them.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free test leg (even
//! under `--all-features`).

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::schema::{
    database_validator::{create_introspector, find_unbacked_sources},
    function_root_contract::validate_function_roots,
    pg_catalog::PgCatalog,
};
use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldType, InjectedParamSource, QueryDefinition, SourceKind,
};
use fraiseql_db::postgres::PostgresTlsConfig;
use tokio_postgres::NoTls;

const SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_1690_test CASCADE;
CREATE SCHEMA fql_1690_test;
CREATE FUNCTION fql_1690_test.fn_stable(p_asset_id uuid, p_tenant_id text)
  RETURNS TABLE(data jsonb) LANGUAGE sql STABLE AS $$ SELECT '{}'::jsonb $$;
CREATE FUNCTION fql_1690_test.fn_volatile(p_asset_id uuid, p_tenant_id text)
  RETURNS TABLE(data jsonb) LANGUAGE sql VOLATILE AS $$ SELECT '{}'::jsonb $$;
CREATE FUNCTION fql_1690_test.fn_scalar(p_asset_id uuid, p_tenant_id text)
  RETURNS jsonb LANGUAGE sql STABLE AS $$ SELECT '{}'::jsonb $$;
CREATE FUNCTION fql_1690_test.fn_unary(p_asset_id uuid)
  RETURNS TABLE(data jsonb) LANGUAGE sql STABLE AS $$ SELECT '{}'::jsonb $$;
";

const TEARDOWN: &str = "DROP SCHEMA IF EXISTS fql_1690_test CASCADE;";

/// Connect, run DDL. Returns `None` to signal skip (no/unreachable DB).
async fn setup() -> Option<String> {
    let url = fraiseql_test_support::try_database_url()?;
    let (client, connection) = match tokio_postgres::connect(&url, NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #1690 against-db test: cannot connect ({e})");
            return None;
        },
    };
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(SETUP).await.expect("setup DDL failed");
    Some(url)
}

async fn teardown(url: &str) {
    if let Ok((client, connection)) = tokio_postgres::connect(url, NoTls).await {
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let _ = client.batch_execute(TEARDOWN).await;
    }
}

/// A tenant-bound list read of `function`: `assetId`, then `tenant_id`.
fn root(name: &str, function: &str) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, "LineItem")
        .returning_list()
        .with_sql_source_function(format!("fql_1690_test.{function}"));
    query.arguments = vec![
        ArgumentDefinition::new("assetId", FieldType::Uuid),
        ArgumentDefinition::optional("where", FieldType::Json),
    ];
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
    query
}

fn schema(queries: Vec<QueryDefinition>) -> CompiledSchema {
    CompiledSchema {
        queries,
        ..Default::default()
    }
}

#[tokio::test]
async fn the_catalog_answers_what_a_function_root_reads() {
    let Some(url) = setup().await else {
        eprintln!("skipping #1690 against-db test: no DATABASE_URL");
        return;
    };
    let catalog = PgCatalog::connect(&url, &PostgresTlsConfig::default()).await.unwrap();
    let problems = |function: &'static str| {
        let catalog = &catalog;
        async move {
            validate_function_roots(&schema(vec![root("q", function)]), catalog)
                .await
                .unwrap()
        }
    };

    assert_eq!(problems("fn_stable").await, Vec::<String>::new());

    let volatile = problems("fn_volatile").await;
    let scalar = problems("fn_scalar").await;
    let unary = problems("fn_unary").await;
    let missing = problems("fn_missing").await;
    teardown(&url).await;

    assert!(volatile.len() == 1 && volatile[0].contains("VOLATILE"), "{volatile:?}");
    assert!(
        scalar.iter().any(|p| p.contains("returns one value")),
        "a scalar return has no rows, nor a `data` column: {scalar:?}"
    );
    assert!(unary.len() == 1 && unary[0].contains("takes 1"), "{unary:?}");
    assert!(missing.len() == 1 && missing[0].contains("no function"), "{missing:?}");
}

#[tokio::test]
async fn an_unbacked_function_root_is_reported_as_a_missing_function() {
    let Some(url) = setup().await else {
        eprintln!("skipping #1690 against-db test: no DATABASE_URL");
        return;
    };
    let introspector = create_introspector(&url, &PostgresTlsConfig::default()).await.unwrap();
    let unbacked = find_unbacked_sources(
        &schema(vec![root("backed", "fn_stable"), root("unbacked", "fn_missing")]),
        &introspector,
    )
    .await
    .unwrap();
    teardown(&url).await;

    let unbacked: Vec<(String, SourceKind)> =
        unbacked.into_iter().map(|p| (p.display_name(), p.kind)).collect();
    assert_eq!(
        unbacked,
        vec![("fql_1690_test.fn_missing".to_string(), SourceKind::Function)],
        "the backed function root is found in pg_proc, the other is named as a function"
    );
}
