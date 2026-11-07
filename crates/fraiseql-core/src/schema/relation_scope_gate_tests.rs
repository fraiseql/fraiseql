//! Unit tests for the relation-scope check that need no database.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use serde_json::json;

use super::*;
use crate::{
    compiler::fact_table::FactTableMetadata,
    error::FraiseQLError,
    runtime::test_support::MockAdapter,
    schema::{SessionVariableMapping, SessionVariableSource, SessionVariablesConfig},
};

fn fact_table(name: &str, row_scope: &str) -> FactTableMetadata {
    serde_json::from_value(json!({
        "table_name": name,
        "row_scope": row_scope,
        "measures": [],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [],
    }))
    .unwrap()
}

fn schema_with(tables: &[(&str, &str)]) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    for (name, row_scope) in tables {
        schema.add_fact_table((*name).to_string(), fact_table(name, row_scope));
    }
    schema.session_variables = SessionVariablesConfig {
        variables: vec![SessionVariableMapping {
            name:   "app.tenant_id".to_string(),
            source: SessionVariableSource::Jwt {
                claim: "tid".to_string(),
            },
        }],
        ..SessionVariablesConfig::default()
    };
    schema
}

#[tokio::test]
async fn a_schema_without_a_relation_scoped_fact_table_reads_no_catalog() {
    let adapter = MockAdapter::new(vec![]);
    let schema = schema_with(&[("tf_server", "server"), ("tf_shared", "shared")]);

    refuse_unfiltered_relation_scopes(&adapter, &schema).await.unwrap();

    assert!(adapter.raw_sql().is_empty(), "{:?}", adapter.raw_sql());
}

#[tokio::test]
async fn a_relation_scope_with_no_server_set_setting_is_refused_without_reading_the_catalog() {
    let adapter = MockAdapter::new(vec![]);
    let mut schema = schema_with(&[("tf_scoped", "relation")]);
    schema.session_variables.variables[0].source = SessionVariableSource::Header {
        header: "x-tenant".to_string(),
    };

    let result = refuse_unfiltered_relation_scopes(&adapter, &schema).await;

    match result {
        Err(FraiseQLError::Configuration { message }) => {
            assert!(
                message.contains("tf_scoped") && message.contains("no session variable"),
                "{message}"
            );
        },
        other => panic!("a relation scope with nothing to filter on must be refused: {other:?}"),
    }
    assert!(adapter.raw_sql().is_empty(), "{:?}", adapter.raw_sql());
}

#[tokio::test]
async fn a_relation_the_catalog_does_not_answer_for_is_refused() {
    let adapter = MockAdapter::new(vec![]);
    let schema = schema_with(&[("tf_scoped", "relation")]);

    let result = refuse_unfiltered_relation_scopes(&adapter, &schema).await;

    match result {
        Err(FraiseQLError::Configuration { message }) => {
            assert!(message.contains("tf_scoped"), "{message}");
        },
        other => panic!("a relation the catalog cannot show must be refused: {other:?}"),
    }
}
