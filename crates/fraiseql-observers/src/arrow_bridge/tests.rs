use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::event::EventKind;

#[test]
fn test_convert_single_event() {
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 100.50}),
    )
    .with_user_id("user-1".to_string());

    let converter = EventToArrowConverter::new(10_000);
    let batch = converter.convert_events(&[event]).unwrap();

    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 8);
}

#[test]
fn test_convert_multiple_events() {
    let events = vec![
        EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 100.50}),
        )
        .with_user_id("user-1".to_string()),
        EntityEvent::new(
            EventKind::Updated,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"total": 200.00}),
        ),
        EntityEvent::new(
            EventKind::Deleted,
            "Product".to_string(),
            Uuid::new_v4(),
            json!({"id": 42}),
        ),
    ];

    let converter = EventToArrowConverter::new(10_000);
    let batch = converter.convert_events(&events).unwrap();

    assert_eq!(batch.num_rows(), 3);
    assert_eq!(batch.num_columns(), 8);
}

#[test]
fn test_convert_empty_batch() {
    let converter = EventToArrowConverter::new(10_000);
    let batch = converter.convert_events(&[]).unwrap();

    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.num_columns(), 8);
}

#[test]
fn test_null_user_id() {
    let event =
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({}));

    let converter = EventToArrowConverter::new(10_000);
    let batch = converter.convert_events(&[event]).unwrap();

    assert_eq!(batch.num_rows(), 1);
    // user_id column should have a null value
}

#[test]
fn test_event_types() {
    let events = vec![
        EntityEvent::new(EventKind::Created, "Order".to_string(), Uuid::new_v4(), json!({})),
        EntityEvent::new(EventKind::Updated, "Order".to_string(), Uuid::new_v4(), json!({})),
        EntityEvent::new(EventKind::Deleted, "Order".to_string(), Uuid::new_v4(), json!({})),
        EntityEvent::new(EventKind::Custom, "Order".to_string(), Uuid::new_v4(), json!({})),
    ];

    let converter = EventToArrowConverter::new(10_000);
    let batch = converter.convert_events(&events).unwrap();

    assert_eq!(batch.num_rows(), 4);
}

#[test]
fn test_complex_json_data() {
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({
            "total": 100.50,
            "items": [
                {"name": "Widget", "price": 50.25},
                {"name": "Gadget", "price": 50.25}
            ],
            "shipping": {
                "address": "123 Main St",
                "city": "Anytown",
                "country": "US"
            }
        }),
    );

    let converter = EventToArrowConverter::new(10_000);
    let batch = converter.convert_events(&[event]).unwrap();

    assert_eq!(batch.num_rows(), 1);
    // Complex JSON should be serialized to string
}
