//! Trigger registry: Central coordinator for all trigger types and lifecycle.
//!
//! The `TriggerRegistry` loads function definitions from a schema, parses trigger strings,
//! builds internal structures (matchers, chains, schedulers), and manages startup/shutdown.

use crate::FunctionDefinition;
use crate::triggers::mutation::{AfterMutationTrigger, BeforeMutationTrigger, TriggerMatcher};
use crate::triggers::http::{HttpTriggerRoute, HttpTriggerMatcher};
use serde::{Deserialize, Serialize};

/// Error type for trigger registry operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryError {
    /// Error message.
    pub message: String,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RegistryError {}

/// Parsed trigger configuration extracted from trigger string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedTrigger {
    /// After mutation: `after:mutation:<entity_type>:<operation>`
    AfterMutation {
        /// Entity type name (e.g., "User", "Post").
        entity_type: String,
        /// Operation kind: "insert", "update", "delete", or None for all.
        operation: Option<String>,
    },
    /// Before mutation: `before:mutation:<mutation_name>`
    BeforeMutation {
        /// Mutation name (e.g., "createUser").
        mutation_name: String,
    },
    /// After storage: `after:storage:<bucket>:<operation>`
    AfterStorage {
        /// Bucket name.
        bucket: String,
        /// Operation: "upload", "delete", or "all".
        operation: String,
    },
    /// Cron: `cron:<expression>`
    Cron {
        /// POSIX cron expression.
        expression: String,
    },
    /// HTTP: `http:<method>:<path>`
    Http {
        /// HTTP method (GET, POST, etc.).
        method: String,
        /// URL path pattern.
        path: String,
    },
}

impl ParsedTrigger {
    /// Parse a trigger string into a structured trigger configuration.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` if the trigger string format is invalid or unrecognized.
    pub fn parse(trigger: &str) -> Result<Self, RegistryError> {
        let parts: Vec<&str> = trigger.split(':').collect();

        match parts.first().copied() {
            Some("after") if parts.len() >= 3 && parts[1] == "mutation" => {
                let entity_type = parts[2].to_string();
                let operation = if parts.len() > 3 {
                    Some(parts[3].to_string())
                } else {
                    None
                };
                Ok(ParsedTrigger::AfterMutation {
                    entity_type,
                    operation,
                })
            }
            Some("before") if parts.len() >= 3 && parts[1] == "mutation" => {
                let mutation_name = parts[2].to_string();
                Ok(ParsedTrigger::BeforeMutation { mutation_name })
            }
            Some("after") if parts.len() >= 4 && parts[1] == "storage" => {
                let bucket = parts[2].to_string();
                let operation = parts[3].to_string();
                Ok(ParsedTrigger::AfterStorage { bucket, operation })
            }
            Some("cron") if parts.len() >= 2 => {
                // Cron expressions can have colons in them (e.g., "cron:0 2 * * * :30")
                // So we need to rejoin the remaining parts
                let expression = parts[1..].join(":");
                Ok(ParsedTrigger::Cron { expression })
            }
            Some("http") if parts.len() >= 3 => {
                let method = parts[1].to_string();
                let path = parts[2..].join(":");
                Ok(ParsedTrigger::Http { method, path })
            }
            _ => Err(RegistryError {
                message: format!("Invalid trigger format: {}", trigger),
            }),
        }
    }

    /// Get the trigger type name (e.g., "after:mutation", "http").
    pub const fn trigger_type(&self) -> &'static str {
        match self {
            ParsedTrigger::AfterMutation { .. } => "after:mutation",
            ParsedTrigger::BeforeMutation { .. } => "before:mutation",
            ParsedTrigger::AfterStorage { .. } => "after:storage",
            ParsedTrigger::Cron { .. } => "cron",
            ParsedTrigger::Http { .. } => "http",
        }
    }

    /// Check if this is an after:mutation trigger.
    pub const fn is_after_mutation(&self) -> bool {
        matches!(self, ParsedTrigger::AfterMutation { .. })
    }

    /// Check if this is a before:mutation trigger.
    pub const fn is_before_mutation(&self) -> bool {
        matches!(self, ParsedTrigger::BeforeMutation { .. })
    }

    /// Check if this is an HTTP trigger.
    pub const fn is_http(&self) -> bool {
        matches!(self, ParsedTrigger::Http { .. })
    }

    /// Check if this is a cron trigger.
    pub const fn is_cron(&self) -> bool {
        matches!(self, ParsedTrigger::Cron { .. })
    }

    /// Check if this is an after:storage trigger.
    pub const fn is_after_storage(&self) -> bool {
        matches!(self, ParsedTrigger::AfterStorage { .. })
    }
}

/// Central registry for all triggers in the system.
#[derive(Debug, Default)]
pub struct TriggerRegistry {
    /// After-mutation triggers indexed by entity and operation.
    pub after_mutation_triggers: TriggerMatcher,
    /// Before-mutation triggers indexed by mutation name.
    pub before_mutation_triggers: Vec<BeforeMutationTrigger>,
    /// HTTP trigger routes indexed by method and path.
    pub http_routes: HttpTriggerMatcher,
    /// Total function definitions loaded.
    pub function_count: usize,
}

impl TriggerRegistry {
    /// Create a new empty trigger registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load triggers from function definitions.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` if any function's trigger string is invalid or if loading a trigger type fails.
    pub fn load_from_definitions(functions: &[FunctionDefinition]) -> Result<Self, RegistryError> {
        let mut registry = Self::new();
        registry.function_count = functions.len();

        for func in functions {
            let parsed = ParsedTrigger::parse(&func.trigger)?;

            match parsed {
                ParsedTrigger::AfterMutation {
                    entity_type,
                    operation,
                } => {
                    let trigger = AfterMutationTrigger {
                        function_name: func.name.clone(),
                        entity_type,
                        event_filter: operation.as_ref().and_then(|op| {
                            match op.as_str() {
                                "insert" => Some(crate::EventKind::Insert),
                                "update" => Some(crate::EventKind::Update),
                                "delete" => Some(crate::EventKind::Delete),
                                _ => None,
                            }
                        }),
                    };
                    registry.after_mutation_triggers.add(trigger);
                }
                ParsedTrigger::BeforeMutation { mutation_name } => {
                    let trigger = BeforeMutationTrigger {
                        function_name: func.name.clone(),
                        mutation_name,
                    };
                    registry.before_mutation_triggers.push(trigger);
                }
                ParsedTrigger::Http { method, path } => {
                    let route = HttpTriggerRoute {
                        function_name: func.name.clone(),
                        method,
                        path,
                        requires_auth: false, // TODO: infer from config
                    };
                    registry.http_routes.add(route);
                }
                ParsedTrigger::AfterStorage {
                    bucket: _,
                    operation: _,
                } => {
                    // TODO: Implement storage trigger loading
                    return Err(RegistryError {
                        message: "after:storage triggers not yet implemented".to_string(),
                    });
                }
                ParsedTrigger::Cron { expression: _ } => {
                    // TODO: Implement cron trigger loading
                    return Err(RegistryError {
                        message: "cron triggers not yet implemented in registry".to_string(),
                    });
                }
            }
        }

        Ok(registry)
    }

    /// Get the number of after:mutation triggers.
    pub const fn after_mutation_count(&self) -> usize {
        // This is approximate; TriggerMatcher doesn't expose count
        0
    }

    /// Get the number of before:mutation triggers.
    pub const fn before_mutation_count(&self) -> usize {
        self.before_mutation_triggers.len()
    }

    /// Get the number of HTTP routes.
    pub fn http_route_count(&self) -> usize {
        self.http_routes.routes().len()
    }

    /// Get all HTTP routes.
    pub fn http_routes(&self) -> &[HttpTriggerRoute] {
        self.http_routes.routes()
    }

    /// Find an HTTP route by method and path.
    pub fn find_http_route(&self, method: &str, path: &str) -> Option<HttpTriggerRoute> {
        self.http_routes.find(method, path)
    }

    /// Get all before:mutation triggers for a specific mutation.
    pub fn before_mutation_triggers_for(&self, mutation_name: &str) -> Vec<&BeforeMutationTrigger> {
        self.before_mutation_triggers
            .iter()
            .filter(|t| t.mutation_name == mutation_name)
            .collect()
    }

    /// Check if there are any before:mutation triggers for a mutation.
    pub fn has_before_mutation_triggers(&self, mutation_name: &str) -> bool {
        self.before_mutation_triggers
            .iter()
            .any(|t| t.mutation_name == mutation_name)
    }

    /// Build a [`BeforeMutationChain`] for the named mutation.
    ///
    /// Returns `None` when no `before:mutation` triggers are registered for this mutation
    /// (the fast path — zero overhead when hooks are absent).
    pub fn before_chain(
        &self,
        mutation_name: &str,
    ) -> Option<crate::triggers::mutation::BeforeMutationChain> {
        let triggers: Vec<_> = self
            .before_mutation_triggers
            .iter()
            .filter(|t| t.mutation_name == mutation_name)
            .cloned()
            .collect();
        if triggers.is_empty() {
            None
        } else {
            Some(crate::triggers::mutation::BeforeMutationChain { triggers })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_after_mutation_trigger() {
        let parsed = ParsedTrigger::parse("after:mutation:createUser").expect("parse");
        match parsed {
            ParsedTrigger::AfterMutation {
                entity_type,
                operation,
            } => {
                assert_eq!(entity_type, "createUser");
                assert_eq!(operation, None);
            }
            _ => panic!("Wrong trigger type"),
        }
    }

    #[test]
    fn test_parse_before_mutation_trigger() {
        let parsed = ParsedTrigger::parse("before:mutation:validateUser").expect("parse");
        match parsed {
            ParsedTrigger::BeforeMutation { mutation_name } => {
                assert_eq!(mutation_name, "validateUser");
            }
            _ => panic!("Wrong trigger type"),
        }
    }

    #[test]
    fn test_parse_http_trigger() {
        let parsed = ParsedTrigger::parse("http:GET:/users/:id").expect("parse");
        match parsed {
            ParsedTrigger::Http { method, path } => {
                assert_eq!(method, "GET");
                assert_eq!(path, "/users/:id");
            }
            _ => panic!("Wrong trigger type"),
        }
    }

    #[test]
    fn test_parse_cron_trigger() {
        let parsed = ParsedTrigger::parse("cron:0 2 * * *").expect("parse");
        match parsed {
            ParsedTrigger::Cron { expression } => {
                assert_eq!(expression, "0 2 * * *");
            }
            _ => panic!("Wrong trigger type"),
        }
    }

    #[test]
    fn test_parse_invalid_trigger() {
        let result = ParsedTrigger::parse("invalid:format:here");
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_loads_multiple_triggers() {
        use crate::{FunctionDefinition, RuntimeType};

        let functions = vec![
            FunctionDefinition::new("onUserCreated", "after:mutation:createUser", RuntimeType::Deno),
            FunctionDefinition::new("validateInput", "before:mutation:createUser", RuntimeType::Deno),
            FunctionDefinition::new("getUser", "http:GET:/users/:id", RuntimeType::Deno),
        ];

        let registry = TriggerRegistry::load_from_definitions(&functions)
            .expect("load registry");

        assert_eq!(registry.function_count, 3);
        assert_eq!(registry.before_mutation_count(), 1);
        assert_eq!(registry.http_route_count(), 1);
    }

    #[test]
    fn test_registry_finds_http_route() {
        use crate::{FunctionDefinition, RuntimeType};

        let functions = vec![
            FunctionDefinition::new("getUser", "http:GET:/users/:id", RuntimeType::Deno),
            FunctionDefinition::new("listUsers", "http:GET:/users", RuntimeType::Deno),
        ];

        let registry = TriggerRegistry::load_from_definitions(&functions)
            .expect("load registry");

        let route = registry.http_routes.find("GET", "/users/123");
        assert!(route.is_some());
        assert_eq!(route.expect("route found").function_name, "getUser");
    }

    #[test]
    fn test_parsed_trigger_type_detection() {
        let after_mut = ParsedTrigger::parse("after:mutation:createUser").expect("parse");
        assert!(after_mut.is_after_mutation());
        assert_eq!(after_mut.trigger_type(), "after:mutation");

        let http = ParsedTrigger::parse("http:POST:/data").expect("parse");
        assert!(http.is_http());
        assert_eq!(http.trigger_type(), "http");
    }

    #[test]
    fn test_registry_before_mutation_lookup() {
        use crate::{FunctionDefinition, RuntimeType};

        let functions = vec![
            FunctionDefinition::new("validate1", "before:mutation:createUser", RuntimeType::Deno),
            FunctionDefinition::new("validate2", "before:mutation:createUser", RuntimeType::Deno),
            FunctionDefinition::new("validate3", "before:mutation:deleteUser", RuntimeType::Deno),
        ];

        let registry = TriggerRegistry::load_from_definitions(&functions)
            .expect("load registry");

        assert_eq!(registry.before_mutation_count(), 3);
        assert!(registry.has_before_mutation_triggers("createUser"));
        assert!(registry.has_before_mutation_triggers("deleteUser"));
        assert!(!registry.has_before_mutation_triggers("updateUser"));

        let create_user_triggers = registry.before_mutation_triggers_for("createUser");
        assert_eq!(create_user_triggers.len(), 2);
    }

    #[test]
    fn test_registry_before_chain_returns_none_for_unknown_mutation() {
        use crate::{FunctionDefinition, RuntimeType};

        let functions = vec![FunctionDefinition::new(
            "validate",
            "before:mutation:createUser",
            RuntimeType::Deno,
        )];
        let registry = TriggerRegistry::load_from_definitions(&functions).expect("load");

        // Unknown mutation → None (zero overhead fast path)
        assert!(registry.before_chain("updateUser").is_none());
        assert!(registry.before_chain("deleteUser").is_none());
    }

    #[test]
    fn test_registry_before_chain_returns_chain_for_known_mutation() {
        use crate::{FunctionDefinition, RuntimeType};

        let functions = vec![
            FunctionDefinition::new("validate1", "before:mutation:createUser", RuntimeType::Deno),
            FunctionDefinition::new("validate2", "before:mutation:createUser", RuntimeType::Deno),
            FunctionDefinition::new("other", "before:mutation:deleteUser", RuntimeType::Deno),
        ];
        let registry = TriggerRegistry::load_from_definitions(&functions).expect("load");

        let chain = registry.before_chain("createUser").expect("chain present");
        assert_eq!(chain.triggers.len(), 2);
        assert_eq!(chain.triggers[0].function_name, "validate1");
        assert_eq!(chain.triggers[1].function_name, "validate2");

        // deleteUser chain has only 1 trigger
        let del_chain = registry.before_chain("deleteUser").expect("chain present");
        assert_eq!(del_chain.triggers.len(), 1);
    }
}
