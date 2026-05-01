//! Cycle 4 tests: Auth user management at /admin/v1/users/*
#![allow(clippy::unwrap_used)]
#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use fraiseql_server::routes::studio::auth_users::{UserInviteRequest, UserListResponse};

#[test]
fn test_user_list_response_shape() {
    let resp = UserListResponse {
        users: vec![],
        total: 0,
        page: 1,
        page_size: 50,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"users\""));
    assert!(json.contains("\"total\""));
    assert!(json.contains("\"page\""));
    assert!(json.contains("\"page_size\""));
}

#[test]
fn test_user_invite_request_parses() {
    let input = r#"{"email": "test@example.com"}"#;
    let req: UserInviteRequest = serde_json::from_str(input).unwrap();
    assert_eq!(req.email, "test@example.com");
}
