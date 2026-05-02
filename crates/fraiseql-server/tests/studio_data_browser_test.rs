//! Data browser backend at /admin/v1/data/*
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions

use fraiseql_server::routes::studio::data::{
    DataBrowserQuery, DataQueryResponse, FilterOp, MutateOperation, SortDir,
};

/// `DataQueryResponse` must serialize with the shape agreed with the Luxen UI author.
#[test]
fn test_data_query_response_shape() {
    let resp = DataQueryResponse {
        rows: vec![serde_json::json!({"id": 1, "name": "Alice"})],
        total: 42,
        page: 1,
        page_size: 50,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"rows\""));
    assert!(json.contains("\"total\""));
    assert!(json.contains("\"page\""));
    assert!(json.contains("\"page_size\""));
}

/// `DataBrowserQuery` must parse `filter` and `sort` fields.
#[test]
fn test_data_browser_query_deserialization() {
    let input = r#"{
        "page": 2,
        "page_size": 25,
        "filter": [{"field": "name", "op": "eq", "value": "Alice"}],
        "sort": [{"field": "id", "dir": "asc"}]
    }"#;
    let q: DataBrowserQuery = serde_json::from_str(input).unwrap();
    assert_eq!(q.page, 2);
    assert_eq!(q.page_size, 25);
    assert_eq!(q.filter.len(), 1);
    assert_eq!(q.filter[0].field, "name");
    assert!(matches!(q.filter[0].op, FilterOp::Eq));
    assert_eq!(q.sort.len(), 1);
    assert_eq!(q.sort[0].field, "id");
    assert!(matches!(q.sort[0].dir, SortDir::Asc));
}

/// Filter operators deserialize correctly.
#[test]
fn test_filter_op_variants() {
    let ops = [
        (r#""eq""#, FilterOp::Eq),
        (r#""ne""#, FilterOp::Ne),
        (r#""lt""#, FilterOp::Lt),
        (r#""lte""#, FilterOp::Lte),
        (r#""gt""#, FilterOp::Gt),
        (r#""gte""#, FilterOp::Gte),
        (r#""contains""#, FilterOp::Contains),
    ];
    for (json_val, expected) in ops {
        let op: FilterOp = serde_json::from_str(json_val).unwrap();
        assert!(
            std::mem::discriminant(&op) == std::mem::discriminant(&expected),
            "FilterOp::{expected:?} must round-trip"
        );
    }
}

/// `MutateOperation` covers insert/update/delete.
#[test]
fn test_mutate_operation_variants() {
    let ops = [r#""insert""#, r#""update""#, r#""delete""#];
    for op_json in ops {
        let op: MutateOperation = serde_json::from_str(op_json).unwrap();
        let _ = op; // just verify it compiles and parses
    }
}

/// Default `DataBrowserQuery` uses page 1 and `page_size` 50.
#[test]
fn test_data_browser_query_defaults() {
    let q: DataBrowserQuery = serde_json::from_str("{}").unwrap();
    assert_eq!(q.page, 1);
    assert_eq!(q.page_size, 50);
    assert!(q.filter.is_empty());
    assert!(q.sort.is_empty());
}
