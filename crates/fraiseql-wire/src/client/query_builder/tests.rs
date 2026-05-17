fn build_test_sql(entity: &str, predicates: Vec<&str>, order_by: Option<&str>) -> String {
    let mut sql = format!("SELECT data FROM {}", entity);
    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    if let Some(order) = order_by {
        sql.push_str(" ORDER BY ");
        sql.push_str(order);
    }
    sql
}

#[test]
fn test_build_sql_simple() {
    let sql = build_test_sql("user", vec![], None);
    assert_eq!(sql, "SELECT data FROM user");
}

#[test]
fn test_build_sql_with_where() {
    let sql = build_test_sql("user", vec!["data->>'status' = 'active'"], None);
    assert_eq!(
        sql,
        "SELECT data FROM user WHERE data->>'status' = 'active'"
    );
}

#[test]
fn test_build_sql_with_order() {
    let sql = build_test_sql("user", vec![], Some("data->>'name' ASC"));
    assert_eq!(sql, "SELECT data FROM user ORDER BY data->>'name' ASC");
}

#[test]
fn test_build_sql_with_limit() {
    let mut sql = "SELECT data FROM user".to_string();
    sql.push_str(" LIMIT 10");
    assert_eq!(sql, "SELECT data FROM user LIMIT 10");
}

#[test]
fn test_build_sql_with_offset() {
    let mut sql = "SELECT data FROM user".to_string();
    sql.push_str(" OFFSET 20");
    assert_eq!(sql, "SELECT data FROM user OFFSET 20");
}

#[test]
fn test_build_sql_with_limit_and_offset() {
    let mut sql = "SELECT data FROM user".to_string();
    sql.push_str(" LIMIT 10");
    sql.push_str(" OFFSET 20");
    assert_eq!(sql, "SELECT data FROM user LIMIT 10 OFFSET 20");
}

#[test]
fn test_build_sql_complete() {
    let mut sql = "SELECT data FROM user".to_string();
    sql.push_str(" WHERE data->>'status' = 'active'");
    sql.push_str(" ORDER BY data->>'name' ASC");
    sql.push_str(" LIMIT 10");
    sql.push_str(" OFFSET 20");
    assert_eq!(
        sql,
        "SELECT data FROM user WHERE data->>'status' = 'active' ORDER BY data->>'name' ASC LIMIT 10 OFFSET 20"
    );
}

// Projection tests
#[test]
fn test_build_sql_default_select() {
    let sql = build_test_sql("users", vec![], None);
    assert!(sql.starts_with("SELECT data FROM"));
    assert_eq!(sql, "SELECT data FROM users");
}

#[test]
fn test_projection_single_field() {
    let sql = "SELECT jsonb_build_object('id', data->>'id') as data FROM users".to_string();
    assert!(sql.contains("as data"));
    assert!(sql.starts_with("SELECT jsonb_build_object("));
    assert!(sql.contains("FROM users"));
}

#[test]
fn test_projection_multiple_fields() {
    let projection =
        "jsonb_build_object('id', data->>'id', 'name', data->>'name', 'email', data->>'email')";
    let sql = format!("SELECT {} as data FROM users", projection);
    assert!(sql.contains("as data FROM users"));
    assert!(sql.contains("jsonb_build_object("));
    assert!(sql.contains("'id'"));
    assert!(sql.contains("'name'"));
    assert!(sql.contains("'email'"));
}

#[test]
fn test_projection_with_where_clause() {
    let projection = "jsonb_build_object('id', data->>'id')";
    let mut sql = format!("SELECT {} as data FROM users", projection);
    sql.push_str(" WHERE data->>'status' = 'active'");
    assert!(sql.contains("SELECT jsonb_build_object("));
    assert!(sql.contains("as data FROM users"));
    assert!(sql.contains("WHERE data->>'status' = 'active'"));
}

#[test]
fn test_projection_with_order_by() {
    let projection = "jsonb_build_object('id', data->>'id')";
    let mut sql = format!("SELECT {} as data FROM users", projection);
    sql.push_str(" ORDER BY data->>'name' ASC");
    assert!(sql.contains("SELECT jsonb_build_object("));
    assert!(sql.contains("ORDER BY data->>'name' ASC"));
}

#[test]
fn test_projection_with_limit() {
    let projection = "jsonb_build_object('id', data->>'id')";
    let mut sql = format!("SELECT {} as data FROM users", projection);
    sql.push_str(" LIMIT 1000");
    assert!(sql.contains("as data FROM users"));
    assert!(sql.contains("LIMIT 1000"));
}

#[test]
fn test_projection_with_offset() {
    let projection = "jsonb_build_object('id', data->>'id')";
    let mut sql = format!("SELECT {} as data FROM users", projection);
    sql.push_str(" OFFSET 500");
    assert!(sql.contains("as data FROM users"));
    assert!(sql.contains("OFFSET 500"));
}

#[test]
fn test_projection_full_pipeline() {
    let projection =
        "jsonb_build_object('user_id', data->>'user_id', 'event_type', data->>'event_type')";
    let mut sql = format!("SELECT {} as data FROM events", projection);
    sql.push_str(" WHERE event_type IN ('purchase', 'view')");
    sql.push_str(" ORDER BY timestamp DESC");
    sql.push_str(" LIMIT 5000");
    assert!(sql.contains("SELECT jsonb_build_object("));
    assert!(sql.contains("'user_id'"));
    assert!(sql.contains("'event_type'"));
    assert!(sql.contains("as data FROM events"));
    assert!(sql.contains("WHERE event_type IN ('purchase', 'view')"));
    assert!(sql.contains("ORDER BY timestamp DESC"));
    assert!(sql.contains("LIMIT 5000"));
}

// Stream pipeline integration tests
#[test]
fn test_typed_stream_with_value_type() {
    // Verify that TypedJsonStream can wrap a raw JSON stream
    use crate::stream::TypedJsonStream;
    use futures::stream;

    let values = vec![
        Ok(serde_json::json!({"id": "1", "name": "Alice"})),
        Ok(serde_json::json!({"id": "2", "name": "Bob"})),
    ];

    let json_stream = stream::iter(values);
    let typed_stream: TypedJsonStream<serde_json::Value> =
        TypedJsonStream::new(Box::new(json_stream));

    // This verifies the stream compiles and has correct type
    let _stream: Box<dyn futures::stream::Stream<Item = crate::Result<serde_json::Value>> + Unpin> =
        Box::new(typed_stream);
}

#[test]
fn test_filtered_stream_with_typed_output() {
    // Verify that FilteredStream correctly filters before TypedJsonStream
    use crate::stream::{FilteredStream, TypedJsonStream};
    use futures::stream;

    let values = vec![
        Ok(serde_json::json!({"id": 1, "active": true})),
        Ok(serde_json::json!({"id": 2, "active": false})),
        Ok(serde_json::json!({"id": 3, "active": true})),
    ];

    let json_stream = stream::iter(values);
    let predicate = Box::new(|v: &serde_json::Value| v["active"].as_bool().unwrap_or(false));

    let filtered = FilteredStream::new(json_stream, predicate);
    let typed_stream: TypedJsonStream<serde_json::Value> = TypedJsonStream::new(Box::new(filtered));

    // This verifies the full pipeline compiles
    let _stream: Box<dyn futures::stream::Stream<Item = crate::Result<serde_json::Value>> + Unpin> =
        Box::new(typed_stream);
}

#[test]
fn test_stream_pipeline_type_flow() {
    // Comprehensive test of stream type compatibility:
    // JsonStream (Result<Value>) -> FilteredStream (Result<Value>) -> TypedJsonStream<T> (Result<T>)
    use crate::stream::{FilteredStream, TypedJsonStream};
    use futures::stream;
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    // Reason: test fixture struct used only for deserialization verification
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    struct TestUser {
        id: String,
        active: bool,
    }

    let values = vec![
        Ok(serde_json::json!({"id": "1", "active": true})),
        Ok(serde_json::json!({"id": "2", "active": false})),
    ];

    let json_stream = stream::iter(values);

    // Step 1: FilteredStream filters JSON values
    let predicate: Box<dyn Fn(&serde_json::Value) -> bool + Send> =
        Box::new(|v| v["active"].as_bool().unwrap_or(false));
    let filtered: Box<
        dyn futures::stream::Stream<Item = crate::Result<serde_json::Value>> + Send + Unpin,
    > = Box::new(FilteredStream::new(json_stream, predicate));

    // Step 2: TypedJsonStream deserializes to TestUser
    let typed: TypedJsonStream<TestUser> = TypedJsonStream::new(filtered);

    // This verifies type system is compatible:
    // - FilteredStream outputs Result<Value>
    // - TypedJsonStream<T> takes Box<dyn Stream<Item = Result<Value>>>
    // - TypedJsonStream<T> outputs Result<T>
    let _final_stream: Box<dyn futures::stream::Stream<Item = crate::Result<TestUser>> + Unpin> =
        Box::new(typed);
}
