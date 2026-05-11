use super::*;

#[tokio::test]
async fn test_tcp_connect_failure() {
    let result = Transport::connect_tcp("localhost", 9999).await;
    assert!(
        result.is_err(),
        "expected Err for connection to closed port 9999, got: {result:?}"
    );
}
