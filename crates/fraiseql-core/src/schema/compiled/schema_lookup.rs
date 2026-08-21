//! Lookup and indexing methods for [`CompiledSchema`].
//!
//! All `find_*` methods, `build_indexes`, `display_name`, and `operation_count`.

use super::{
    directive::DirectiveDefinition, mutation::MutationDefinition, query::QueryDefinition,
    schema::CompiledSchema,
};
use crate::schema::{
    config_types::NamingConvention,
    graphql_type_defs::{
        EnumDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition, UnionDefinition,
    },
    subscription_types::SubscriptionDefinition,
};

impl CompiledSchema {
    /// Build the schema's derived state: O(1) operation lookup indexes, and the
    /// filter/sort input surface `where`/`orderBy` are typed against.
    ///
    /// Called automatically by `from_json()`. Must be called manually after any
    /// direct mutation of `self.queries`, `self.mutations`, or `self.subscriptions`.
    ///
    /// # The derived filter surface
    ///
    /// [`derived_inputs::derive`] emits `{Entity}WhereInput`,
    /// `{Entity}OrderByInput` and their leaf filters; they are appended to
    /// `input_types`/`enums` so introspection, the federation SDL, § 5.8.2 and
    /// the client emitters all read one materialised surface rather than four
    /// re-derivations of it. Nothing is overwritten — a name the author declared
    /// is never derived over — which is also what makes a second call inert.
    ///
    /// They are deliberately *not* part of the compiled artifact. The `where`
    /// and `orderBy` arguments themselves are synthesized at read time from
    /// [`auto_params`](crate::schema::QueryDefinition::auto_params) rather than
    /// stored; their types follow the same rule, so a schema compiled by an
    /// older CLI gains the surface on load and the artifact's content hash does
    /// not move.
    ///
    /// A bare `serde_json::from_str::<CompiledSchema>` skips this, as it always
    /// has for the operation indexes. The consequence is a *degraded* surface,
    /// never a wrong one: `graphql_arguments` types an argument only when the
    /// schema carries the type, so a schema that was never built falls back to
    /// `JSON` rather than naming something undefined. Load through
    /// [`from_json`](Self::from_json) to get the typed surface.
    ///
    /// [`derived_inputs::derive`]: crate::schema::derived_inputs::derive
    pub fn build_indexes(&mut self) {
        let derived = crate::schema::derived_inputs::derive(self);
        self.input_types.extend(derived.input_types);
        self.enums.extend(derived.enums);

        let camel = matches!(self.naming_convention, NamingConvention::CamelCase);

        self.query_index = self
            .queries
            .iter()
            .enumerate()
            .flat_map(|(i, q)| {
                let mut entries = vec![(q.name.clone(), i)];
                if camel {
                    let converted = crate::utils::casing::to_camel_case(&q.name);
                    if converted != q.name {
                        entries.push((converted, i));
                    }
                }
                entries
            })
            .collect();

        self.mutation_index = self
            .mutations
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                let mut entries = vec![(m.name.clone(), i)];
                if camel {
                    let converted = crate::utils::casing::to_camel_case(&m.name);
                    if converted != m.name {
                        entries.push((converted, i));
                    }
                }
                entries
            })
            .collect();

        self.subscription_index = self
            .subscriptions
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                let mut entries = vec![(s.name.clone(), i)];
                if camel {
                    let converted = crate::utils::casing::to_camel_case(&s.name);
                    if converted != s.name {
                        entries.push((converted, i));
                    }
                }
                entries
            })
            .collect();
    }

    /// Return the display name for an operation, applying the naming convention.
    ///
    /// When `naming_convention` is `CamelCase`, converts `snake_case` names to
    /// `camelCase` (e.g., `create_dns_server` → `createDnsServer`).
    /// When `Preserve`, returns the name unchanged.
    #[must_use]
    pub fn display_name(&self, name: &str) -> String {
        match self.naming_convention {
            NamingConvention::CamelCase => crate::utils::casing::to_camel_case(name),
            NamingConvention::Preserve => name.to_string(),
        }
    }

    /// Find a type definition by name.
    #[must_use]
    pub fn find_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Whether the named type has any policy-gated field
    /// ([`FieldDefinition::authorize`](crate::schema::FieldDefinition)).
    ///
    /// Used by projection paths that do not (yet) run the dynamic field authorizer
    /// to fail closed conservatively when a gated type could be projected (#423).
    #[must_use]
    pub fn type_has_gated_field(&self, type_name: &str) -> bool {
        self.find_type(type_name).is_some_and(|t| t.fields.iter().any(|f| f.authorize))
    }

    /// Whether any object type in the schema declares a policy-gated field (#423).
    ///
    /// Used by projection paths with a dynamic/unknown result type (Relay `node`,
    /// federation `_entities`) to fail closed when field-level authorization is in
    /// use but cannot yet be enforced on that path.
    #[must_use]
    pub fn has_any_authorize_field(&self) -> bool {
        self.types.iter().any(|t| t.fields.iter().any(|f| f.authorize))
    }

    /// Find an enum definition by name.
    #[must_use]
    pub fn find_enum(&self, name: &str) -> Option<&EnumDefinition> {
        self.enums.iter().find(|e| e.name == name)
    }

    /// Find an input object definition by name.
    #[must_use]
    pub fn find_input_type(&self, name: &str) -> Option<&InputObjectDefinition> {
        self.input_types.iter().find(|i| i.name == name)
    }

    /// Find an interface definition by name.
    #[must_use]
    pub fn find_interface(&self, name: &str) -> Option<&InterfaceDefinition> {
        self.interfaces.iter().find(|i| i.name == name)
    }

    /// Find all types that implement a given interface.
    #[must_use]
    pub fn find_implementors(&self, interface_name: &str) -> Vec<&TypeDefinition> {
        self.types
            .iter()
            .filter(|t| t.implements.contains(&interface_name.to_string()))
            .collect()
    }

    /// Find a union definition by name.
    #[must_use]
    pub fn find_union(&self, name: &str) -> Option<&UnionDefinition> {
        self.unions.iter().find(|u| u.name == name)
    }

    /// Find a query definition by name.
    ///
    /// Uses the O(1) pre-built index when available; falls back to O(n) linear
    /// scan for schemas built directly in tests without calling `build_indexes()`.
    ///
    /// If the exact name is not found, retries with `to_snake_case(name)` to
    /// handle camelCase → `snake_case` normalization (e.g. `dnsServers` →
    /// `dns_servers`). This supports schemas compiled before the SDK camelCase
    /// migration.
    #[must_use]
    pub fn find_query(&self, name: &str) -> Option<&QueryDefinition> {
        if self.query_index.is_empty() && !self.queries.is_empty() {
            self.queries.iter().find(|q| q.name == name).or_else(|| {
                let snake = crate::utils::casing::to_snake_case(name);
                self.queries.iter().find(|q| q.name == snake)
            })
        } else {
            self.query_index
                .get(name)
                .or_else(|| self.query_index.get(&crate::utils::casing::to_snake_case(name)))
                .map(|&i| &self.queries[i])
        }
    }

    /// Find a mutation definition by name.
    ///
    /// Uses the O(1) pre-built index when available; falls back to O(n) linear
    /// scan for schemas built directly in tests without calling `build_indexes()`.
    ///
    /// If the exact name is not found, retries with `to_snake_case(name)` to
    /// handle camelCase → `snake_case` normalization. This supports schemas
    /// compiled before the SDK camelCase migration.
    #[must_use]
    pub fn find_mutation(&self, name: &str) -> Option<&MutationDefinition> {
        if self.mutation_index.is_empty() && !self.mutations.is_empty() {
            self.mutations.iter().find(|m| m.name == name).or_else(|| {
                let snake = crate::utils::casing::to_snake_case(name);
                self.mutations.iter().find(|m| m.name == snake)
            })
        } else {
            self.mutation_index
                .get(name)
                .or_else(|| self.mutation_index.get(&crate::utils::casing::to_snake_case(name)))
                .map(|&i| &self.mutations[i])
        }
    }

    /// Find a subscription definition by name.
    ///
    /// Uses the O(1) pre-built index when available; falls back to O(n) linear
    /// scan for schemas built directly in tests without calling `build_indexes()`.
    ///
    /// If the exact name is not found, retries with `to_snake_case(name)` to
    /// handle camelCase → `snake_case` normalization. This supports schemas
    /// compiled before the SDK camelCase migration.
    #[must_use]
    pub fn find_subscription(&self, name: &str) -> Option<&SubscriptionDefinition> {
        if self.subscription_index.is_empty() && !self.subscriptions.is_empty() {
            self.subscriptions.iter().find(|s| s.name == name).or_else(|| {
                let snake = crate::utils::casing::to_snake_case(name);
                self.subscriptions.iter().find(|s| s.name == snake)
            })
        } else {
            self.subscription_index
                .get(name)
                .or_else(|| self.subscription_index.get(&crate::utils::casing::to_snake_case(name)))
                .map(|&i| &self.subscriptions[i])
        }
    }

    /// Find a custom directive definition by name.
    #[must_use]
    pub fn find_directive(&self, name: &str) -> Option<&DirectiveDefinition> {
        self.directives.iter().find(|d| d.name == name)
    }

    /// Get total number of operations (queries + mutations + subscriptions).
    #[must_use]
    pub const fn operation_count(&self) -> usize {
        self.queries.len() + self.mutations.len() + self.subscriptions.len()
    }
}
