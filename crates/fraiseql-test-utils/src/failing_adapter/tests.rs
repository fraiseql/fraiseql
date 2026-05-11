use super::*;

    #[tokio::test]
    async fn test_default_returns_empty() {
        let adapter = FailingAdapter::new();
        let result = adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_canned_response() {
        let adapter = FailingAdapter::new()
            .with_response("v_user", vec![JsonbValue::new(serde_json::json!({"id": 1}))]);
        let result = adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_fail_on_query_zero() {
        let adapter = FailingAdapter::new().fail_on_query(0);
        let result = adapter.execute_where_query("v_user", None, None, None, None).await;
        assert!(result.is_err(), "expected Err when fail_on_query(0) is set, got: {result:?}");
    }

    #[tokio::test]
    async fn test_query_count_and_log() {
        let adapter = FailingAdapter::new();
        let _ = adapter.execute_where_query("v_user", None, None, None, None).await;
        let _ = adapter.execute_where_query("v_post", None, None, None, None).await;
        assert_eq!(adapter.query_count(), 2);
        assert_eq!(adapter.recorded_queries(), vec!["v_user", "v_post"]);
    }

    #[tokio::test]
    async fn test_reset() {
        let adapter = FailingAdapter::new().fail_on_query(0);
        assert!(
            adapter.execute_where_query("v_user", None, None, None, None).await.is_err(),
            "expected Err before reset"
        );
        adapter.reset();
        adapter
            .execute_where_query("v_user", None, None, None, None)
            .await
            .unwrap_or_else(|e| panic!("expected Ok after reset: {e}"));
        assert_eq!(adapter.query_count(), 1);
    }
