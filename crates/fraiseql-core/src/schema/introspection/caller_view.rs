//! `__schema` and `__type` as one caller may see them.
//!
//! The role gate refuses a `requires_role` operation as *"not found in schema"*, so that
//! a refusal cannot be used to enumerate operations. Introspection must say the same
//! thing: an operation is listed only to a caller who could run it, a type with a role
//! only to a caller who holds it, and a type is dropped with an operation when nothing
//! the caller can still see reaches that type.
//!
//! What a caller sees depends only on which of the schema's gating roles it holds, so a
//! view is built once per such set and shared. A schema with no `requires_role`
//! operation has one view, and serves it without taking a lock.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use parking_lot::RwLock;
use serde_json::Value;

use super::schema_builder::IntrospectionResponses;
use crate::{
    schema::CompiledSchema,
    security::{
        SecurityContext,
        role_gate::{caller_holds, visible_schema},
    },
};

/// The introspection responses of a schema, answered per caller.
pub struct CallerIntrospection {
    /// Every operation listed: what a caller holding every gating role sees.
    full:           Arc<IntrospectionResponses>,
    /// Every role a type, query or mutation of the schema requires.
    gating_roles:   BTreeSet<String>,
    /// The type names `full` reaches from its roots.
    full_reachable: HashSet<String>,
    /// Builds responses from a schema; the executor's, so a view is filtered exactly as
    /// `full` is.
    build:          fn(&CompiledSchema) -> IntrospectionResponses,
    /// Views built so far, by [`view_key`].
    views:          RwLock<HashMap<String, Arc<IntrospectionResponses>>>,
}

impl CallerIntrospection {
    /// Build the full responses of `schema` with `build`.
    pub fn new(
        schema: &CompiledSchema,
        build: fn(&CompiledSchema) -> IntrospectionResponses,
    ) -> Self {
        let full = build(schema);
        let full_reachable = reachable_types(schema_of(&full));
        let gating_roles = schema
            .queries
            .iter()
            .filter_map(|q| q.requires_role.clone())
            .chain(schema.mutations.iter().filter_map(|m| m.requires_role.clone()))
            .chain(schema.types.iter().filter_map(|t| t.requires_role.clone()))
            .collect();
        Self {
            full: Arc::new(full),
            gating_roles,
            full_reachable,
            build,
            views: RwLock::new(HashMap::new()),
        }
    }

    /// The responses `caller` may see, and a key that is equal for two callers exactly
    /// when their responses are.
    ///
    /// `schema` must be the schema this was built from.
    pub fn for_caller(
        &self,
        schema: &CompiledSchema,
        caller: Option<&SecurityContext>,
    ) -> (String, Arc<IntrospectionResponses>) {
        let held: Vec<&str> = self
            .gating_roles
            .iter()
            .map(String::as_str)
            .filter(|role| caller_holds(Some(role), caller))
            .collect();
        if held.len() == self.gating_roles.len() {
            return (String::new(), Arc::clone(&self.full));
        }
        let key = view_key(&held);
        if let Some(view) = self.views.read().get(&key) {
            return (key, Arc::clone(view));
        }
        let view = Arc::new(self.build_view(schema, caller));
        let view = Arc::clone(self.views.write().entry(key.clone()).or_insert(view));
        (key, view)
    }

    /// The responses of the schema `caller` may see, without the types only what it may not
    /// see reached.
    fn build_view(
        &self,
        schema: &CompiledSchema,
        caller: Option<&SecurityContext>,
    ) -> IntrospectionResponses {
        let mut view = (self.build)(&visible_schema(schema, caller));

        let still_reached = reachable_types(schema_of(&view));
        let dropped: HashSet<&str> = self
            .full_reachable
            .iter()
            .map(String::as_str)
            .filter(|name| !still_reached.contains(*name))
            .collect();
        if dropped.is_empty() {
            return view;
        }
        view.type_responses.retain(|name, _| !dropped.contains(name.as_str()));
        let mut schema_response = view.schema_response.as_ref().clone();
        if let Some(types) = schema_response
            .pointer_mut("/data/__schema/types")
            .and_then(Value::as_array_mut)
        {
            types.retain(|t| t["name"].as_str().is_none_or(|name| !dropped.contains(name)));
        }
        view.schema_response = Arc::new(schema_response);
        view
    }
}

/// The key of the view a caller holding `held` (in sorted order) sees. Never empty, so
/// it cannot collide with the full view's.
fn view_key(held: &[&str]) -> String {
    format!("\u{1}{}", held.join("\u{1f}"))
}

/// The `__schema` object of a response.
fn schema_of(responses: &IntrospectionResponses) -> &Value {
    &responses.schema_response["data"]["__schema"]
}

/// The names of the types a client can reach from `schema`'s root operation types and
/// its directives' arguments, by fields, arguments, input fields, interfaces and
/// possible types.
fn reachable_types(schema: &Value) -> HashSet<String> {
    let by_name: HashMap<&str, &Value> = schema["types"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["name"].as_str().map(|name| (name, t)))
        .collect();

    let mut pending: Vec<&str> = ["queryType", "mutationType", "subscriptionType"]
        .iter()
        .filter_map(|root| schema[root]["name"].as_str())
        .collect();
    for directive in schema["directives"].as_array().into_iter().flatten() {
        pending.extend(input_types(&directive["args"]));
    }

    let mut reached = HashSet::new();
    while let Some(name) = pending.pop() {
        if !reached.insert(name.to_string()) {
            continue;
        }
        let Some(t) = by_name.get(name) else { continue };
        for field in t["fields"].as_array().into_iter().flatten() {
            pending.extend(named_type(&field["type"]));
            pending.extend(input_types(&field["args"]));
        }
        pending.extend(input_types(&t["inputFields"]));
        for list in ["interfaces", "possibleTypes"] {
            pending.extend(
                t[list].as_array().into_iter().flatten().filter_map(|r| r["name"].as_str()),
            );
        }
    }
    reached
}

/// The named types of a list of `__InputValue`s.
fn input_types(values: &Value) -> impl Iterator<Item = &str> {
    values.as_array().into_iter().flatten().filter_map(|v| named_type(&v["type"]))
}

/// The named type under any `NON_NULL` and `LIST` wrappers.
fn named_type(type_ref: &Value) -> Option<&str> {
    let mut current = type_ref;
    loop {
        if let Some(name) = current["name"].as_str() {
            return Some(name);
        }
        current = current.get("ofType")?;
    }
}
