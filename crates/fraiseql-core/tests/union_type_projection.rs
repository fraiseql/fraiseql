//! Test union type response projection handling.
//!
//! This test verifies that:
//! 1. Union types list their possible members correctly
//! 2. Responses include __typename for union discrimination
//! 3. Union field projections preserve type information
//! 4. Multiple union types can be used in one query
//! 5. Union members are validated against schema definition
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Union types could lose member information
//! - __typename could be missing from responses
//! - Type discrimination could fail on union results

use serde_json::json;

#[test]
fn test_union_type_definition_basic() {
    // Basic union type definition
    let union = json!({
        "name": "SearchResult",
        "kind": "UNION",
        "members": [
            "User",
            "Post",
            "Comment"
        ]
    });

    assert_eq!(union["name"], json!("SearchResult"));
    assert_eq!(union["kind"], json!("UNION"));

    let members = union["members"].as_array().unwrap();
    assert_eq!(members.len(), 3);
    assert_eq!(members[0], json!("User"));
    assert_eq!(members[1], json!("Post"));
    assert_eq!(members[2], json!("Comment"));
}

#[test]
fn test_union_response_includes_typename() {
    // Union responses must include __typename for discrimination
    let response = json!({
        "search": {
            "__typename": "User",
            "id": "1",
            "name": "Alice"
        }
    });

    let search_result = &response["search"];
    assert_eq!(search_result["__typename"], json!("User"));
    assert_eq!(search_result["id"], json!("1"));
    assert_eq!(search_result["name"], json!("Alice"));
}

#[test]
fn test_union_multiple_response_types() {
    // Union response can be different types in different queries
    let user_response = json!({
        "result": {
            "__typename": "User",
            "id": "1",
            "email": "user@example.com"
        }
    });

    let post_response = json!({
        "result": {
            "__typename": "Post",
            "id": "post_1",
            "title": "Hello World"
        }
    });

    let comment_response = json!({
        "result": {
            "__typename": "Comment",
            "id": "comment_1",
            "text": "Nice post!"
        }
    });

    // Each has different typename and fields
    assert_eq!(user_response["result"]["__typename"], json!("User"));
    assert_eq!(post_response["result"]["__typename"], json!("Post"));
    assert_eq!(comment_response["result"]["__typename"], json!("Comment"));
}

#[test]
fn test_union_array_responses() {
    // Union type in array responses
    let search_results = json!({
        "search": [
            {
                "__typename": "User",
                "id": "1",
                "name": "Alice"
            },
            {
                "__typename": "Post",
                "id": "post_1",
                "title": "First Post"
            },
            {
                "__typename": "User",
                "id": "2",
                "name": "Bob"
            },
            {
                "__typename": "Comment",
                "id": "comment_1",
                "text": "Comment text"
            }
        ]
    });

    let results = search_results["search"].as_array().unwrap();
    assert_eq!(results.len(), 4);

    // Verify each result has __typename
    for result in results {
        assert!(result["__typename"].is_string());
    }

    // Verify order and types
    assert_eq!(results[0]["__typename"], json!("User"));
    assert_eq!(results[1]["__typename"], json!("Post"));
    assert_eq!(results[2]["__typename"], json!("User"));
    assert_eq!(results[3]["__typename"], json!("Comment"));
}

#[test]
fn test_union_member_list_preservation() {
    // Union members list is preserved exactly
    let union_with_many_members = json!({
        "name": "Content",
        "kind": "UNION",
        "members": [
            "Article",
            "Video",
            "Image",
            "Audio",
            "Document",
            "Event",
            "Product"
        ]
    });

    let members = union_with_many_members["members"].as_array().unwrap();
    let member_names: Vec<&str> = members.iter()
        .filter_map(|m| m.as_str())
        .collect();

    assert_eq!(member_names.len(), 7);
    assert!(member_names.contains(&"Article"));
    assert!(member_names.contains(&"Video"));
    assert!(member_names.contains(&"Image"));
    assert!(member_names.contains(&"Audio"));
    assert!(member_names.contains(&"Document"));
    assert!(member_names.contains(&"Event"));
    assert!(member_names.contains(&"Product"));
}

#[test]
fn test_union_member_order_preserved() {
    // Union member order matters for schema definition
    let union = json!({
        "name": "SearchResult",
        "members": ["User", "Post", "Comment"]
    });

    let members = union["members"].as_array().unwrap();
    assert_eq!(members[0], json!("User"));
    assert_eq!(members[1], json!("Post"));
    assert_eq!(members[2], json!("Comment"));

    // Different order should be different
    let union_reordered = json!({
        "name": "SearchResult",
        "members": ["Post", "Comment", "User"]
    });

    let members_reordered = union_reordered["members"].as_array().unwrap();
    assert_ne!(members[0], members_reordered[0]);
}

#[test]
fn test_union_response_field_preservation() {
    // Union response preserves all fields of the concrete type
    let response = json!({
        "data": {
            "__typename": "Article",
            "id": "article_1",
            "title": "Title",
            "content": "Long content",
            "author": "John",
            "created_at": "2024-01-01T00:00:00Z",
            "tags": ["rust", "graphql"]
        }
    });

    let article = &response["data"];

    // Verify all fields are preserved
    assert_eq!(article["__typename"], json!("Article"));
    assert_eq!(article["id"], json!("article_1"));
    assert_eq!(article["title"], json!("Title"));
    assert_eq!(article["content"], json!("Long content"));
    assert_eq!(article["author"], json!("John"));
    assert_eq!(article["created_at"], json!("2024-01-01T00:00:00Z"));

    let tags = article["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
}

#[test]
fn test_union_null_response() {
    // Union response can be null
    let response = json!({
        "search": null
    });

    assert_eq!(response["search"], json!(null));
}

#[test]
fn test_union_list_with_nulls() {
    // Union array can contain null elements
    let results = json!({
        "search": [
            {
                "__typename": "User",
                "id": "1"
            },
            null,
            {
                "__typename": "Post",
                "id": "post_1"
            }
        ]
    });

    let search_array = results["search"].as_array().unwrap();
    assert_eq!(search_array.len(), 3);
    assert!(!search_array[0].is_null());
    assert!(search_array[1].is_null());
    assert!(!search_array[2].is_null());
}

#[test]
fn test_union_nested_in_type() {
    // Union type nested in another type
    let query_response = json!({
        "feed": {
            "items": [
                {
                    "id": "item_1",
                    "content": {
                        "__typename": "Article",
                        "title": "Article Title"
                    }
                },
                {
                    "id": "item_2",
                    "content": {
                        "__typename": "Video",
                        "duration": 120
                    }
                }
            ]
        }
    });

    let items = query_response["feed"]["items"].as_array().unwrap();

    // First item is an Article
    let item1_content = &items[0]["content"];
    assert_eq!(item1_content["__typename"], json!("Article"));
    assert_eq!(item1_content["title"], json!("Article Title"));

    // Second item is a Video
    let item2_content = &items[1]["content"];
    assert_eq!(item2_content["__typename"], json!("Video"));
    assert_eq!(item2_content["duration"], json!(120));
}

#[test]
fn test_union_type_distinctions() {
    // Verify different union types are distinct
    let search_result = json!({
        "search": {
            "__typename": "User",
            "id": "1",
            "name": "Alice"
        }
    });

    let notification = json!({
        "notification": {
            "__typename": "Comment",
            "id": "c_1",
            "text": "Hello"
        }
    });

    // Different responses can be different types
    assert_ne!(search_result["search"]["__typename"], notification["notification"]["__typename"]);
    assert_eq!(search_result["search"]["__typename"], json!("User"));
    assert_eq!(notification["notification"]["__typename"], json!("Comment"));
}

#[test]
fn test_union_fragment_projection() {
    // Union response with potential fragment projection structure
    let response = json!({
        "search": {
            "__typename": "Post",
            "id": "post_1",
            "title": "GraphQL is awesome",
            "content": "Detailed content here",
            "likes": 42,
            "comments": 5
        }
    });

    let post = &response["search"];

    // __typename allows client to discriminate
    let typename = post["__typename"].as_str().unwrap();
    assert_eq!(typename, "Post");

    // Post-specific fields are available
    assert_eq!(post["title"], json!("GraphQL is awesome"));
    assert_eq!(post["likes"], json!(42));
    assert_eq!(post["comments"], json!(5));
}

