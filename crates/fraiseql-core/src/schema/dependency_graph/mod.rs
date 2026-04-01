//! Schema dependency graph analysis.
//!
//! This module provides tools for analyzing type dependencies in a compiled schema,
//! including cycle detection, unused type detection, and impact analysis.
//!
//! # Example
//!
//! ```
//! use fraiseql_core::schema::{CompiledSchema, SchemaDependencyGraph};
//!
//! let schema = CompiledSchema::default();
//! let graph = SchemaDependencyGraph::build(&schema);
//!
//! // Check for circular dependencies
//! let cycles = graph.find_cycles();
//! if !cycles.is_empty() {
//!     for cycle in &cycles {
//!         println!("Cycle detected: {}", cycle.path_string());
//!     }
//! }
//!
//! // Find unused types
//! let unused = graph.find_unused();
//! for type_name in &unused {
//!     println!("Unused type: {}", type_name);
//! }
//! ```

mod analysis;
mod builder;
mod graph;
mod types;

pub use graph::SchemaDependencyGraph;
pub use types::{ChangeImpact, CyclePath};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldType,
        InputFieldDefinition, InputObjectDefinition, InterfaceDefinition, MutationDefinition,
        QueryDefinition, SubscriptionDefinition, TypeDefinition, UnionDefinition,
    };

    /// Helper to create a simple type with the given fields.
    fn make_type(name: &str, fields: Vec<(&str, FieldType)>) -> TypeDefinition {
        TypeDefinition {
            name: name.into(),
            sql_source: format!("v_{}", name.to_lowercase()).into(),
            jsonb_column: "data".to_string(),
            fields: fields.into_iter().map(|(n, ft)| FieldDefinition::new(n, ft)).collect(),
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: vec![],
        }
    }

    // =========================================================================
    // Basic Graph Construction Tests
    // =========================================================================

    #[test]
    fn test_empty_schema() {
        let schema = CompiledSchema::default();
        let graph = SchemaDependencyGraph::build(&schema);

        assert_eq!(graph.type_count(), 0);
        assert!(graph.find_cycles().is_empty());
        assert!(graph.find_unused().is_empty());
    }

    #[test]
    fn test_single_type_no_dependencies() {
        let schema = CompiledSchema {
            types: vec![make_type(
                "User",
                vec![
                    ("id", FieldType::Id),
                    ("name", FieldType::String),
                    ("email", FieldType::String),
                ],
            )],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("User"));
        assert!(graph.has_type("Query"));
        assert_eq!(graph.dependencies_of("User").len(), 0);
        assert_eq!(graph.dependents_of("User"), vec!["Query"]);
    }

    #[test]
    fn test_type_with_object_reference() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![
                        ("id", FieldType::Id),
                        ("profile", FieldType::Object("Profile".to_string())),
                    ],
                ),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // User depends on Profile
        assert_eq!(graph.dependencies_of("User"), vec!["Profile"]);
        // Profile is referenced by User
        assert_eq!(graph.dependents_of("Profile"), vec!["User"]);
    }

    #[test]
    fn test_type_with_list_reference() {
        let schema = CompiledSchema {
            types: vec![
                make_type(
                    "User",
                    vec![
                        ("id", FieldType::Id),
                        ("posts", FieldType::List(Box::new(FieldType::Object("Post".to_string())))),
                    ],
                ),
                make_type("Post", vec![("title", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // User depends on Post (through list)
        assert_eq!(graph.dependencies_of("User"), vec!["Post"]);
        assert_eq!(graph.dependents_of("Post"), vec!["User"]);
    }

    #[test]
    fn test_enum_reference() {
        let schema = CompiledSchema {
            types: vec![make_type(
                "User",
                vec![
                    ("id", FieldType::Id),
                    ("status", FieldType::Enum("UserStatus".to_string())),
                ],
            )],
            enums: vec![EnumDefinition {
                name: "UserStatus".to_string(),
                values: vec![
                    EnumValueDefinition::new("ACTIVE"),
                    EnumValueDefinition::new("INACTIVE"),
                ],
                description: None,
            }],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("UserStatus"));
        assert_eq!(graph.dependencies_of("User"), vec!["UserStatus"]);
        assert_eq!(graph.dependents_of("UserStatus"), vec!["User"]);
    }

    // =========================================================================
    // Cycle Detection Tests
    // =========================================================================

    #[test]
    fn test_no_cycles() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("profile", FieldType::Object("Profile".to_string()))]),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert!(cycles.is_empty());
    }

    #[test]
    fn test_self_referencing_cycle() {
        let schema = CompiledSchema {
            types: vec![make_type(
                "Node",
                vec![
                    ("id", FieldType::Id),
                    ("next", FieldType::Object("Node".to_string())),
                ],
            )],
            queries: vec![QueryDefinition::new("nodes", "Node").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].nodes, vec!["Node"]);
        assert!(cycles[0].is_self_reference());
        assert_eq!(cycles[0].path_string(), "Node → Node");
    }

    #[test]
    fn test_two_node_cycle() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("a", FieldType::Object("A".to_string()))]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 2);
        // Normalized to start from "A"
        assert_eq!(cycles[0].nodes, vec!["A", "B"]);
        assert_eq!(cycles[0].path_string(), "A → B → A");
    }

    #[test]
    fn test_three_node_cycle() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("a", FieldType::Object("A".to_string()))]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
        assert_eq!(cycles[0].nodes, vec!["A", "B", "C"]);
        assert_eq!(cycles[0].path_string(), "A → B → C → A");
    }

    #[test]
    fn test_multiple_independent_cycles() {
        let schema = CompiledSchema {
            types: vec![
                // Cycle 1: A <-> B
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("a", FieldType::Object("A".to_string()))]),
                // Cycle 2: X <-> Y
                make_type("X", vec![("y", FieldType::Object("Y".to_string()))]),
                make_type("Y", vec![("x", FieldType::Object("X".to_string()))]),
            ],
            queries: vec![
                QueryDefinition::new("aItems", "A").returning_list(),
                QueryDefinition::new("xItems", "X").returning_list(),
            ],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let cycles = graph.find_cycles();

        assert_eq!(cycles.len(), 2);
    }

    // =========================================================================
    // Unused Type Detection Tests
    // =========================================================================

    #[test]
    fn test_no_unused_types() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("profile", FieldType::Object("Profile".to_string()))]),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        assert!(unused.is_empty());
    }

    #[test]
    fn test_unused_type_no_references() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("name", FieldType::String)]),
                make_type("OrphanType", vec![("data", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        assert_eq!(unused, vec!["OrphanType"]);
    }

    #[test]
    fn test_multiple_unused_types() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("name", FieldType::String)]),
                make_type("Orphan1", vec![("data", FieldType::String)]),
                make_type("Orphan2", vec![("data", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        assert_eq!(unused, vec!["Orphan1", "Orphan2"]);
    }

    #[test]
    fn test_root_types_never_unused() {
        // Query type exists but has no incoming references (it's a root)
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);
        let unused = graph.find_unused();

        // Query should NOT appear in unused list (it's a root type)
        assert!(!unused.contains(&"Query".to_string()));
    }

    // =========================================================================
    // Impact Analysis Tests
    // =========================================================================

    #[test]
    fn test_impact_of_deletion_no_dependents() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("profile", FieldType::Object("Profile".to_string()))]),
                make_type("Profile", vec![("bio", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // Deleting Profile affects User (and Query transitively)
        let impact = graph.impact_of_deletion("Profile");
        assert!(impact.has_impact());
        assert!(impact.affected_types.contains("User"));
    }

    #[test]
    fn test_impact_of_deletion_chain() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("d", FieldType::Object("D".to_string()))]),
                make_type("D", vec![("value", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // Deleting D affects C, B, A (and Query)
        let impact = graph.impact_of_deletion("D");
        assert!(impact.affected_types.contains("C"));
        assert!(impact.affected_types.contains("B"));
        assert!(impact.affected_types.contains("A"));
    }

    // =========================================================================
    // Transitive Dependency Tests
    // =========================================================================

    #[test]
    fn test_transitive_dependencies() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("value", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        let deps = graph.transitive_dependencies("A");
        assert!(deps.contains("B"));
        assert!(deps.contains("C"));
        assert!(!deps.contains("A")); // Should not include self
    }

    #[test]
    fn test_transitive_dependents() {
        let schema = CompiledSchema {
            types: vec![
                make_type("A", vec![("b", FieldType::Object("B".to_string()))]),
                make_type("B", vec![("c", FieldType::Object("C".to_string()))]),
                make_type("C", vec![("value", FieldType::String)]),
            ],
            queries: vec![QueryDefinition::new("items", "A").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        let refs = graph.transitive_dependents("C");
        assert!(refs.contains("B"));
        assert!(refs.contains("A"));
        assert!(refs.contains("Query"));
        assert!(!refs.contains("C")); // Should not include self
    }

    // =========================================================================
    // Interface and Union Tests
    // =========================================================================

    #[test]
    fn test_interface_dependencies() {
        let schema = CompiledSchema {
            types: vec![TypeDefinition {
                name: "User".into(),
                sql_source: "v_user".into(),
                jsonb_column: "data".to_string(),
                fields: vec![FieldDefinition::new("id", FieldType::Id)],
                description: None,
                sql_projection_hint: None,
                implements: vec!["Node".to_string()],
                requires_role: None,
                is_error: false,
                relay: false,
                relationships: vec![],
            }],
            interfaces: vec![InterfaceDefinition {
                name: "Node".to_string(),
                fields: vec![FieldDefinition::new("id", FieldType::Id)],
                description: None,
            }],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // User depends on Node (implements it)
        assert!(graph.dependencies_of("User").contains(&"Node".to_string()));
        // Node is referenced by User
        assert!(graph.dependents_of("Node").contains(&"User".to_string()));
    }

    #[test]
    fn test_union_dependencies() {
        let schema = CompiledSchema {
            types: vec![
                make_type("User", vec![("name", FieldType::String)]),
                make_type("Post", vec![("title", FieldType::String)]),
            ],
            unions: vec![UnionDefinition {
                name: "SearchResult".to_string(),
                member_types: vec!["User".to_string(), "Post".to_string()],
                description: None,
            }],
            queries: vec![QueryDefinition::new("search", "SearchResult").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // SearchResult depends on User and Post
        let deps = graph.dependencies_of("SearchResult");
        assert!(deps.contains(&"User".to_string()));
        assert!(deps.contains(&"Post".to_string()));
    }

    // =========================================================================
    // Input Type Tests
    // =========================================================================

    #[test]
    fn test_input_type_dependencies() {
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            input_types: vec![
                InputObjectDefinition {
                    name: "UserFilter".to_string(),
                    fields: vec![InputFieldDefinition::new("status", "UserStatus")],
                    description: None,
                    metadata: None,
                },
                InputObjectDefinition {
                    name: "UserStatus".to_string(),
                    fields: vec![InputFieldDefinition::new("active", "Boolean")],
                    description: None,
                    metadata: None,
                },
            ],
            queries: vec![QueryDefinition::new("users", "User").returning_list()],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        // UserFilter depends on UserStatus
        assert!(graph.has_type("UserFilter"));
        assert!(graph.has_type("UserStatus"));
        assert!(graph.dependencies_of("UserFilter").contains(&"UserStatus".to_string()));
    }

    // =========================================================================
    // Mutation and Subscription Tests
    // =========================================================================

    #[test]
    fn test_mutation_return_type_dependency() {
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            mutations: vec![MutationDefinition::new("createUser", "User")],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("Mutation"));
        assert!(graph.dependencies_of("Mutation").contains(&"User".to_string()));
        assert!(graph.dependents_of("User").contains(&"Mutation".to_string()));
    }

    #[test]
    fn test_subscription_return_type_dependency() {
        let schema = CompiledSchema {
            types: vec![make_type("User", vec![("name", FieldType::String)])],
            subscriptions: vec![SubscriptionDefinition::new("userCreated", "User")],
            ..Default::default()
        };

        let graph = SchemaDependencyGraph::build(&schema);

        assert!(graph.has_type("Subscription"));
        assert!(graph.dependencies_of("Subscription").contains(&"User".to_string()));
    }

    // =========================================================================
    // CyclePath Tests
    // =========================================================================

    #[test]
    fn test_cycle_path_formatting() {
        let cycle = CyclePath::new(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        assert_eq!(cycle.path_string(), "A → B → C → A");
        assert_eq!(cycle.len(), 3);
        assert!(!cycle.is_self_reference());
        assert!(!cycle.is_empty());
    }

    #[test]
    fn test_cycle_path_self_reference() {
        let cycle = CyclePath::new(vec!["Node".to_string()]);
        assert_eq!(cycle.path_string(), "Node → Node");
        assert!(cycle.is_self_reference());
    }

    #[test]
    fn test_cycle_path_empty() {
        let cycle = CyclePath::new(vec![]);
        assert_eq!(cycle.path_string(), "");
        assert!(cycle.is_empty());
    }
}
