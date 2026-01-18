//! Integration tests for Rust predicates

mod common;

use common::connect_test_client;
use futures::StreamExt;

#[tokio::test]
async fn test_hybrid_filtering() {
    let client = connect_test_client().await.expect("connect");

    // This test uses the v_user view with JSON data that has notification settings
    // We filter for users with notifications enabled using both SQL and Rust predicates

    let mut stream = client
        .query::<serde_json::Value>("test.v_user")
        .where_sql("1 = 1") // Get all users
        .where_rust(|json| {
            // Rust: filter to only users with notifications enabled
            json["settings"]["notifications"].as_bool().unwrap_or(false)
        })
        .execute()
        .await
        .expect("query");

    let mut filtered_users = Vec::new();
    while let Some(item) = stream.next().await {
        let json = item.expect("item");
        filtered_users.push(json);
    }

    // Should get users with notifications=true (Alice and Carol based on seed data)
    assert!(
        !filtered_users.is_empty(),
        "should have results from hybrid filtering"
    );
    for user in &filtered_users {
        assert_eq!(
            user["settings"]["notifications"].as_bool(),
            Some(true),
            "all filtered users should have notifications enabled"
        );
    }

    println!(
        "Filtered {} users with notifications enabled",
        filtered_users.len()
    );
}
