use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
pub struct CascadeSelections {
    pub fields: Vec<String>,
    #[serde(default)]
    pub updated: Option<FieldSelections>,
    #[serde(default)]
    pub deleted: Option<FieldSelections>,
    #[serde(default)]
    pub invalidations: Option<FieldSelections>,
    #[serde(default)]
    pub metadata: Option<FieldSelections>,
}

#[derive(Debug, Deserialize)]
pub struct FieldSelections {
    pub fields: Vec<String>,
    #[serde(default)]
    pub entity_selections: Option<EntitySelections>,
}

#[derive(Debug, Deserialize)]
pub struct EntitySelections {
    #[serde(flatten)]
    pub type_selections: std::collections::HashMap<String, Vec<String>>,
}

pub fn filter_cascade_by_selections(
    cascade: &Value,
    selections: &CascadeSelections,
    auto_camel_case: bool,
) -> Result<Value, String> {
    if selections.fields.is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    let cascade_obj = match cascade {
        Value::Object(obj) => obj,
        _ => return Err("CASCADE must be an object".to_string()),
    };

    let mut filtered = Map::with_capacity(selections.fields.len());

    for field_name in &selections.fields {
        let key = convert_field_name(field_name, auto_camel_case);

        if let Some(value) = cascade_obj.get(&key) {
            let filtered_value = match field_name.as_str() {
                "updated" => filter_updated_field(value, selections.updated.as_ref())?,
                "deleted" => filter_simple_field(value, selections.deleted.as_ref())?,
                "invalidations" => filter_simple_field(value, selections.invalidations.as_ref())?,
                "metadata" => filter_simple_field(value, selections.metadata.as_ref())?,
                _ => value.clone(),
            };

            filtered.insert(key, filtered_value);
        }
    }

    Ok(Value::Object(filtered))
}

fn filter_updated_field(
    value: &Value,
    field_selections: Option<&FieldSelections>,
) -> Result<Value, String> {
    let Some(selections) = field_selections else {
        return Ok(value.clone());
    };

    if let Value::Array(entities) = value {
        let filtered_entities: Vec<Value> = entities
            .iter()
            .map(|entity| filter_entity_fields(entity, &selections.fields))
            .collect::<Result<_, _>>()?;

        Ok(Value::Array(filtered_entities))
    } else {
        Ok(value.clone())
    }
}

fn filter_simple_field(
    value: &Value,
    field_selections: Option<&FieldSelections>,
) -> Result<Value, String> {
    let Some(selections) = field_selections else {
        return Ok(value.clone());
    };

    if let Value::Array(items) = value {
        let filtered_items: Vec<Value> = items
            .iter()
            .map(|item| filter_object_fields(item, &selections.fields))
            .collect::<Result<_, _>>()?;

        Ok(Value::Array(filtered_items))
    } else if let Value::Object(_) = value {
        filter_object_fields(value, &selections.fields)
    } else {
        Ok(value.clone())
    }
}

fn filter_entity_fields(entity: &Value, fields: &[String]) -> Result<Value, String> {
    let entity_obj = match entity {
        Value::Object(obj) => obj,
        _ => return Ok(entity.clone()),
    };

    let mut filtered = Map::new();

    for field in fields {
        if let Some(value) = entity_obj.get(field) {
            filtered.insert(field.clone(), value.clone());
        }
    }

    if !filtered.contains_key("__typename") {
        if let Some(typename) = entity_obj.get("__typename") {
            filtered.insert("__typename".to_string(), typename.clone());
        }
    }

    Ok(Value::Object(filtered))
}

fn filter_object_fields(obj: &Value, fields: &[String]) -> Result<Value, String> {
    let obj_map = match obj {
        Value::Object(map) => map,
        _ => return Ok(obj.clone()),
    };

    let mut filtered = Map::new();

    for field in fields {
        if let Some(value) = obj_map.get(field) {
            filtered.insert(field.clone(), value.clone());
        }
    }

    Ok(Value::Object(filtered))
}

fn convert_field_name(field_name: &str, auto_camel_case: bool) -> String {
    if !auto_camel_case {
        return field_name.to_string();
    }

    let mut result = String::new();
    let mut capitalize_next = false;

    for (i, ch) in field_name.chars().enumerate() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else if i == 0 {
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_field_name_camel_case() {
        assert_eq!(convert_field_name("updated", true), "updated");
        assert_eq!(convert_field_name("affected_count", true), "affectedCount");
        assert_eq!(convert_field_name("query_name", true), "queryName");
    }

    #[test]
    fn test_convert_field_name_no_camel_case() {
        assert_eq!(convert_field_name("updated", false), "updated");
        assert_eq!(convert_field_name("affected_count", false), "affected_count");
    }

    #[test]
    fn test_filter_cascade_empty_selections() {
        let cascade = serde_json::json!({
            "updated": [],
            "deleted": [],
        });

        let selections = CascadeSelections {
            fields: vec![],
            updated: None,
            deleted: None,
            invalidations: None,
            metadata: None,
        };

        let result = filter_cascade_by_selections(&cascade, &selections, false).unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[test]
    fn test_filter_cascade_single_field() {
        let cascade = serde_json::json!({
            "updated": [{"id": "1"}],
            "deleted": [{"id": "2"}],
            "metadata": {"affectedCount": 2}
        });

        let selections = CascadeSelections {
            fields: vec!["metadata".to_string()],
            updated: None,
            deleted: None,
            invalidations: None,
            metadata: Some(FieldSelections {
                fields: vec!["affectedCount".to_string()],
                entity_selections: None,
            }),
        };

        let result = filter_cascade_by_selections(&cascade, &selections, false).unwrap();
        let result_obj = result.as_object().unwrap();

        assert!(result_obj.contains_key("metadata"));
        assert!(!result_obj.contains_key("updated"));
        assert!(!result_obj.contains_key("deleted"));
    }
}
