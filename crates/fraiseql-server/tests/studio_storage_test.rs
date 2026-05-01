//! Cycle 5 tests: Storage browser at /admin/v1/storage/*
#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use fraiseql_server::routes::studio::storage_browser::{ObjectListResponse, PresignRequest};

#[test]
fn test_object_list_response_shape() {
    let resp = ObjectListResponse {
        objects: vec![],
        total: 0,
        page: 1,
        page_size: 50,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"objects\""));
    assert!(json.contains("\"total\""));
}

#[test]
fn test_presign_request_parses() {
    let input = r#"{"bucket": "avatars", "key": "u1.png", "expires_in_secs": 3600}"#;
    let req: PresignRequest = serde_json::from_str(input).unwrap();
    assert_eq!(req.bucket, "avatars");
    assert_eq!(req.key, "u1.png");
    assert_eq!(req.expires_in_secs, 3600);
}
