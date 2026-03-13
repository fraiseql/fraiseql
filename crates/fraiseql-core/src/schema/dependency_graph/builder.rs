//! Graph construction: builds the dependency graph from a compiled schema.

use std::collections::{HashMap, HashSet};

use crate::schema::{CompiledSchema, FieldType};

use super::graph::SchemaDependencyGraph;

impl SchemaDependencyGraph {
    /// Build a dependency graph from a compiled schema.
    ///
    /// This analyzes all types, queries, mutations, and subscriptions to
    /// build a complete dependency graph.
    #[must_use]
    pub fn build(schema: &CompiledSchema) -> Self {
        let mut outgoing: HashMap<String, HashSet<String>> = HashMap::new();
        let mut incoming: HashMap<String, HashSet<String>> = HashMap::new();
        let mut all_types: HashSet<String> = HashSet::new();
        let mut root_types: HashSet<String> = HashSet::new();

        // Collect all type names first
        for type_def in &schema.types {
            all_types.insert(type_def.name.to_string());
            outgoing.entry(type_def.name.to_string()).or_default();
            incoming.entry(type_def.name.to_string()).or_default();
        }

        for enum_def in &schema.enums {
            all_types.insert(enum_def.name.clone());
            outgoing.entry(enum_def.name.clone()).or_default();
            incoming.entry(enum_def.name.clone()).or_default();
        }

        for input_def in &schema.input_types {
            all_types.insert(input_def.name.clone());
            outgoing.entry(input_def.name.clone()).or_default();
            incoming.entry(input_def.name.clone()).or_default();
        }

        for interface_def in &schema.interfaces {
            all_types.insert(interface_def.name.clone());
            outgoing.entry(interface_def.name.clone()).or_default();
            incoming.entry(interface_def.name.clone()).or_default();
        }

        for union_def in &schema.unions {
            all_types.insert(union_def.name.clone());
            outgoing.entry(union_def.name.clone()).or_default();
            incoming.entry(union_def.name.clone()).or_default();
        }

        // Add virtual root types for operations
        if !schema.queries.is_empty() {
            root_types.insert("Query".to_string());
            all_types.insert("Query".to_string());
            outgoing.entry("Query".to_string()).or_default();
            incoming.entry("Query".to_string()).or_default();
        }
        if !schema.mutations.is_empty() {
            root_types.insert("Mutation".to_string());
            all_types.insert("Mutation".to_string());
            outgoing.entry("Mutation".to_string()).or_default();
            incoming.entry("Mutation".to_string()).or_default();
        }
        if !schema.subscriptions.is_empty() {
            root_types.insert("Subscription".to_string());
            all_types.insert("Subscription".to_string());
            outgoing.entry("Subscription".to_string()).or_default();
            incoming.entry("Subscription".to_string()).or_default();
        }

        // Build dependencies for object types
        for type_def in &schema.types {
            for field in &type_def.fields {
                if let Some(ref_type) = Self::extract_referenced_type(&field.field_type) {
                    if all_types.contains(&ref_type) {
                        outgoing.entry(type_def.name.to_string()).or_default().insert(ref_type.clone());
                        incoming.entry(ref_type.clone()).or_default().insert(type_def.name.to_string());
                    }
                }
            }

            // Track interface implementations
            for interface_name in &type_def.implements {
                if all_types.contains(interface_name) {
                    outgoing
                        .entry(type_def.name.to_string())
                        .or_default()
                        .insert(interface_name.clone());
                    incoming
                        .entry(interface_name.clone())
                        .or_default()
                        .insert(type_def.name.to_string());
                }
            }
        }

        // Build dependencies for interfaces
        for interface_def in &schema.interfaces {
            for field in &interface_def.fields {
                if let Some(ref_type) = Self::extract_referenced_type(&field.field_type) {
                    if all_types.contains(&ref_type) {
                        outgoing
                            .entry(interface_def.name.clone())
                            .or_default()
                            .insert(ref_type.clone());
                        incoming
                            .entry(ref_type.clone())
                            .or_default()
                            .insert(interface_def.name.clone());
                    }
                }
            }
        }

        // Build dependencies for unions
        for union_def in &schema.unions {
            for member_type in &union_def.member_types {
                if all_types.contains(member_type) {
                    outgoing.entry(union_def.name.clone()).or_default().insert(member_type.clone());
                    incoming.entry(member_type.clone()).or_default().insert(union_def.name.clone());
                }
            }
        }

        // Build dependencies for input types (they can reference other input types)
        for input_def in &schema.input_types {
            for field in &input_def.fields {
                // Parse the field_type string to extract type references
                let parsed = FieldType::parse(&field.field_type, &all_types);
                if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                    if all_types.contains(&ref_type) {
                        outgoing
                            .entry(input_def.name.clone())
                            .or_default()
                            .insert(ref_type.clone());
                        incoming
                            .entry(ref_type.clone())
                            .or_default()
                            .insert(input_def.name.clone());
                    }
                }
            }
        }

        // Build dependencies from queries to their return types
        for query in &schema.queries {
            let parsed = FieldType::parse(&query.return_type, &all_types);
            if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                if all_types.contains(&ref_type) {
                    outgoing.entry("Query".to_string()).or_default().insert(ref_type.clone());
                    incoming.entry(ref_type.clone()).or_default().insert("Query".to_string());
                }
            }
        }

        // Build dependencies from mutations to their return types
        for mutation in &schema.mutations {
            let parsed = FieldType::parse(&mutation.return_type, &all_types);
            if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                if all_types.contains(&ref_type) {
                    outgoing.entry("Mutation".to_string()).or_default().insert(ref_type.clone());
                    incoming.entry(ref_type.clone()).or_default().insert("Mutation".to_string());
                }
            }
        }

        // Build dependencies from subscriptions to their return types
        for subscription in &schema.subscriptions {
            let parsed = FieldType::parse(&subscription.return_type, &all_types);
            if let Some(ref_type) = Self::extract_referenced_type(&parsed) {
                if all_types.contains(&ref_type) {
                    outgoing
                        .entry("Subscription".to_string())
                        .or_default()
                        .insert(ref_type.clone());
                    incoming
                        .entry(ref_type.clone())
                        .or_default()
                        .insert("Subscription".to_string());
                }
            }
        }

        Self {
            outgoing,
            incoming,
            all_types,
            root_types,
        }
    }

    /// Extract the referenced type name from a `FieldType`, recursively unwrapping lists.
    pub(super) fn extract_referenced_type(field_type: &FieldType) -> Option<String> {
        match field_type {
            FieldType::Object(name)
            | FieldType::Enum(name)
            | FieldType::Input(name)
            | FieldType::Interface(name)
            | FieldType::Union(name) => Some(name.clone()),
            FieldType::List(inner) => Self::extract_referenced_type(inner),
            _ => None, // Scalars don't create dependencies
        }
    }
}
