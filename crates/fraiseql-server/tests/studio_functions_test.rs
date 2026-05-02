//! Function operations at /admin/v1/functions/*
#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use fraiseql_server::routes::studio::function_ops::{FunctionListResponse, SecretSetRequest};

#[test]
fn test_function_list_response_shape() {
    let resp = FunctionListResponse { functions: vec![] };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"functions\""));
}

#[test]
fn test_secret_set_request_parses() {
    let input = r#"{"value": "supersecret"}"#;
    let req: SecretSetRequest = serde_json::from_str(input).unwrap();
    assert_eq!(req.value, "supersecret");
}
