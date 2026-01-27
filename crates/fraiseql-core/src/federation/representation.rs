//! Entity representation parsing for _Any scalar.

use super::types::{EntityRepresentation, FederationMetadata};
use serde_json::Value;

/// Parse entity representations from _entities input
pub fn parse_representations(
    input: &Value,
    metadata: &FederationMetadata,
) -> Result<Vec<EntityRepresentation>, String> {
    let array = input.as_array()
        .ok_or_else(|| "Representations must be array".to_string())?;

    let mut reps = Vec::new();

    for (idx, item) in array.iter().enumerate() {
        let mut rep = EntityRepresentation::from_any(item)
            .map_err(|e| format!("Representation {}: {}", idx, e))?;

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

/// Validate entity representations
pub fn validate_representations(
    reps: &[EntityRepresentation],
    metadata: &FederationMetadata,
) -> Result<(), Vec<String>> {
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
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
