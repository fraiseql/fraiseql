//! `__schema` query response construction.
//!
//! Builds the Query, Mutation, and Subscription root introspection types, and
//! the `IntrospectionBuilder` and `IntrospectionResponses` public entry points.

use std::collections::HashMap;

use super::super::{
    CompiledSchema, MutationDefinition, QueryDefinition, SubscriptionDefinition,
};
use super::directive_builder::{build_custom_directives, builtin_directives};
use super::field_resolver::{build_arg_input_value, type_ref};
use super::type_resolver::{
    build_enum_type, build_input_object_type, build_interface_type, build_object_type,
    build_union_type, builtin_scalars,
};
use super::types::{
    IntrospectionField, IntrospectionInputValue, IntrospectionSchema, IntrospectionType,
    IntrospectionTypeRef, TypeKind,
};

// =============================================================================
// IntrospectionBuilder
// =============================================================================

/// Builds introspection schema from compiled schema.
pub struct IntrospectionBuilder;

impl IntrospectionBuilder {
    /// Build complete introspection schema from compiled schema.
    #[must_use]
    pub fn build(schema: &CompiledSchema) -> IntrospectionSchema {
        let mut types = Vec::new();

        // Add built-in scalar types
        types.extend(builtin_scalars());

        // Add user-defined types
        for type_def in &schema.types {
            types.push(build_object_type(type_def));
        }

        // Add enum types
        for enum_def in &schema.enums {
            types.push(build_enum_type(enum_def));
        }

        // Add input object types
        for input_def in &schema.input_types {
            types.push(build_input_object_type(input_def));
        }

        // Add interface types
        for interface_def in &schema.interfaces {
            types.push(build_interface_type(interface_def, schema));
        }

        // Add union types
        for union_def in &schema.unions {
            types.push(build_union_type(union_def));
        }

        // Add Query root type
        types.push(build_query_type(schema));

        // Add Mutation root type if mutations exist
        if !schema.mutations.is_empty() {
            types.push(build_mutation_type(schema));
        }

        // Add Subscription root type if subscriptions exist
        if !schema.subscriptions.is_empty() {
            types.push(build_subscription_type(schema));
        }

        // Build directives: built-in + custom
        let mut directives = builtin_directives();
        directives.extend(build_custom_directives(&schema.directives));

        IntrospectionSchema {
            description: Some("FraiseQL GraphQL Schema".to_string()),
            types,
            query_type: IntrospectionTypeRef {
                name: "Query".to_string(),
            },
            mutation_type: if schema.mutations.is_empty() {
                None
            } else {
                Some(IntrospectionTypeRef {
                    name: "Mutation".to_string(),
                })
            },
            subscription_type: if schema.subscriptions.is_empty() {
                None
            } else {
                Some(IntrospectionTypeRef {
                    name: "Subscription".to_string(),
                })
            },
            directives,
        }
    }

    /// Build a lookup map for `__type(name:)` queries.
    #[must_use]
    pub fn build_type_map(schema: &IntrospectionSchema) -> HashMap<String, IntrospectionType> {
        let mut map = HashMap::new();
        for t in &schema.types {
            if let Some(ref name) = t.name {
                map.insert(name.clone(), t.clone());
            }
        }
        map
    }

    /// Expose `type_ref` as an associated function for use in tests.
    #[must_use]
    pub fn type_ref(name: &str) -> IntrospectionType {
        type_ref(name)
    }
}

// =============================================================================
// Root type builders
// =============================================================================

/// Build Query root type.
fn build_query_type(schema: &CompiledSchema) -> IntrospectionType {
    let mut fields: Vec<IntrospectionField> =
        schema.queries.iter().map(build_query_field).collect();

    // Inject synthetic `node(id: ID!): Node` field when relay types exist.
    let has_relay_types = schema.types.iter().any(|t| t.relay)
        || schema.interfaces.iter().any(|i| i.name == "Node");
    if has_relay_types && !fields.iter().any(|f| f.name == "node") {
        fields.push(build_node_query_field());
    }

    IntrospectionType {
        kind:               TypeKind::Object,
        name:               Some("Query".to_string()),
        description:        Some("Root query type".to_string()),
        fields:             Some(fields),
        interfaces:         Some(vec![]),
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    }
}

/// Build Mutation root type.
fn build_mutation_type(schema: &CompiledSchema) -> IntrospectionType {
    let fields: Vec<IntrospectionField> =
        schema.mutations.iter().map(build_mutation_field).collect();

    IntrospectionType {
        kind:               TypeKind::Object,
        name:               Some("Mutation".to_string()),
        description:        Some("Root mutation type".to_string()),
        fields:             Some(fields),
        interfaces:         Some(vec![]),
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    }
}

/// Build Subscription root type.
fn build_subscription_type(schema: &CompiledSchema) -> IntrospectionType {
    let fields: Vec<IntrospectionField> =
        schema.subscriptions.iter().map(build_subscription_field).collect();

    IntrospectionType {
        kind:               TypeKind::Object,
        name:               Some("Subscription".to_string()),
        description:        Some("Root subscription type".to_string()),
        fields:             Some(fields),
        interfaces:         Some(vec![]),
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    }
}

// =============================================================================
// Operation field builders
// =============================================================================

/// Build query field introspection.
fn build_query_field(query: &QueryDefinition) -> IntrospectionField {
    // Relay connection queries expose `XxxConnection` as their return type
    // (always non-null) and add the four standard cursor arguments.
    if query.relay {
        return build_relay_query_field(query);
    }

    let return_type = type_ref(&query.return_type);
    let return_type = if query.returns_list {
        IntrospectionType {
            kind:               TypeKind::List,
            name:               None,
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            Some(Box::new(return_type)),
            specified_by_u_r_l: None,
        }
    } else {
        return_type
    };

    let return_type = if query.nullable {
        return_type
    } else {
        IntrospectionType {
            kind:               TypeKind::NonNull,
            name:               None,
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            Some(Box::new(return_type)),
            specified_by_u_r_l: None,
        }
    };

    // Build arguments
    let args: Vec<IntrospectionInputValue> =
        query.arguments.iter().map(build_arg_input_value).collect();

    IntrospectionField {
        name: query.name.clone(),
        description: query.description.clone(),
        args,
        field_type: return_type,
        is_deprecated: query.is_deprecated(),
        deprecation_reason: query.deprecation_reason().map(ToString::to_string),
    }
}

/// Build introspection for a Relay connection query.
///
/// Relay connection queries differ from normal list queries:
/// - Return type is `XxxConnection!` (non-null), not `[Xxx!]!`
/// - Arguments are `first: Int, after: String, last: Int, before: String`
///   (instead of `limit`/`offset`)
fn build_relay_query_field(query: &QueryDefinition) -> IntrospectionField {
    let connection_type = format!("{}Connection", query.return_type);

    // Return type: XxxConnection! (always non-null)
    let return_type = IntrospectionType {
        kind:               TypeKind::NonNull,
        name:               None,
        description:        None,
        fields:             None,
        interfaces:         None,
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            Some(Box::new(type_ref(&connection_type))),
        specified_by_u_r_l: None,
    };

    // Standard Relay cursor arguments.
    let nullable_int = || IntrospectionType {
        kind:               TypeKind::Scalar,
        name:               Some("Int".to_string()),
        description:        None,
        fields:             None,
        interfaces:         None,
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    };
    let nullable_string = || IntrospectionType {
        kind:               TypeKind::Scalar,
        name:               Some("String".to_string()),
        description:        None,
        fields:             None,
        interfaces:         None,
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    };
    let relay_args = vec![
        IntrospectionInputValue {
            name:               "first".to_string(),
            description:        Some("Return the first N items.".to_string()),
            input_type:         nullable_int(),
            default_value:      None,
            is_deprecated:      false,
            deprecation_reason: None,
            validation_rules:   vec![],
        },
        IntrospectionInputValue {
            name:               "after".to_string(),
            description:        Some("Cursor: return items after this position.".to_string()),
            input_type:         nullable_string(),
            default_value:      None,
            is_deprecated:      false,
            deprecation_reason: None,
            validation_rules:   vec![],
        },
        IntrospectionInputValue {
            name:               "last".to_string(),
            description:        Some("Return the last N items.".to_string()),
            input_type:         nullable_int(),
            default_value:      None,
            is_deprecated:      false,
            deprecation_reason: None,
            validation_rules:   vec![],
        },
        IntrospectionInputValue {
            name:               "before".to_string(),
            description:        Some("Cursor: return items before this position.".to_string()),
            input_type:         nullable_string(),
            default_value:      None,
            is_deprecated:      false,
            deprecation_reason: None,
            validation_rules:   vec![],
        },
    ];

    IntrospectionField {
        name:               query.name.clone(),
        description:        query.description.clone(),
        args:               relay_args,
        field_type:         return_type,
        is_deprecated:      query.is_deprecated(),
        deprecation_reason: query.deprecation_reason().map(ToString::to_string),
    }
}

/// Build the synthetic `node(id: ID!): Node` field for the Query root type.
///
/// Injected automatically when the schema contains Relay types (relay=true).
fn build_node_query_field() -> IntrospectionField {
    // Return type: Node (nullable per Relay spec — unknown id returns null).
    // Kind must be INTERFACE because Node is declared as an interface type,
    // not an OBJECT. Relay's compiler uses this to dispatch `... on User` fragments.
    let return_type = IntrospectionType {
        kind:               TypeKind::Interface,
        name:               Some("Node".to_string()),
        description:        None,
        fields:             None,
        interfaces:         None,
        possible_types:     None,
        enum_values:        None,
        input_fields:       None,
        of_type:            None,
        specified_by_u_r_l: None,
    };

    // Argument: id: ID! (non-null)
    let id_arg = IntrospectionInputValue {
        name:               "id".to_string(),
        description:        Some("Globally unique opaque identifier.".to_string()),
        input_type:         IntrospectionType {
            kind:               TypeKind::NonNull,
            name:               None,
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            Some(Box::new(type_ref("ID"))),
            specified_by_u_r_l: None,
        },
        default_value:      None,
        is_deprecated:      false,
        deprecation_reason: None,
        validation_rules:   vec![],
    };

    IntrospectionField {
        name:               "node".to_string(),
        description:        Some(
            "Fetch any object that implements the Node interface by its global ID.".to_string(),
        ),
        args:               vec![id_arg],
        field_type:         return_type,
        is_deprecated:      false,
        deprecation_reason: None,
    }
}

/// Build mutation field introspection.
fn build_mutation_field(mutation: &MutationDefinition) -> IntrospectionField {
    // Mutations always return a single object (not a list)
    let return_type = type_ref(&mutation.return_type);

    // Build arguments
    let args: Vec<IntrospectionInputValue> =
        mutation.arguments.iter().map(build_arg_input_value).collect();

    IntrospectionField {
        name: mutation.name.clone(),
        description: mutation.description.clone(),
        args,
        field_type: return_type,
        is_deprecated: mutation.is_deprecated(),
        deprecation_reason: mutation.deprecation_reason().map(ToString::to_string),
    }
}

/// Build subscription field introspection.
fn build_subscription_field(subscription: &SubscriptionDefinition) -> IntrospectionField {
    // Subscriptions typically return a single item per event
    let return_type = type_ref(&subscription.return_type);

    // Build arguments
    let args: Vec<IntrospectionInputValue> =
        subscription.arguments.iter().map(build_arg_input_value).collect();

    IntrospectionField {
        name: subscription.name.clone(),
        description: subscription.description.clone(),
        args,
        field_type: return_type,
        is_deprecated: subscription.is_deprecated(),
        deprecation_reason: subscription.deprecation_reason().map(ToString::to_string),
    }
}

// =============================================================================
// IntrospectionResponses
// =============================================================================

/// Pre-built introspection responses for fast serving.
#[derive(Debug, Clone)]
pub struct IntrospectionResponses {
    /// Full `__schema` response JSON.
    pub schema_response: String,
    /// Map of type name -> `__type` response JSON.
    pub type_responses:  HashMap<String, String>,
}

impl IntrospectionResponses {
    /// Build introspection responses from compiled schema.
    ///
    /// This is called once at server startup and cached.
    #[must_use]
    pub fn build(schema: &CompiledSchema) -> Self {
        let introspection = IntrospectionBuilder::build(schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Build __schema response
        let schema_response = serde_json::json!({
            "data": {
                "__schema": introspection
            }
        })
        .to_string();

        // Build __type responses for each type
        let mut type_responses = HashMap::new();
        for (name, t) in type_map {
            let response = serde_json::json!({
                "data": {
                    "__type": t
                }
            })
            .to_string();
            type_responses.insert(name, response);
        }

        Self {
            schema_response,
            type_responses,
        }
    }

    /// Get response for `__type(name: "...")` query.
    #[must_use]
    pub fn get_type_response(&self, type_name: &str) -> String {
        self.type_responses.get(type_name).cloned().unwrap_or_else(|| {
            serde_json::json!({
                "data": {
                    "__type": null
                }
            })
            .to_string()
        })
    }
}

