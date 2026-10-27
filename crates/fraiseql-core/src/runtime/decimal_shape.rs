//! Where a schema's `Decimal` values sit, for reading a row's text with their digits kept.
//!
//! A `Decimal` travels as a JSON string carrying its stored digits. A read that reaches one
//! takes the row as text and quotes each number at a `Decimal` position before parsing it
//! ([`quote_decimals`](crate::db::traits::quote_decimals)); [`DecimalIndex`] says where
//! those positions are, for a document of any type.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    db::traits::{DecimalFields, DecimalNode, DecimalShape},
    runtime::projection::stored_key_candidates,
    schema::{CompiledSchema, FieldType},
};

#[cfg(test)]
mod tests;

/// The fields of every type in a schema that leads to a `Decimal`, built once per schema.
#[derive(Debug, Clone, Default)]
pub struct DecimalIndex {
    types: Arc<HashMap<String, DecimalFields>>,
}

impl DecimalIndex {
    /// Index `schema`: each type that holds a `Decimal`, or a field leading to a type that
    /// does, with every key a document may hold such a field by.
    #[must_use]
    pub fn new(schema: &CompiledSchema) -> Self {
        let reaching = reaching_types(schema);
        let types = schema
            .types
            .iter()
            .filter(|t| reaching.contains(t.name.as_str()))
            .map(|t| {
                let mut fields = DecimalFields::new();
                for field in &t.fields {
                    let Some(node) = node_for(schema, &reaching, &field.field_type) else {
                        continue;
                    };
                    let (stored, camel) = stored_key_candidates(field.name.as_str());
                    let keys = [
                        Some(field.name.to_string()),
                        field.alias.clone(),
                        Some(stored),
                        camel,
                    ];
                    for key in keys.into_iter().flatten() {
                        fields.insert(key, node.clone());
                    }
                }
                (t.name.to_string(), fields)
            })
            .collect();
        Self {
            types: Arc::new(types),
        }
    }

    /// The shape of a document read as `type_name`, an object type, a union or an
    /// interface; `None` when no `Decimal` is reachable from it.
    #[must_use]
    pub fn shape(&self, schema: &CompiledSchema, type_name: &str) -> Option<DecimalShape> {
        self.shape_with_fallback(schema, type_name, None)
    }

    /// [`shape`](Self::shape) for an abstract `type_name` whose documents may name no
    /// `__typename`: each such document is read as `fallback`.
    #[must_use]
    pub fn shape_with_fallback(
        &self,
        schema: &CompiledSchema,
        type_name: &str,
        fallback: Option<&str>,
    ) -> Option<DecimalShape> {
        let root = if schema.find_type(type_name).is_some() {
            DecimalNode::Object(type_name.to_string())
        } else {
            let members = abstract_members(schema, type_name)?;
            if !members.iter().any(|m| self.types.contains_key(m.as_str())) {
                return None;
            }
            DecimalNode::Abstract(fallback.map(str::to_string))
        };
        if let DecimalNode::Object(name) = &root {
            self.types.get(name)?;
        }
        Some(DecimalShape::new(root, Arc::clone(&self.types)))
    }
}

/// The concrete types a union or an interface may hold; `None` when `name` is neither.
fn abstract_members(schema: &CompiledSchema, name: &str) -> Option<Vec<String>> {
    if let Some(union) = schema.find_union(name) {
        return Some(union.member_types.clone());
    }
    schema.find_interface(name)?;
    Some(schema.find_implementors(name).iter().map(|t| t.name.to_string()).collect())
}

/// Every object type from which a `Decimal` is reachable, through any depth of fields.
fn reaching_types(schema: &CompiledSchema) -> HashSet<String> {
    let mut reaching = HashSet::new();
    loop {
        let before = reaching.len();
        for t in &schema.types {
            if !reaching.contains(t.name.as_str())
                && t.fields.iter().any(|f| node_for(schema, &reaching, &f.field_type).is_some())
            {
                reaching.insert(t.name.to_string());
            }
        }
        if reaching.len() == before {
            return reaching;
        }
    }
}

/// Where a value of `field_type` leads, given the types known to reach a `Decimal`.
fn node_for(
    schema: &CompiledSchema,
    reaching: &HashSet<String>,
    field_type: &FieldType,
) -> Option<DecimalNode> {
    match field_type {
        FieldType::Decimal => Some(DecimalNode::Decimal),
        FieldType::List { item, .. } => node_for(schema, reaching, item),
        FieldType::Object(name) => {
            reaching.contains(name.as_str()).then(|| DecimalNode::Object(name.clone()))
        },
        FieldType::Union(name) | FieldType::Interface(name) => abstract_members(schema, name)?
            .iter()
            .any(|m| reaching.contains(m.as_str()))
            .then_some(DecimalNode::Abstract(None)),
        _ => None,
    }
}
