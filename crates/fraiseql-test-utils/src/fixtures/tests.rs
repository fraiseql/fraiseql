use super::*;

#[test]
fn test_sample_user() {
    let user = sample_user();
    assert_eq!(user["name"], "Test User");
    assert_eq!(user["email"], "test@example.com");
}

#[test]
fn test_sample_query_response() {
    let response = sample_query_response();
    assert_eq!(response["data"]["user"]["name"], "John Doe");
}

#[test]
fn test_sample_error_response() {
    let error = sample_error_response("Something went wrong");
    assert_eq!(error["errors"][0]["message"], "Something went wrong");
}
