//! Trigger registry: Central coordinator for all trigger types and lifecycle.
//!
//! The `TriggerRegistry` loads function definitions from a schema, parses trigger strings,
//! builds internal structures (matchers, chains, schedulers), and manages startup/shutdown.

use serde::{Deserialize, Serialize};

use crate::{
    FunctionDefinition,
    triggers::{
        http::{HttpTriggerMatcher, HttpTriggerRoute},
        ingest::{InboundMessage, IngestSource, IngestTrigger},
        mutation::{AfterMutationTrigger, BeforeMutationTrigger, TriggerMatcher},
    },
};

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
        operation:   Option<String>,
    },
    /// Before mutation: `before:mutation:<mutation_name>`
    BeforeMutation {
        /// Mutation name (e.g., "createUser").
        mutation_name: String,
    },
    /// After storage: `after:storage:<bucket>:<operation>`
    AfterStorage {
        /// Bucket name.
        bucket:    String,
        /// Operation: "upload", "delete", or "all".
        operation: String,
    },
    /// After ingest: `after:ingest[:<source>]` (e.g. `after:ingest:webhook:stripe`).
    AfterIngest {
        /// Source discriminant (`webhook:<provider>` / `email`); `None` matches
        /// every source.
        source: Option<String>,
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
        path:   String,
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
            },
            Some("before") if parts.len() >= 3 && parts[1] == "mutation" => {
                let mutation_name = parts[2].to_string();
                Ok(ParsedTrigger::BeforeMutation { mutation_name })
            },
            Some("after") if parts.len() >= 4 && parts[1] == "storage" => {
                let bucket = parts[2].to_string();
                let operation = parts[3].to_string();
                Ok(ParsedTrigger::AfterStorage { bucket, operation })
            },
            Some("after") if parts.len() >= 2 && parts[1] == "ingest" => {
                // The source discriminant may itself contain a colon
                // (`webhook:stripe`), so rejoin everything past `after:ingest`.
                let source = if parts.len() > 2 {
                    Some(parts[2..].join(":"))
                } else {
                    None
                };
                Ok(ParsedTrigger::AfterIngest { source })
            },
            Some("cron") if parts.len() >= 2 => {
                // Cron expressions can have colons in them (e.g., "cron:0 2 * * * :30")
                // So we need to rejoin the remaining parts
                let expression = parts[1..].join(":");
                Ok(ParsedTrigger::Cron { expression })
            },
            Some("http") if parts.len() >= 3 => {
                let method = parts[1].to_string();
                let path = parts[2..].join(":");
                Ok(ParsedTrigger::Http { method, path })
            },
            _ => Err(RegistryError {
                message: format!("Invalid trigger format: {}", trigger),
            }),
        }
    }

    /// Get the trigger type name (e.g., "after:mutation", "http").
    #[must_use]
    pub const fn trigger_type(&self) -> &'static str {
        match self {
            ParsedTrigger::AfterMutation { .. } => "after:mutation",
            ParsedTrigger::BeforeMutation { .. } => "before:mutation",
            ParsedTrigger::AfterStorage { .. } => "after:storage",
            ParsedTrigger::AfterIngest { .. } => "after:ingest",
            ParsedTrigger::Cron { .. } => "cron",
            ParsedTrigger::Http { .. } => "http",
        }
    }

    /// Check if this is an after:mutation trigger.
    #[must_use]
    pub const fn is_after_mutation(&self) -> bool {
        matches!(self, ParsedTrigger::AfterMutation { .. })
    }

    /// Check if this is a before:mutation trigger.
    #[must_use]
    pub const fn is_before_mutation(&self) -> bool {
        matches!(self, ParsedTrigger::BeforeMutation { .. })
    }

    /// Check if this is an HTTP trigger.
    #[must_use]
    pub const fn is_http(&self) -> bool {
        matches!(self, ParsedTrigger::Http { .. })
    }

    /// Check if this is a cron trigger.
    #[must_use]
    pub const fn is_cron(&self) -> bool {
        matches!(self, ParsedTrigger::Cron { .. })
    }

    /// Check if this is an after:storage trigger.
    #[must_use]
    pub const fn is_after_storage(&self) -> bool {
        matches!(self, ParsedTrigger::AfterStorage { .. })
    }

    /// Check if this is an after:ingest trigger.
    #[must_use]
    pub const fn is_after_ingest(&self) -> bool {
        matches!(self, ParsedTrigger::AfterIngest { .. })
    }
}

/// Central registry for all triggers in the system.
#[derive(Debug, Default)]
pub struct TriggerRegistry {
    /// After-mutation triggers indexed by entity and operation.
    pub after_mutation_triggers:  TriggerMatcher,
    /// Before-mutation triggers indexed by mutation name.
    pub before_mutation_triggers: Vec<BeforeMutationTrigger>,
    /// HTTP trigger routes indexed by method and path.
    pub http_routes:              HttpTriggerMatcher,
    /// Cron-scheduled triggers.
    pub cron_triggers:            Vec<crate::triggers::cron::CronTrigger>,
    /// `after:ingest` triggers for inbound-message ingestion.
    pub ingest_triggers:          Vec<IngestTrigger>,
    /// Total function definitions loaded.
    pub function_count:           usize,
}

impl TriggerRegistry {
    /// Create a new empty trigger registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load triggers from function definitions.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` if any function's trigger string is invalid or if loading a trigger
    /// type fails.
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
                        event_filter: operation.as_ref().and_then(|op| match op.as_str() {
                            "insert" => Some(crate::EventKind::Insert),
                            "update" => Some(crate::EventKind::Update),
                            "delete" => Some(crate::EventKind::Delete),
                            _ => None,
                        }),
                    };
                    registry.after_mutation_triggers.add(trigger);
                },
                ParsedTrigger::BeforeMutation { mutation_name } => {
                    let trigger = BeforeMutationTrigger {
                        function_name: func.name.clone(),
                        mutation_name,
                    };
                    registry.before_mutation_triggers.push(trigger);
                },
                ParsedTrigger::Http { method, path } => {
                    let route = HttpTriggerRoute {
                        function_name: func.name.clone(),
                        method,
                        path,
                        requires_auth: false,
                    };
                    registry.http_routes.add(route);
                },
                ParsedTrigger::AfterStorage {
                    bucket: _,
                    operation: _,
                } => {
                    return Err(RegistryError {
                        message: "after:storage triggers not yet implemented".to_string(),
                    });
                },
                ParsedTrigger::AfterIngest { source } => {
                    // A `None` source matches every inbound source; a named source
                    // must be a recognised discriminant (fail loud otherwise).
                    let source = match source {
                        None => None,
                        Some(key) => {
                            Some(IngestSource::from_key(&key).ok_or_else(|| RegistryError {
                                message: format!(
                                    "unknown after:ingest source '{key}' (expected \
                                     'email' or 'webhook:<provider>')"
                                ),
                            })?)
                        },
                    };
                    registry.ingest_triggers.push(IngestTrigger {
                        function_name: func.name.clone(),
                        source,
                    });
                },
                ParsedTrigger::Cron { expression } => {
                    let trigger = crate::triggers::cron::CronTrigger {
                        function_name: func.name.clone(),
                        schedule:      expression,
                        timezone:      "UTC".to_string(),
                    };
                    registry.cron_triggers.push(trigger);
                },
            }
        }

        Ok(registry)
    }

    /// Get the number of after:mutation triggers.
    #[must_use]
    pub const fn after_mutation_count(&self) -> usize {
        // This is approximate; TriggerMatcher doesn't expose count
        0
    }

    /// Get the number of before:mutation triggers.
    #[must_use]
    pub const fn before_mutation_count(&self) -> usize {
        self.before_mutation_triggers.len()
    }

    /// Get the number of cron triggers.
    #[must_use]
    pub const fn cron_trigger_count(&self) -> usize {
        self.cron_triggers.len()
    }

    /// Get the number of `after:ingest` triggers.
    #[must_use]
    pub const fn ingest_trigger_count(&self) -> usize {
        self.ingest_triggers.len()
    }

    /// Find all `after:ingest` triggers matching the given inbound message.
    ///
    /// A source-agnostic trigger (`after:ingest`) matches every message; a
    /// source-specific one (`after:ingest:webhook:stripe`) matches only its
    /// source.
    #[must_use]
    pub fn find_ingest_triggers(&self, message: &InboundMessage) -> Vec<IngestTrigger> {
        self.ingest_triggers
            .iter()
            .filter(|trigger| trigger.matches(message))
            .cloned()
            .collect()
    }

    /// Build a [`CronScheduler`] from all registered cron triggers.
    ///
    /// Returns `None` when no cron triggers are registered (the fast path —
    /// avoids spawning a background task when no schedules exist).
    ///
    /// [`CronScheduler`]: crate::triggers::cron::CronScheduler
    #[must_use]
    pub fn cron_scheduler(&self) -> Option<crate::triggers::cron::CronScheduler> {
        if self.cron_triggers.is_empty() {
            None
        } else {
            Some(crate::triggers::cron::CronScheduler::new(self.cron_triggers.clone()))
        }
    }

    /// Get the number of HTTP routes.
    #[must_use]
    pub fn http_route_count(&self) -> usize {
        self.http_routes.routes().len()
    }

    /// Get all HTTP routes.
    #[must_use]
    pub fn http_routes(&self) -> &[HttpTriggerRoute] {
        self.http_routes.routes()
    }

    /// Find an HTTP route by method and path.
    #[must_use]
    pub fn find_http_route(&self, method: &str, path: &str) -> Option<HttpTriggerRoute> {
        self.http_routes.find(method, path)
    }

    /// Get all before:mutation triggers for a specific mutation.
    #[must_use]
    pub fn before_mutation_triggers_for(&self, mutation_name: &str) -> Vec<&BeforeMutationTrigger> {
        self.before_mutation_triggers
            .iter()
            .filter(|t| t.mutation_name == mutation_name)
            .collect()
    }

    /// Check if there are any before:mutation triggers for a mutation.
    #[must_use]
    pub fn has_before_mutation_triggers(&self, mutation_name: &str) -> bool {
        self.before_mutation_triggers.iter().any(|t| t.mutation_name == mutation_name)
    }

    /// Build a [`BeforeMutationChain`](crate::BeforeMutationChain) for the named mutation.
    ///
    /// Returns `None` when no `before:mutation` triggers are registered for this mutation
    /// (the fast path — zero overhead when hooks are absent).
    #[must_use]
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
mod tests;
