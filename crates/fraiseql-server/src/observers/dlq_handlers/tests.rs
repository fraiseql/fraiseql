use super::*;

#[test]
fn delivery_status_summary_serializes() {
    let summary = DeliveryStatusSummary {
        running:          true,
        observer_count:   3,
        events_processed: 42,
        errors:           1,
        dlq_count:        2,
    };
    let json = serde_json::to_value(&summary).expect("serialize");
    assert_eq!(json["running"], true);
    assert_eq!(json["dlq_count"], 2);
    assert_eq!(json["events_processed"], 42);
}

#[test]
fn dlq_list_response_serializes() {
    let response = DlqListResponse {
        items:  vec![],
        total:  0,
        limit:  50,
        offset: 0,
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert_eq!(json["total"], 0);
    assert_eq!(json["limit"], 50);
}

#[test]
fn retry_response_serializes() {
    let response = RetryResponse {
        success: true,
        item_id: Uuid::nil(),
        message: "ok".to_string(),
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert_eq!(json["success"], true);
}

#[test]
fn retry_all_response_serializes() {
    let response = RetryAllResponse {
        items_retried: 5,
        items_failed:  1,
        message:       "done".to_string(),
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert_eq!(json["items_retried"], 5);
    assert_eq!(json["items_failed"], 1);
}

#[test]
fn default_limit_is_50() {
    assert_eq!(default_limit(), 50);
}
