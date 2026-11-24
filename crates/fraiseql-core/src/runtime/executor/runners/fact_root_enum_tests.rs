//! A fact-table root holds every enum a document writes to the enum its published input
//! type declares, as a list root does: an unknown member is refused as one, naming the enum
//! and its members, before any statement, rather than by the planner's or a parser's own
//! reading of the value.

use std::sync::Arc;

use super::{schema_with_fact_table, tenant_security_context};
use crate::runtime::{Executor, executor::test_support::CapturingMockAdapter};

/// The fixture's fact table, with its roots published, as a loaded schema publishes them.
fn published() -> crate::schema::CompiledSchema {
    let mut schema = schema_with_fact_table();
    schema.build_indexes();
    schema
}

/// What `document` is refused with, and whether a statement was issued.
async fn refused(document: &str) -> (String, Option<String>) {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let err = Executor::new(published(), adapter.clone())
        .execute_with_security(document, None, &tenant_security_context("tenant-abc"))
        .await
        .expect_err(document);
    (err.to_string(), adapter.captured_aggregate_sql())
}

const COLUMN: &str = r#"select: [{type: filter, name: "tenant_id", alias: "tenant_id"}]"#;

#[tokio::test]
async fn every_fact_root_enum_refuses_an_unknown_member() {
    let cases = [
        (
            "{ sales_aggregate(aggregates: [{count: true}], orderBy: [{field: nosuch}]) { count } }"
                .to_string(),
            "SalesAggregateOrderByField",
        ),
        (
            "{ sales_aggregate(aggregates: [{count: true}], orderBy: [{field: count, direction: \
             SIDEWAYS}]) { count } }"
                .to_string(),
            "SortDirection",
        ),
        (
            r#"{ sales_window(select: [{type: nosuch, name: "tenant_id", alias: "t"}]) }"#
                .to_string(),
            "WindowColumnKind",
        ),
        (
            format!("{{ sales_window({COLUMN}, windows: [{{function: {{type: nosuch}}, alias: \"r\"}}]) }}"),
            "WindowFunctionKind",
        ),
        (
            format!(
                "{{ sales_window({COLUMN}, windows: [{{function: {{type: rank}}, alias: \"r\", \
                 frame: {{frame_type: SIDEWAYS, start: {{type: current_row}}, end: {{type: \
                 current_row}}}}}}]) }}"
            ),
            "WindowFrameType",
        ),
        (
            format!(
                "{{ sales_window({COLUMN}, windows: [{{function: {{type: rank}}, alias: \"r\", \
                 frame: {{frame_type: ROWS, start: {{type: nosuch}}, end: {{type: \
                 current_row}}}}}}]) }}"
            ),
            "WindowFrameBoundaryKind",
        ),
        (
            format!(
                "{{ sales_window({COLUMN}, windows: [{{function: {{type: rank}}, alias: \"r\", \
                 frame: {{frame_type: ROWS, start: {{type: current_row}}, end: {{type: \
                 current_row}}, exclusion: nosuch}}}}]) }}"
            ),
            "WindowFrameExclusion",
        ),
        (
            format!(
                "{{ sales_window({COLUMN}, windows: [{{function: {{type: rank}}, alias: \"r\", \
                 partitionBy: [{{type: nosuch, name: \"tenant_id\"}}]}}]) }}"
            ),
            "WindowColumnKind",
        ),
        (
            format!("{{ sales_window({COLUMN}, orderBy: [{{field: \"tenant_id\", direction: SIDEWAYS}}]) }}"),
            "SortDirection",
        ),
    ];
    for (document, enum_name) in cases {
        let (message, sql) = refused(&document).await;
        assert!(
            message.contains(&format!("has enum type `{enum_name}`")),
            "{document}: {message}"
        );
        assert!(sql.is_none(), "{document}: reached the database: {sql:?}");
    }
}

/// The counterweight: every member the cases above replace is accepted.
#[tokio::test]
async fn the_same_documents_with_members_are_served() {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(published(), adapter);
    for document in [
        "{ sales_aggregate(aggregates: [{count: true}], orderBy: [{field: count, direction: \
         DESC}]) { count } }"
            .to_string(),
        format!(
            "{{ sales_window({COLUMN}, windows: [{{function: {{type: rank}}, alias: \"r\", \
             partitionBy: [{{type: filter, name: \"tenant_id\"}}], orderBy: [{{field: \
             \"tenant_id\", direction: DESC}}], frame: {{frame_type: ROWS, start: {{type: \
             unbounded_preceding}}, end: {{type: current_row}}, exclusion: no_others}}}}], \
             orderBy: [{{field: \"tenant_id\", direction: ASC}}]) }}"
        ),
    ] {
        executor
            .execute_with_security(&document, None, &tenant_security_context("tenant-abc"))
            .await
            .map_err(|e| format!("{document}: {e}"))
            .unwrap();
    }
}
