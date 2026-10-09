//! Non-null completion of a response — GraphQL § 6.4.4 (*Handling Execution Errors*, #1522).
//!
//! A field the schema publishes as non-null (`String!`, `[Edge!]!`) must never be answered
//! with `null`. When the stored value is missing or `null`, § 6.4.4 makes it a **field
//! error**: the error is added to `errors` with the field's response path, and the `null`
//! propagates to the nearest ancestor position that allows it (a nullable field or list
//! item), up to `data` itself when every position above is non-null.
//!
//! Before this nothing checked it: a row with no `name` came back `{"name": null}` under a
//! `String!`, and a typed client generated from the schema crashed on a value it was told
//! could not occur.
//!
//! # The types are the ones introspection publishes
//!
//! The rule is applied against [`IntrospectionBuilder::build`], the same model `__schema`
//! and `__type` answer from, so what the runtime enforces is exactly what it advertises:
//! the relay `Connection`/`Edge` wrappers, the translations siblings of localized fields
//! and the root operation types included. A type or field that model does not carry is
//! passed through unadjudicated, the governing rule of the validators next door.
//!
//! # One pass, after execution
//!
//! Completion runs once over a root field's finished value, guided by the operation's
//! selection set (aliases are response keys; inline fragments apply when their type
//! condition matches the object's `__typename`; `@skip`/`@include` are evaluated against the
//! request's variables), rather than inside each projector. Every read path — regular,
//! relay, composed, node, federation `_entities`, aggregates, mutation payloads and
//! subscription events — produces a value of the published type, so one walk over that
//! value is the one place the rule can be applied to all of them alike.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::{
    graphql::{DirectiveEvaluator, FieldSelection},
    schema::{CompiledSchema, IntrospectionBuilder, IntrospectionType, TypeKind},
};

/// How deep the walk follows a response before passing the rest through. A response is
/// depth-limited upstream (GATE 1); this bounds a hand-built one.
const MAX_COMPLETION_DEPTH: usize = 64;

/// An output type reference, wrappers included: `[Edge!]!` is
/// `NonNull(List(NonNull(Named("Edge"))))`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeRef {
    NonNull(Box<TypeRef>),
    List(Box<TypeRef>),
    Named(String),
}

impl TypeRef {
    fn from_introspection(t: &IntrospectionType) -> Option<Self> {
        match t.kind {
            TypeKind::NonNull => {
                Some(Self::NonNull(Box::new(Self::from_introspection(t.of_type.as_deref()?)?)))
            },
            TypeKind::List => {
                Some(Self::List(Box::new(Self::from_introspection(t.of_type.as_deref()?)?)))
            },
            _ => t.name.clone().map(Self::Named),
        }
    }
}

/// The output types of a schema, as introspection publishes them: every object and
/// interface's fields with their wrapped types, and every abstract type's possible types.
#[derive(Debug, Clone, Default)]
pub struct OutputTypes {
    /// Object and interface type name → field name → published type.
    fields:   HashMap<String, HashMap<String, TypeRef>>,
    /// Interface and union type name → the object types it may resolve to.
    possible: HashMap<String, HashSet<String>>,
}

impl OutputTypes {
    /// The output types `schema` publishes through introspection.
    #[must_use]
    pub fn from_schema(schema: &CompiledSchema) -> Self {
        let introspection = IntrospectionBuilder::build(schema);
        let mut types = Self::default();
        for t in &introspection.types {
            let Some(name) = &t.name else {
                continue;
            };
            if let Some(fields) = &t.fields {
                let fields = fields
                    .iter()
                    .filter_map(|f| {
                        TypeRef::from_introspection(&f.field_type).map(|r| (f.name.clone(), r))
                    })
                    .collect();
                types.fields.insert(name.clone(), fields);
            }
            if let Some(possible) = &t.possible_types {
                types
                    .possible
                    .insert(name.clone(), possible.iter().map(|p| p.name.clone()).collect());
            }
        }
        types.add_federation(schema);
        types
    }

    /// The federation entry point the subgraph SDL publishes (`_entities(representations:
    /// [_Any!]!): [_Entity]!`, `union _Entity` of the entity types), which introspection
    /// does not carry.
    #[cfg(feature = "federation")]
    fn add_federation(&mut self, schema: &CompiledSchema) {
        let Some(metadata) = schema.federation_metadata() else {
            return;
        };
        let entity = TypeRef::Named("_Entity".to_string());
        self.fields.entry("Query".to_string()).or_default().insert(
            "_entities".to_string(),
            TypeRef::NonNull(Box::new(TypeRef::List(Box::new(entity)))),
        );
        self.possible
            .insert("_Entity".to_string(), metadata.types.iter().map(|t| t.name.clone()).collect());
    }

    #[cfg(not(feature = "federation"))]
    #[allow(clippy::unused_self)] // Reason: the federation arm reads and writes `self`
    const fn add_federation(&mut self, _schema: &CompiledSchema) {}

    fn field(&self, type_name: &str, field: &str) -> Option<&TypeRef> {
        self.fields.get(type_name)?.get(field)
    }

    /// Complete `response["data"]` (§ 6.4.4): each root field of `selections` on
    /// `root_type`, in place. A violation nulls the nearest nullable position, or `data`,
    /// and adds an entry to `response["errors"]`.
    ///
    /// `selections` is the operation's root selection set with fragment spreads expanded
    /// and directives not yet evaluated; `variables` evaluates them.
    pub fn complete(
        &self,
        response: &mut Value,
        root_type: &str,
        selections: &[FieldSelection],
        variables: &HashMap<String, Value>,
    ) {
        let walk = Walk {
            types: self,
            variables,
        };
        let mut errors = Vec::new();
        let Some(data) = response.get_mut("data").and_then(Value::as_object_mut) else {
            return;
        };
        let mut null_data = false;
        let mut path = Vec::new();
        for sel in walk.flatten(selections, root_type) {
            let Some(field_ref) = self.field(root_type, &sel.name) else {
                continue;
            };
            let key = sel.response_key();
            let Some(value) = data.get_mut(key) else {
                continue;
            };
            path.push(Value::String(key.to_string()));
            let label = format!("{root_type}.{}", sel.name);
            match walk.position(value.take(), field_ref, sel, &label, &mut path, &mut errors, 0) {
                Ok(v) => *value = v,
                Err(Propagate) => null_data = true,
            }
            path.pop();
        }
        if null_data {
            response["data"] = Value::Null;
        }
        if errors.is_empty() {
            return;
        }
        match response.get_mut("errors").and_then(Value::as_array_mut) {
            Some(existing) => existing.extend(errors),
            None => {
                if let Some(object) = response.as_object_mut() {
                    object.insert("errors".to_string(), Value::Array(errors));
                }
            },
        }
    }
}

impl OutputTypes {
    /// Complete one value of `type_name` at a nullable position (§ 6.4.4), against the
    /// sub-selection `selections` (spreads expanded, directives evaluated): the value, or
    /// `null` when a violation propagated to it, and the errors, their paths rooted at
    /// `root_key`. A subscription event is one: every subscription field is nullable.
    #[must_use]
    pub fn complete_value(
        &self,
        value: Value,
        type_name: &str,
        selections: &[FieldSelection],
        root_key: &str,
    ) -> (Value, Vec<Value>) {
        let variables = HashMap::new();
        let walk = Walk {
            types:     self,
            variables: &variables,
        };
        let sel = FieldSelection {
            name:          root_key.to_string(),
            alias:         None,
            arguments:     Vec::new(),
            nested_fields: selections.to_vec(),
            directives:    Vec::new(),
        };
        let mut errors = Vec::new();
        let mut path = vec![Value::String(root_key.to_string())];
        let field_ref = TypeRef::Named(type_name.to_string());
        let value = walk
            .position(value, &field_ref, &sel, root_key, &mut path, &mut errors, 0)
            .unwrap_or(Value::Null);
        (value, errors)
    }
}

/// A non-null violation below, to be absorbed by the nearest nullable position.
struct Propagate;

/// One completion: the types and the request's variables.
struct Walk<'a> {
    types:     &'a OutputTypes,
    variables: &'a HashMap<String, Value>,
}

impl Walk<'_> {
    /// The fields `selections` selects on an object of type `type_name`: inline fragments
    /// whose condition the type satisfies are lifted, fields excluded by `@skip`/`@include`
    /// and meta-fields are dropped.
    fn flatten<'s>(
        &self,
        selections: &'s [FieldSelection],
        type_name: &str,
    ) -> Vec<&'s FieldSelection> {
        let mut out = Vec::new();
        for sel in selections {
            // A directive that cannot be evaluated was refused before execution; included is
            // the reading that adjudicates the most.
            if !DirectiveEvaluator::evaluate_directives(sel, self.variables).unwrap_or(true) {
                continue;
            }
            if let Some(condition) = sel.name.strip_prefix("...on ") {
                if self.satisfies(type_name, condition.trim()) {
                    out.extend(self.flatten(&sel.nested_fields, type_name));
                }
                continue;
            }
            if sel.name.starts_with("...") || sel.name.starts_with("__") {
                continue;
            }
            out.push(sel);
        }
        out
    }

    /// Whether an object of type `type_name` matches a fragment on `condition`.
    fn satisfies(&self, type_name: &str, condition: &str) -> bool {
        condition == type_name
            || self.types.possible.get(condition).is_some_and(|p| p.contains(type_name))
    }

    /// Complete `value` at a position of type `field_ref`.
    ///
    /// `Err(Propagate)` when the position is non-null and its value is `null`, whether it
    /// arrived so (an error is recorded here, at `path`) or became so by a violation below
    /// (already recorded there).
    #[allow(clippy::too_many_arguments)] // Reason: one recursive step of one walk; a struct would hold the same state
    fn position(
        &self,
        value: Value,
        field_ref: &TypeRef,
        sel: &FieldSelection,
        label: &str,
        path: &mut Vec<Value>,
        errors: &mut Vec<Value>,
        depth: usize,
    ) -> Result<Value, Propagate> {
        match field_ref {
            TypeRef::NonNull(inner) => {
                match self.nullable(value, inner, sel, label, path, errors, depth) {
                    None => Err(Propagate),
                    Some(Value::Null) => {
                        errors.push(serde_json::json!({
                            "message": format!("Cannot return null for non-nullable field {label}."),
                            "path": path.clone(),
                        }));
                        Err(Propagate)
                    },
                    Some(v) => Ok(v),
                }
            },
            other => Ok(self
                .nullable(value, other, sel, label, path, errors, depth)
                .unwrap_or(Value::Null)),
        }
    }

    /// Complete a value at a nullable position: `None` when a non-null violation below
    /// made it `null`.
    #[allow(clippy::too_many_arguments)] // Reason: as `position`
    fn nullable(
        &self,
        value: Value,
        field_ref: &TypeRef,
        sel: &FieldSelection,
        label: &str,
        path: &mut Vec<Value>,
        errors: &mut Vec<Value>,
        depth: usize,
    ) -> Option<Value> {
        if value.is_null() || depth > MAX_COMPLETION_DEPTH {
            return Some(value);
        }
        match field_ref {
            TypeRef::NonNull(_) => {
                self.position(value, field_ref, sel, label, path, errors, depth).ok()
            },
            TypeRef::List(inner) => {
                let Value::Array(items) = value else {
                    return Some(value);
                };
                // Every item is completed, so every violation is reported, before one
                // that propagated nulls the list.
                let mut out = Vec::with_capacity(items.len());
                let mut propagated = false;
                for (index, item) in items.into_iter().enumerate() {
                    path.push(Value::from(index));
                    let item = self.position(item, inner, sel, label, path, errors, depth + 1);
                    path.pop();
                    match item {
                        Ok(v) => out.push(v),
                        Err(Propagate) => propagated = true,
                    }
                }
                (!propagated).then_some(Value::Array(out))
            },
            TypeRef::Named(name) => match value {
                Value::Object(map) => self.object(map, name, sel, path, errors, depth),
                leaf => Some(leaf),
            },
        }
    }

    /// Complete an object value of declared type `declared` against `sel`'s sub-selection.
    fn object(
        &self,
        mut map: Map<String, Value>,
        declared: &str,
        sel: &FieldSelection,
        path: &mut Vec<Value>,
        errors: &mut Vec<Value>,
        depth: usize,
    ) -> Option<Value> {
        // The object is of the type its `__typename` names, when that is a type the schema
        // publishes: an abstract type's variant, or the error type a failed mutation
        // resolves to under a bare success return type. Without one, the declared type's
        // own fields (an interface's) are all that can be adjudicated.
        let concrete = map
            .get("__typename")
            .and_then(Value::as_str)
            .filter(|t| self.types.fields.contains_key(*t))
            .unwrap_or(declared)
            .to_string();
        if !self.types.fields.contains_key(&concrete) {
            return Some(Value::Object(map));
        }
        // Every field is completed, so every violation is reported, before one that
        // propagated nulls the object.
        let mut propagated = false;
        for child in self.flatten(&sel.nested_fields, &concrete) {
            let Some(field_ref) = self.types.field(&concrete, &child.name) else {
                continue;
            };
            let key = child.response_key();
            let present = map.contains_key(key);
            let value = map.remove(key).unwrap_or(Value::Null);
            path.push(Value::String(key.to_string()));
            let label = format!("{concrete}.{}", child.name);
            let completed = self.position(value, field_ref, child, &label, path, errors, depth + 1);
            path.pop();
            match completed {
                Ok(v) if present => {
                    map.insert(key.to_string(), v);
                },
                Ok(_) => {},
                Err(Propagate) => propagated = true,
            }
        }
        (!propagated).then_some(Value::Object(map))
    }
}

#[cfg(test)]
mod tests;
