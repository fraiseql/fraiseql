//! Which views a mutation can be *proven*, from the schema alone, to invalidate.
//!
//! [`plan_invalidation`] resolves a successful mutation's views from four
//! declarations. Two of them are static — `invalidates_views`, and the views behind
//! the return type and the entity a payload type wraps — and two are only knowable at
//! runtime: the `entity_type` a SQL function stamps on `mutation_response`, and the
//! cascade envelope it returns.
//!
//! When the two static sources resolve to nothing, the mutation is **unattributable**:
//! the engine cannot know what it wrote, and if the function also stamps no
//! `entity_type` the plan is empty and the mutation invalidates nothing at all —
//! silently, forever, for the `cache_ttl_seconds = 0` entries documented as
//! "mutation-invalidated only" (#910).
//!
//! The compiler refuses that shape rather than letting it run, so the split lives
//! here: the compile gate and the runtime plan read the same function and cannot
//! drift into disagreeing about what "resolves to a view" means.
//!
//! [`plan_invalidation`]: crate::runtime

use fraiseql_db::ViewName;

use crate::schema::{CompiledSchema, MutationDefinition};

/// The view a type reads from, if it declares a non-empty one.
#[must_use]
pub fn view_of_type(schema: &CompiledSchema, type_name: &str) -> Option<ViewName> {
    schema
        .types
        .iter()
        .find(|t| t.name == type_name)
        .filter(|t| !t.sql_source.as_str().is_empty())
        .map(|t| ViewName::from(t.sql_source.as_str()))
}

/// The entity type a payload type wraps, read from its `entity` field.
///
/// `CreateUserSuccess { entity: User }` → `Some("User")`.
#[must_use]
pub fn payload_entity_type(payload_type: &str, schema: &CompiledSchema) -> Option<String> {
    schema
        .find_type(payload_type)?
        .fields
        .iter()
        .find(|f| f.name.as_str() == "entity")?
        .field_type
        .type_name()
        .map(std::string::ToString::to_string)
}

/// The views a mutation invalidates that are knowable without running it.
///
/// De-duplicated, in declaration order: `invalidates_views` first, then the return
/// type's view, then the view of the entity a payload return type wraps.
#[must_use]
pub fn statically_resolved_views(
    mutation: &MutationDefinition,
    schema: &CompiledSchema,
) -> Vec<ViewName> {
    let mut views: Vec<ViewName> = Vec::new();
    let push = |view: ViewName, views: &mut Vec<ViewName>| {
        if !views.iter().any(|v| v.as_str() == view.as_str()) {
            views.push(view);
        }
    };

    for declared in &mutation.invalidates_views {
        push(ViewName::from(declared.as_str()), &mut views);
    }
    if let Some(view) = view_of_type(schema, &mutation.return_type) {
        push(view, &mut views);
    }
    if let Some(inner) = payload_entity_type(&mutation.return_type, schema) {
        if let Some(view) = view_of_type(schema, &inner) {
            push(view, &mut views);
        }
    }
    views
}

/// Whether the schema declares at least one cacheable view.
///
/// A `cache_ttl_seconds` annotation is what puts a view in the result cache at all
/// (the adapter runs in opt-in mode), so a schema with none has no entry an
/// unattributable mutation could strand.
#[must_use]
pub fn declares_cacheable_views(schema: &CompiledSchema) -> bool {
    schema
        .queries
        .iter()
        .any(|q| q.cache_ttl_seconds.is_some() && q.sql_source.is_some())
}

/// Every mutation whose invalidation cannot be resolved from the schema.
///
/// Names only, in schema order — the caller decides whether that is a compile
/// error, and needs the list to tell the author which mutations to annotate.
#[must_use]
pub fn unattributable_mutations(schema: &CompiledSchema) -> Vec<&str> {
    schema
        .mutations
        .iter()
        .filter(|m| statically_resolved_views(m, schema).is_empty())
        .map(|m| m.name.as_str())
        .collect()
}

#[cfg(test)]
#[path = "mutation_reach_tests.rs"]
mod mutation_reach_tests;
