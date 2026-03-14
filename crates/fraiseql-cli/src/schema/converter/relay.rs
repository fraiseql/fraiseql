use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, InterfaceDefinition,
    TypeDefinition,
};

/// Inject PageInfo, Node interface, and XxxConnection/XxxEdge types for each relay type.
///
/// For each type `T` with `relay = true`:
/// - Adds `"Node"` to `T.implements` (if not already present)
/// - Generates `TEdge { cursor: String!, node: T! }`
/// - Generates `TConnection { edges: [TEdge!]!, pageInfo: PageInfo! }`
///
/// Also generates:
/// - `PageInfo` type (once): `{ hasNextPage: Boolean!, hasPreviousPage: Boolean!, startCursor:
///   String, endCursor: String }`
/// - `Node` interface (once): `{ id: ID! }`
pub(super) fn inject_relay_types(schema: &mut CompiledSchema) {
    // Collect relay type names (those with relay=true).
    let relay_types: Vec<String> =
        schema.types.iter().filter(|t| t.relay).map(|t| t.name.to_string()).collect();

    if relay_types.is_empty() {
        return;
    }

    // --- Node interface (inject once if not already present) ---
    let has_node_interface = schema.interfaces.iter().any(|i| i.name == "Node");
    if !has_node_interface {
        let node_id_field = FieldDefinition {
            name:           "id".into(),
            field_type:     FieldType::Id,
            nullable:       false,
            description:    Some("Globally unique identifier (UUID).".to_string()),
            default_value:  None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: None,
            on_deny:        FieldDenyPolicy::default(),
            encryption:     None,
        };
        schema.interfaces.push(
            InterfaceDefinition::new("Node")
                .with_description("Relay Node interface — types with a globally unique ID.")
                .with_field(node_id_field),
        );
    }

    // --- PageInfo type (inject once if not already present) ---
    let has_page_info = schema.types.iter().any(|t| t.name == "PageInfo");
    if !has_page_info {
        let make_field = |name: &str, ft: FieldType, nullable: bool, desc: &str| FieldDefinition {
            name: name.into(),
            field_type: ft,
            nullable,
            description: Some(desc.to_string()),
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            encryption: None,
        };
        let page_info = TypeDefinition {
            name:                "PageInfo".into(),
            sql_source:          String::new().into(), // synthetic — no DB source
            jsonb_column:        String::new(),
            fields:              vec![
                make_field(
                    "hasNextPage",
                    FieldType::Boolean,
                    false,
                    "Whether there are more items after the current page.",
                ),
                make_field(
                    "hasPreviousPage",
                    FieldType::Boolean,
                    false,
                    "Whether there are more items before the current page.",
                ),
                make_field(
                    "startCursor",
                    FieldType::String,
                    true,
                    "Cursor for the first item in the current page.",
                ),
                make_field(
                    "endCursor",
                    FieldType::String,
                    true,
                    "Cursor for the last item in the current page.",
                ),
            ],
            description:         Some("Relay pagination info.".to_string()),
            sql_projection_hint: None,
            implements:          Vec::new(),
            requires_role:       None,
            is_error:            false,
            relay:               false,
        };
        schema.types.push(page_info);
    }

    // --- Add "Node" to implements of each relay type ---
    for type_def in &mut schema.types {
        if type_def.relay && !type_def.implements.iter().any(|i| i == "Node") {
            type_def.implements.push("Node".to_string());
        }
    }

    // --- Generate XxxEdge and XxxConnection for each relay type ---
    let make_field = |name: &str, ft: FieldType, nullable: bool, desc: &str| FieldDefinition {
        name: name.into(),
        field_type: ft,
        nullable,
        description: Some(desc.to_string()),
        default_value: None,
        vector_config: None,
        alias: None,
        deprecation: None,
        requires_scope: None,
        on_deny: FieldDenyPolicy::default(),
        encryption: None,
    };

    let mut new_types: Vec<TypeDefinition> = Vec::new();

    for type_name in &relay_types {
        let edge_name = format!("{type_name}Edge");
        let conn_name = format!("{type_name}Connection");

        // Only inject if not already defined (allow user overrides).
        let has_edge = schema.types.iter().any(|t| t.name == edge_name);
        if !has_edge {
            new_types.push(TypeDefinition {
                name:                edge_name.clone().into(),
                sql_source:          String::new().into(),
                jsonb_column:        String::new(),
                fields:              vec![
                    make_field(
                        "cursor",
                        FieldType::String,
                        false,
                        "Opaque pagination cursor (base64-encoded BIGINT pk).",
                    ),
                    // nullable: true — per Relay spec an edge node may be null
                    // when the underlying object is deleted or access is denied.
                    make_field(
                        "node",
                        FieldType::Object(type_name.clone()),
                        true,
                        "The item at this edge.",
                    ),
                ],
                description:         Some(format!("An edge in the {type_name} Relay connection.")),
                sql_projection_hint: None,
                implements:          Vec::new(),
                requires_role:       None,
                is_error:            false,
                relay:               false,
            });
        }

        let has_conn = schema.types.iter().any(|t| t.name == conn_name);
        if !has_conn {
            new_types.push(TypeDefinition {
                name:                conn_name.into(),
                sql_source:          String::new().into(),
                jsonb_column:        String::new(),
                fields:              vec![
                    make_field(
                        "edges",
                        FieldType::List(Box::new(FieldType::Object(edge_name))),
                        false,
                        "List of edges.",
                    ),
                    make_field(
                        "pageInfo",
                        FieldType::Object("PageInfo".to_string()),
                        false,
                        "Pagination metadata.",
                    ),
                    make_field(
                        "totalCount",
                        FieldType::Int,
                        true,
                        "Total number of items matching the filter.",
                    ),
                ],
                description:         Some(format!(
                    "A Relay connection for paginating {type_name} records."
                )),
                sql_projection_hint: None,
                implements:          Vec::new(),
                requires_role:       None,
                is_error:            false,
                relay:               false,
            });
        }
    }

    schema.types.extend(new_types);
}
