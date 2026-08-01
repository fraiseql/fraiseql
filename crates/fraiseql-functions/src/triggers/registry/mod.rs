//! Trigger registry: Central coordinator for all trigger types and lifecycle.
//!
//! The `TriggerRegistry` loads function definitions from a schema, parses trigger strings,
//! builds internal structures (matchers, chains, schedulers), and manages startup/shutdown.

use serde::{Deserialize, Serialize};

use crate::{
    FunctionDefinition,
    triggers::{
        ingest::{InboundMessage, IngestTrigger},
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
    /// After capture: `after:capture:<entity_type>[:<operation>]` (#366) — fires on
    /// an **externally-captured** write to a `@subscribable` table (a third-party
    /// daemon / `psql` INSERT), driven from the change-log reader, distinct from
    /// `after:mutation` (which fires on FraiseQL's own committed mutations).
    AfterCapture {
        /// Entity type name (must be a `@subscribable`/captured entity).
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
            Some("after") if parts.len() >= 3 && parts[1] == "capture" => {
                let entity_type = parts[2].to_string();
                let operation = if parts.len() > 3 {
                    Some(parts[3].to_string())
                } else {
                    None
                };
                Ok(ParsedTrigger::AfterCapture {
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
            ParsedTrigger::AfterCapture { .. } => "after:capture",
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
    /// After-capture triggers (#366) — fire on externally-captured writes, indexed
    /// by entity and operation. Structurally identical to after:mutation triggers
    /// (entity + operation + `when` predicates), kept in a separate matcher because
    /// they fire from the change-log reader, not the mutation route.
    pub after_capture_triggers:   TriggerMatcher,
    /// Before-mutation triggers indexed by mutation name.
    pub before_mutation_triggers: Vec<BeforeMutationTrigger>,
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

    /// Resolve an `after:mutation` / `after:capture` operation token into an
    /// event filter, failing loud on anything unrecognized (#842).
    ///
    /// `None` and the documented `*` wildcard mean "every event kind"; the only
    /// narrowing tokens are exactly `insert` / `update` / `delete`. Anything
    /// else (`created`, `INSERT`, a typo) used to collapse to `None` via
    /// `and_then`, silently widening the trigger to all kinds — a welcome-email
    /// function declared for `:created` also fired on every delete.
    fn resolve_event_filter(
        function_name: &str,
        trigger: &str,
        operation: Option<&str>,
    ) -> Result<Option<crate::EventKind>, RegistryError> {
        match operation {
            None | Some("*") => Ok(None),
            Some("insert") => Ok(Some(crate::EventKind::Insert)),
            Some("update") => Ok(Some(crate::EventKind::Update)),
            Some("delete") => Ok(Some(crate::EventKind::Delete)),
            Some(other) => Err(RegistryError {
                message: format!(
                    "function `{function_name}` trigger `{trigger}`: unknown operation \
                     `{other}` (expected `insert`, `update`, `delete`, or `*` for all)"
                ),
            }),
        }
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
                    // #842: an unrecognized token is a load error, never a
                    // silent widening to every event kind.
                    let event_filter = Self::resolve_event_filter(
                        &func.name,
                        &func.trigger,
                        operation.as_deref(),
                    )?;
                    // #597: validate each `when` predicate against the trigger's
                    // operation at load — `changed_to` is UPDATE-only, exactly one
                    // operator per predicate, unknown keys already rejected by
                    // `deny_unknown_fields` on `TriggerPredicate`. The `*`
                    // wildcard means "all kinds", like the token-less form.
                    let canonical_op = operation.as_deref().filter(|&op| op != "*");
                    for predicate in &func.when {
                        predicate.validate(canonical_op).map_err(|message| RegistryError {
                            message: format!(
                                "function `{}` trigger `{}`: {message}",
                                func.name, func.trigger
                            ),
                        })?;
                    }
                    let trigger = AfterMutationTrigger {
                        function_name: func.name.clone(),
                        entity_type,
                        event_filter,
                        predicates: func.when.clone(),
                    };
                    registry.after_mutation_triggers.add(trigger);
                },
                ParsedTrigger::AfterCapture {
                    entity_type,
                    operation,
                } => {
                    // #842: same loud rejection as after:mutation.
                    let event_filter = Self::resolve_event_filter(
                        &func.name,
                        &func.trigger,
                        operation.as_deref(),
                    )?;
                    // #366: same `when` validation as after:mutation.
                    let canonical_op = operation.as_deref().filter(|&op| op != "*");
                    for predicate in &func.when {
                        predicate.validate(canonical_op).map_err(|message| RegistryError {
                            message: format!(
                                "function `{}` trigger `{}`: {message}",
                                func.name, func.trigger
                            ),
                        })?;
                    }
                    let trigger = AfterMutationTrigger {
                        function_name: func.name.clone(),
                        entity_type,
                        event_filter,
                        predicates: func.when.clone(),
                    };
                    registry.after_capture_triggers.add(trigger);
                },
                ParsedTrigger::BeforeMutation { mutation_name } => {
                    let trigger = BeforeMutationTrigger {
                        function_name: func.name.clone(),
                        mutation_name,
                    };
                    registry.before_mutation_triggers.push(trigger);
                },
                ParsedTrigger::Http { .. } => {
                    // #871 item 2: `http_routes` has no consumer — no server
                    // code mounts the matcher, and `POST /functions/v1/{name}`
                    // dispatches by function name, ignoring the trigger. A
                    // declared function that can never serve is a
                    // misconfiguration; fail loud like `after:storage` until
                    // routes are actually mounted.
                    return Err(RegistryError {
                        message: format!(
                            "function `{}` trigger `{}`: http triggers are not mounted by the \
                             server (the declared route would never serve); invoke the function \
                             via POST /functions/v1/{} instead",
                            func.name, func.trigger, func.name
                        ),
                    });
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
                    // must be a recognised selector (fail loud otherwise).
                    let source = match source {
                        None => None,
                        Some(key) => Some(
                            crate::triggers::ingest::IngestSelector::from_key(&key).ok_or_else(
                                || RegistryError {
                                    message: format!(
                                        "unknown after:ingest source '{key}' (expected \
                                         'email', 'email:<mailbox>' or 'webhook:<provider>')"
                                    ),
                                },
                            )?,
                        ),
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
