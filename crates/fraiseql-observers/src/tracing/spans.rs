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
/// ```no_run
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
