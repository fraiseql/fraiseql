//! Entity representation parsing for _Any scalar.

use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

/// Maximum number of entity representations accepted in a single `_entities` call.
///
/// Each representation is parsed, validated against the schema, and resolved.
/// An uncapped batch lets a single request trigger unbounded work; 1 000 entries
/// is well above any legitimate use case while preventing accidental or intentional
/// runaway.
const MAX_ENTITIES_BATCH_SIZE: usize = 1_000;

use super::types::{EntityRepresentation, FederationMetadata};

/// Parse entity representations from _entities input.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the input is not an array or any
/// representation is missing the `__typename` field.
pub fn parse_representations(
    input: &Value,
    metadata: &FederationMetadata,
) -> Result<Vec<EntityRepresentation>> {
    let array = input.as_array().ok_or_else(|| FraiseQLError::Validation {
        message: "Representations must be an array".to_string(),
        path:    None,
    })?;

    if array.len() > MAX_ENTITIES_BATCH_SIZE {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Too many entity representations: {} (max {MAX_ENTITIES_BATCH_SIZE})",
                array.len()
            ),
            path:    None,
        });
    }

    let mut reps = Vec::new();

    for (idx, item) in array.iter().enumerate() {
        let mut rep =
            EntityRepresentation::from_any(item).map_err(|e| FraiseQLError::Validation {
                message: format!("Representation {idx}: {e}"),
                path:    None,
            })?;

        // Extract key fields based on metadata
        if let Some(fed_type) = metadata.types.iter().find(|t| t.name == rep.typename) {
            if let Some(key) = fed_type.keys.first() {
                rep.extract_key_fields(&key.fields);
            }
        }

        reps.push(rep);
    }

    Ok(reps)
}

/// Validate entity representations.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if any representation references an unknown
/// type or is missing required key fields.
pub fn validate_representations(
    reps: &[EntityRepresentation],
    metadata: &FederationMetadata,
) -> Result<()> {
    let mut errors = Vec::new();

    for rep in reps {
        // Check typename exists in schema
        if !metadata.types.iter().any(|t| t.name == rep.typename) {
            errors.push(format!("Type {} not found in federation metadata", rep.typename));
            continue;
        }

        // Check required key fields are present
        if let Some(fed_type) = metadata.types.iter().find(|t| t.name == rep.typename) {
            if let Some(key) = fed_type.keys.first() {
                for field in &key.fields {
                    if !rep.key_fields.contains_key(field) {
                        errors.push(format!(
                            "Type {}: key field '{}' missing in representation",
                            rep.typename, field
                        ));
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(FraiseQLError::Validation {
            message: format!("Invalid representations: {}", errors.join("; ")),
            path:    None,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_representations() {
        let input = json!([
            {"__typename": "User", "id": "123"},
            {"__typename": "User", "id": "456"},
        ]);

        let metadata = FederationMetadata::default();
        let reps = parse_representations(&input, &metadata).unwrap();

        assert_eq!(reps.len(), 2);
        assert_eq!(reps[0].typename, "User");
        assert_eq!(reps[1].typename, "User");
    }

    #[test]
    fn test_parse_representations_invalid() {
        let input = json!("not an array");

        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_representations_missing_typename() {
        let input = json!([
            {"id": "123"},
        ]);

        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);

        assert!(result.is_err());
    }

    // ── Batch-size guard tests ─────────────────────────────────────────────────

    #[test]
    fn test_parse_representations_at_max_accepted() {
        // MAX_ENTITIES_BATCH_SIZE items must be accepted.
        let items: Vec<_> = (0..MAX_ENTITIES_BATCH_SIZE)
            .map(|i| json!({"__typename": "User", "id": i.to_string()}))
            .collect();
        let input = Value::Array(items);
        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);
        assert!(result.is_ok(), "exactly MAX_ENTITIES_BATCH_SIZE reps must be accepted");
        assert_eq!(result.unwrap().len(), MAX_ENTITIES_BATCH_SIZE);
    }

    #[test]
    fn test_parse_representations_exceeding_max_rejected() {
        // MAX_ENTITIES_BATCH_SIZE + 1 items must be rejected before any parsing.
        let items: Vec<_> = (0..=MAX_ENTITIES_BATCH_SIZE)
            .map(|i| json!({"__typename": "User", "id": i.to_string()}))
            .collect();
        let input = Value::Array(items);
        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);
        assert!(result.is_err(), "batch exceeding max must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("1000") || msg.contains("1001"),
            "error must mention the count: {msg}"
        );
    }
}
