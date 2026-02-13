//! Span creation utilities for distributed tracing
//!
//! Provides convenient functions for creating spans at different levels
//! of the event processing pipeline.

use crate::event::Event;

/// Create event processing root span
///
/// Creates the root span for an event processing operation.
/// All other spans during event processing should be children of this span.
///
/// # Arguments
///
/// * `event` - The event being processed
///
/// # Returns
///
/// A formatted span name and attributes for the event processing span
///
/// # Example
///
/// ```ignore
/// use fraiseql_observers::tracing::create_event_span;
///
/// let span = create_event_span(&event);
/// // Use span for tracing
/// ```
pub fn create_event_span(event: &Event) -> (String, Vec<(&'static str, String)>) {
    let attributes = vec![
        ("event_id", event.id.to_string()),
        ("entity_type", event.entity.type_name().to_string()),
        ("event_kind", format!("{:?}", event.kind)),
    ];

    ("process_event".to_string(), attributes)
}

/// Create action execution span
///
/// Creates a span for action execution within event processing.
///
/// # Arguments
///
/// * `action_type` - Name of the action type
/// * `action_count` - Total number of actions being executed
///
/// # Returns
///
/// A formatted span name and attributes for the action execution span
pub fn create_action_span(action_type: &str, action_count: usize) -> (String, Vec<(&'static str, String)>) {
    let attributes = vec![
        ("action_type", action_type.to_string()),
        ("action_count", action_count.to_string()),
    ];

    ("execute_action".to_string(), attributes)
}

/// Create phase span (checkpoint, condition eval, etc.)
///
/// Creates spans for specific phases of event processing.
///
/// # Arguments
///
/// * `phase_name` - Name of the phase
/// * `attributes` - Phase-specific attributes
///
/// # Returns
///
/// A formatted span name and attributes
pub fn create_phase_span(
    phase_name: &str,
    attributes: Vec<(&'static str, String)>,
) -> (String, Vec<(&'static str, String)>) {
    (phase_name.to_string(), attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityKind;
    use uuid::Uuid;

    #[test]
    fn test_create_event_span() {
        let event = Event {
            id: Uuid::new_v4(),
            entity: crate::entity::Entity {
                id: Uuid::new_v4(),
                entity_type: "Order".to_string(),
                data: serde_json::json!({}),
            },
            kind: EntityKind::Created,
            timestamp: std::time::SystemTime::now(),
        };

        let (span_name, attributes) = create_event_span(&event);

        assert_eq!(span_name, "process_event");
        assert!(!attributes.is_empty());

        let attr_names: Vec<_> = attributes.iter().map(|(k, _)| *k).collect();
        assert!(attr_names.contains(&"event_id"));
        assert!(attr_names.contains(&"entity_type"));
        assert!(attr_names.contains(&"event_kind"));
    }

    #[test]
    fn test_create_action_span() {
        let (span_name, attributes) = create_action_span("webhook", 3);

        assert_eq!(span_name, "execute_action");
        assert_eq!(attributes.len(), 2);

        let attr_map: std::collections::HashMap<_, _> = attributes.iter().cloned().collect();
        assert_eq!(attr_map.get("action_type"), Some(&"webhook".to_string()));
        assert_eq!(attr_map.get("action_count"), Some(&"3".to_string()));
    }

    #[test]
    fn test_create_phase_span() {
        let attrs = vec![("status", "success".to_string())];
        let (span_name, attributes) = create_phase_span("checkpoint_load", attrs);

        assert_eq!(span_name, "checkpoint_load");
        assert_eq!(attributes.len(), 1);
    }
}
