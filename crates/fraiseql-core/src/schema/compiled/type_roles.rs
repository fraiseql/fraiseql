//! Type-level `requires_role`: propagation onto the operations that return a type,
//! and rejection of the shapes propagation cannot cover.
//!
//! # What was broken (#677)
//!
//! [`TypeDefinition::requires_role`] is documented as an access gate —
//! *"JWT role required to access any field on this type"* (`docs/authoring.md`) and
//! *"Role required to see this type in introspection and access queries returning
//! it"* — and it gated nothing. Only two non-test reads of it existed, neither an
//! execution gate: the bespoke REST `/introspection` route's filter, and a reporting
//! entry in the metadata endpoint. Every genuine enforcement site
//! (`query_regular`, `query_relay`, `mutation`, federation `_entities`, the REST
//! handler) reads the **operation**-level role instead, and nothing ever seeded an
//! operation's role from the type it returns.
//!
//! The repository's own golden fixture demonstrates the hole: in
//! `05-security-inject-cache.json` the type `Order` carries `requires_role: "admin"`,
//! the query `orders` carries it too — and the query `orderSummary`, which also
//! returns `Order`, carries none. A principal without `admin` could read every field
//! of an `admin`-gated type through `orderSummary`.
//!
//! # The shape of the fix
//!
//! Rather than add a sixth enforcement site — the recurring bug in this codebase is
//! one path having a check and its sibling not — the type's role is **lowered onto
//! the operations that return it**, so the five existing operation-level gates all
//! enforce it with no new code. Enforcement stays in one place; only the input
//! changes.
//!
//! Two shapes propagation cannot express, both rejected rather than half-enforced:
//!
//! * **A conflicting pair.** A query declaring `requires_role = "manager"` that returns a type
//!   declaring `"admin"` means *both* are required, and `Option<String>` cannot carry a
//!   conjunction. Silently keeping one would grant access the author did not intend.
//! * **Nested reachability.** A gated type reachable as a field of a type that is not gated the
//!   same way is readable through the containing type's own operations, which propagation never
//!   touches. The compiled schema is rejected with both type names, because the alternative is a
//!   documented control that holds for top-level selections and quietly does not for nested ones.

use std::collections::BTreeMap;

use super::CompiledSchema;

impl CompiledSchema {
    /// Copy each type's `requires_role` onto the operations that return it.
    ///
    /// Only fills an operation whose own role is `None`; an operation that declares
    /// a role keeps it, and any disagreement is reported by
    /// [`Self::type_role_violations`] rather than silently resolved here.
    ///
    /// Subscriptions are deliberately absent: `SubscriptionDefinition` carries no
    /// `requires_role` and no subscription path consults one, so there is nothing to
    /// propagate onto. `type_role_violations` rejects that combination instead.
    ///
    /// Idempotent: running it twice assigns the same values.
    pub(crate) fn propagate_type_roles(&mut self) {
        let roles: BTreeMap<String, String> = self
            .types
            .iter()
            .filter_map(|t| t.requires_role.as_ref().map(|r| (t.name.to_string(), r.clone())))
            .collect();
        if roles.is_empty() {
            return;
        }

        for query in &mut self.queries {
            if query.requires_role.is_none() {
                if let Some(role) = roles.get(query.return_type.as_str()) {
                    query.requires_role = Some(role.clone());
                }
            }
        }
        for mutation in &mut self.mutations {
            if mutation.requires_role.is_none() {
                if let Some(role) = roles.get(mutation.return_type.as_str()) {
                    mutation.requires_role = Some(role.clone());
                }
            }
        }
    }

    /// Declarations a role-gated type makes that the runtime cannot honour.
    ///
    /// Returns one human-readable message per violation, empty when the schema is
    /// enforceable. Called by [`CompiledSchema::from_json`] so a schema that would
    /// enforce its own documentation only partially never loads.
    #[must_use]
    pub fn type_role_violations(&self) -> Vec<String> {
        let roles: BTreeMap<&str, &str> = self
            .types
            .iter()
            .filter_map(|t| t.requires_role.as_deref().map(|r| (t.name.as_str(), r)))
            .collect();
        if roles.is_empty() {
            return Vec::new();
        }

        let mut violations = Vec::new();

        // 1. An operation whose own role disagrees with its return type's.
        let mut check_operation = |kind: &str, name: &str, ret: &str, own: Option<&str>| {
            if let (Some(own), Some(type_role)) = (own, roles.get(ret)) {
                if own != *type_role {
                    violations.push(format!(
                        "{kind} '{name}' requires role '{own}' but returns type '{ret}', which \
                         requires '{type_role}'. Both are required and a compiled operation \
                         carries only one role — give them the same role, or drop one."
                    ));
                }
            }
        };
        for q in &self.queries {
            check_operation("query", &q.name, q.return_type.as_str(), q.requires_role.as_deref());
        }
        for m in &self.mutations {
            check_operation(
                "mutation",
                &m.name,
                m.return_type.as_str(),
                m.requires_role.as_deref(),
            );
        }
        // Subscriptions carry no `requires_role` of their own — `SubscriptionDefinition`
        // has no such field and no subscription path consults one — so a role-gated
        // type cannot be gated when it is streamed. Rejected rather than streamed
        // ungated under a gated name.
        for s in &self.subscriptions {
            if let Some(type_role) = roles.get(s.return_type.as_str()) {
                violations.push(format!(
                    "subscription '{}' returns type '{}', which requires role \
                     '{type_role}', but subscriptions carry no role gate — the type \
                     would stream to any subscriber. Remove `requires_role` from '{}' \
                     or drop the subscription.",
                    s.name, s.return_type, s.return_type
                ));
            }
        }

        // 2. A gated type reachable as a field of a type that is not gated the same way. Operations
        //    returning the *container* carry the container's role (or none), so the gated type
        //    travels out inside their response.
        for container in &self.types {
            let container_role = roles.get(container.name.as_str()).copied();
            for field in &container.fields {
                let Some(named) = named_type(&field.field_type) else {
                    continue;
                };
                let Some(nested_role) = roles.get(named) else {
                    continue;
                };
                if container_role == Some(*nested_role) {
                    continue;
                }
                let held =
                    container_role.map_or_else(|| "no role".to_string(), |r| format!("role '{r}'"));
                violations.push(format!(
                    "type '{}' field '{}' exposes type '{named}', which requires role \
                     '{nested_role}', but '{}' itself requires {held}. A role-gated type \
                     reachable through an operation that does not carry its role is not \
                     gated at all — put the same role on '{}', or remove it from '{named}'.",
                    container.name, field.name, container.name, container.name
                ));
            }
        }

        violations
    }
}

/// The named type a field refers to, unwrapping list nesting.
///
/// Only object/interface/union references matter: an enum or a scalar has no fields
/// and cannot carry `requires_role`.
fn named_type(ty: &crate::schema::FieldType) -> Option<&str> {
    match ty {
        crate::schema::FieldType::List(inner) => named_type(inner),
        crate::schema::FieldType::Object(name)
        | crate::schema::FieldType::Interface(name)
        | crate::schema::FieldType::Union(name) => Some(name),
        _ => None,
    }
}
