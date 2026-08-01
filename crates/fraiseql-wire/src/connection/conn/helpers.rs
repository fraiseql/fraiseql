//! Internal helper functions for connection operations

/// Constrain a caller-supplied entity name to a safe Prometheus label value.
///
/// The entity used to be re-derived from the rendered SQL text, where a
/// heuristic scan could land inside a user-supplied filter literal and mint
/// unbounded label cardinality (#877). The caller now passes the entity it
/// already knows; this guard keeps the label a bounded identifier even if a
/// caller passes something exotic: anything not shaped like
/// `[A-Za-z_][A-Za-z0-9_]*` becomes `"unknown"`.
pub(super) fn metrics_entity_label(entity: &str) -> String {
    let mut chars = entity.chars();
    let valid = match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    };
    if valid {
        entity.to_string()
    } else {
        "unknown".to_string()
    }
}
