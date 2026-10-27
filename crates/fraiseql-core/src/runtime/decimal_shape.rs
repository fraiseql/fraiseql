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
    db::traits::{DecimalFields, DecimalNode, DecimalShape, WriteDecimals},
    runtime::projection::stored_key_candidates,
    schema::{CompiledSchema, FieldType},
};

#[cfg(test)]
mod tests;

/// Every type in a schema from which a `Decimal` is reachable, and the shape of a document
/// read as each, built once per schema.
#[derive(Debug, Clone, Default)]
pub struct DecimalIndex {
    /// The shape of a document read as each object type, union or interface.
    shapes: HashMap<String, Arc<DecimalShape>>,
    /// The shape of the documents each query reads.
    reads:  HashMap<String, Arc<DecimalShape>>,
    /// The fields of every type from which a `Decimal` is reachable.
    types:  Arc<HashMap<String, DecimalFields>>,
}

impl DecimalIndex {
    /// Index `schema`: each type that holds a `Decimal`, or a field leading to a type that
    /// does, with every key a document may hold such a field by.
    #[must_use]
    pub fn new(schema: &CompiledSchema) -> Self {
        let reaching = reaching_types(schema);
        let types: HashMap<String, DecimalFields> = schema
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
        let types = Arc::new(types);
        let abstracts = schema
            .unions
            .iter()
            .map(|u| u.name.as_str())
            .chain(schema.interfaces.iter().map(|i| i.name.as_str()));
        let shapes = types
            .keys()
            .map(|name| (name.clone(), DecimalNode::Object(name.clone())))
            .chain(
                abstracts
                    .filter(|name| reaches_abstract(schema, &reaching, name))
                    .map(|name| (name.to_string(), DecimalNode::Abstract(None))),
            )
            .map(|(name, root)| (name, Arc::new(DecimalShape::new(root, Arc::clone(&types)))))
            .collect::<HashMap<_, _>>();
        let reads = schema
            .queries
            .iter()
            .filter_map(|query| {
                let read_as = query.return_type.as_str();
                let shape = shapes.get(read_as)?;
                let stored = schema.possible_types(read_as).and_then(|possible| {
                    schema.type_stored_in(&possible, query.sql_source.as_deref())
                });
                let shape = match stored {
                    Some(fallback) => Arc::new(DecimalShape::new(
                        DecimalNode::Abstract(Some(fallback.to_string())),
                        Arc::clone(&types),
                    )),
                    None => Arc::clone(shape),
                };
                Some((query.name.clone(), shape))
            })
            .collect();
        Self {
            shapes,
            reads,
            types,
        }
    }

    /// The shape of a document read as `type_name`, an object type, a union or an
    /// interface; `None` when no `Decimal` is reachable from it.
    #[must_use]
    pub fn shape(&self, type_name: &str) -> Option<&Arc<DecimalShape>> {
        self.shapes.get(type_name)
    }

    /// The shape of the documents the query `query_name` reads: its return type's, an
    /// abstract one reading a document that names no `__typename` as the one possible type
    /// stored in the query's view, as the read itself types such a row.
    #[must_use]
    pub fn read_by(&self, query_name: &str) -> Option<&Arc<DecimalShape>> {
        self.reads.get(query_name)
    }

    /// A mutation response's documents, each read as the type its row stamps; `None` when
    /// the schema holds no `Decimal`.
    #[must_use]
    pub fn write(&self) -> Option<WriteDecimals> {
        (!self.types.is_empty()).then(|| WriteDecimals::new(Arc::clone(&self.types)))
    }
}

/// Whether `text` is a `Decimal` as one travels: optionally signed digits, with a fractional
/// part after a point when it has one. No exponent, no leading point, no padding.
#[must_use]
pub fn is_decimal_digits(text: &str) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) =
        unsigned.split_once('.').map_or((unsigned, None), |(w, f)| (w, Some(f)));
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    digits(whole) && fraction.is_none_or(digits)
}

/// Whether a union or an interface named `name` holds a type from which a `Decimal` is
/// reachable.
fn reaches_abstract(schema: &CompiledSchema, reaching: &HashSet<String>, name: &str) -> bool {
    schema
        .possible_types(name)
        .is_some_and(|members| members.iter().any(|m| reaching.contains(*m)))
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
        FieldType::Union(name) | FieldType::Interface(name) => {
            reaches_abstract(schema, reaching, name).then_some(DecimalNode::Abstract(None))
        },
        _ => None,
    }
}
