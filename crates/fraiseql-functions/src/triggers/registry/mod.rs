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
    /// Request-serving: `request:query` (#1329).
    ///
    /// The one trigger that is not an event. Every other kind names something that
    /// *happened* — a write committed, a schedule fired, a message arrived — and the
    /// dispatcher decides from the event which functions to run. This one names a
    /// capability: the function answers a GraphQL root query field, and the binding
    /// lives on the query, which declares `function = "<name>"` in place of
    /// `sql_source`.
    ///
    /// # Why the trigger does not name the query
    ///
    /// It would be the second copy of one fact. The engine reads the binding off the
    /// compiled `QueryDefinition` — it has to, since `fraiseql-core` knows nothing
    /// about functions — so a query named here as well could disagree with the query
    /// that actually resolves to this function, and one of the two spellings would
    /// silently win. Naming the kind and leaving the binding to the query keeps one
    /// fact in one place, and the pair is still checked in both directions by
    /// [`validate_query_bindings`](TriggerRegistry::validate_query_bindings).
    RequestQuery,
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
            // Exactly two parts, deliberately. `request:query:quotePreview` is the
            // natural mistake — every other trigger takes a selector — and it must
            // fail here rather than parse to a bare `RequestQuery` that ignores the
            // name the author wrote, which would leave the query bound to whatever
            // the `function` key says while the declaration reads otherwise.
            Some("request") if parts.len() == 2 && parts[1] == "query" => {
                Ok(ParsedTrigger::RequestQuery)
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
            ParsedTrigger::RequestQuery => "request:query",
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

    /// Check if this is a `request:query` trigger (#1329).
    #[must_use]
    pub const fn is_request_query(&self) -> bool {
        matches!(self, ParsedTrigger::RequestQuery)
    }
}

/// One query's declared binding to a request-serving function (#1329).
///
/// Borrowed rather than owned so both call sites can build the slice straight from
/// the compiled schema they already hold, without cloning names to check them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryFunctionBinding<'a> {
    /// The root query field that declares `function = "<name>"`.
    pub query:    &'a str,
    /// The function name it declares.
    pub function: &'a str,
}

/// Render a de-duplicated, sorted name list for a diagnostic, or say there are none.
fn name_list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<&str> = names.collect();
    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        "(none declared)".to_string()
    } else {
        names.join(", ")
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
    /// Names of the `request:query` functions (#1329), in declaration order.
    ///
    /// Not a matcher, because nothing matches on them: a request-serving function
    /// is reached from the compiled query that names it, never from an event. The
    /// list exists so a dispatcher can answer "may this name be invoked to serve a
    /// read?" without re-parsing every trigger string.
    pub request_query_functions:  Vec<String>,
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

    /// Validate a set of function definitions without keeping the registry.
    ///
    /// The **one** definition of "is this set of declarations loadable" — trigger
    /// grammar, `when` predicates against the trigger's operation (#597), event-kind
    /// tokens (#842), the `after:ingest` source selector, and the `http:` /
    /// `after:storage` refusals (#871). The compiler calls it so a bad declaration
    /// fails `fraiseql compile`; the server's schema loader calls it so a
    /// hand-written or stale artifact still fails at boot rather than dispatching
    /// something nobody validated.
    ///
    /// Two *call sites*, one rule. The loader used to carry its own
    /// `VALID_TRIGGER_PREFIXES` list instead, and it had already fallen behind by two
    /// trigger kinds: `after:capture:` (#366) and `after:ingest:` parse and dispatch
    /// here, and were refused there (#1325).
    ///
    /// # Errors
    ///
    /// Returns the first [`RegistryError`] any definition produces, naming the
    /// function.
    pub fn validate_definitions(functions: &[FunctionDefinition]) -> Result<(), RegistryError> {
        Self::load_from_definitions(functions).map(|_| ())
    }

    /// Validate the pairing between function-backed queries and `request:query`
    /// functions — in **both** directions (#1329).
    ///
    /// The **one** definition of "does this schema's function-backed surface hold
    /// together". Two call sites, as with
    /// [`validate_definitions`](Self::validate_definitions): the compiler, so a bad
    /// declaration fails `fraiseql compile`; and the server's schema loader, because
    /// a compiled schema is an input the server does not produce and a hand-written
    /// or stale artifact must still fail at boot.
    ///
    /// Three rules, and the third is the one that is easy to leave out:
    ///
    /// 1. a query's `function` names a **declared** function;
    /// 2. that function's trigger is `request:query` — an `after:mutation` handler bound to a read
    ///    would be invoked with the wrong contract and would never fire for the reason it was
    ///    written;
    /// 3. every `request:query` function is named by **at least one** query. This is #871's rule
    ///    applied to the new kind: a declared function that can never serve is a misconfiguration,
    ///    and this one fails silently in the worst way — the function loads, the server boots, and
    ///    nothing ever calls it.
    ///
    /// Reports every failing pair rather than the first, so one compile names the
    /// whole list.
    ///
    /// # Errors
    ///
    /// Returns a [`RegistryError`] naming each query or function that does not pair.
    pub fn validate_query_bindings(
        bindings: &[QueryFunctionBinding<'_>],
        functions: &[FunctionDefinition],
    ) -> Result<(), RegistryError> {
        let mut failures: Vec<String> = Vec::new();

        for binding in bindings {
            match functions.iter().find(|f| f.name == binding.function) {
                None => failures.push(format!(
                    "  query `{}`: function = `{}` names no declared function. Declared \
                     request-serving functions are: {}",
                    binding.query,
                    binding.function,
                    name_list(Self::request_query_names(functions)),
                )),
                Some(declared) if !Self::serves_requests(declared) => failures.push(format!(
                    "  query `{}`: function = `{}` names a function whose trigger is `{}`. A \
                     query may only be backed by a function declared `request:query` — an \
                     event-triggered function is invoked with an event payload, not with \
                     this query's arguments, so it would never answer the read it is bound \
                     to",
                    binding.query, binding.function, declared.trigger,
                )),
                Some(_) => {},
            }
        }

        for definition in functions.iter().filter(|f| Self::serves_requests(f)) {
            if !bindings.iter().any(|b| b.function == definition.name) {
                failures.push(format!(
                    "  function `{}`: trigger `request:query` is served by the query that \
                     declares `function = \"{}\"`, and no query does. Declare it on a \
                     root query field, or remove the function — as declared it loads, the \
                     server boots, and nothing ever invokes it",
                    definition.name, definition.name,
                ));
            }
        }

        if failures.is_empty() {
            return Ok(());
        }
        Err(RegistryError {
            message: format!(
                "{} function-backed query binding(s) do not pair:\n{}",
                failures.len(),
                failures.join("\n")
            ),
        })
    }

    /// Whether this definition declares the request-serving trigger.
    ///
    /// A trigger that does not parse is not one: it is refused by
    /// [`validate_definitions`](Self::validate_definitions), whose diagnosis is the
    /// useful one, so treating it as "not request-serving" here cannot mask it.
    fn serves_requests(definition: &FunctionDefinition) -> bool {
        ParsedTrigger::parse(&definition.trigger).is_ok_and(|t| t.is_request_query())
    }

    /// The names of the `request:query` functions, for a diagnostic.
    fn request_query_names(functions: &[FunctionDefinition]) -> impl Iterator<Item = &str> {
        functions.iter().filter(|f| Self::serves_requests(f)).map(|f| f.name.as_str())
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
            // Name the function in every trigger diagnosis. The parse error knows
            // only the string; a compiled schema with twenty functions needs to say
            // which one.
            let parsed = ParsedTrigger::parse(&func.trigger).map_err(|error| RegistryError {
                message: format!("function `{}`: {}", func.name, error.message),
            })?;

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
                    // #871 item 2: `http_routes` has no consumer — no server code
                    // mounts the matcher. A declared function that can never serve is
                    // a misconfiguration; fail loud like `after:storage`.
                    //
                    // The remedy changed in #1329. It used to be "invoke it via
                    // POST /functions/v1/<name>", which was library-only — the stock
                    // binary never mounted that route — so the advice named a door
                    // almost nobody had. That route is retired; a function that
                    // answers a request now does it as a typed root query field, where
                    // it is introspectable, argument-checked and cache-governed.
                    return Err(RegistryError {
                        message: format!(
                            "function `{}` trigger `{}`: http triggers are not mounted by the \
                             server (the declared route would never serve). To answer a request \
                             from a function, declare it `request:query` and point a root query \
                             field at it with `function = \"{}\"` (#1329)",
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
                ParsedTrigger::RequestQuery => {
                    // The settings that describe a *dispatch* are refused here, where
                    // both call sites see them. None of them is inert decoration:
                    // `run_as` is an authority ceiling, and a security setting that is
                    // accepted and never applied is the failure this seam exists to
                    // remove (#1329). A request-serving function reads as its caller
                    // through the bridge, so there is no background identity to grant.
                    for (setting, declared, why) in [
                        (
                            "run_as",
                            func.run_as.is_some(),
                            "a request:query function reads as its caller, through the read                              bridge — there is no background identity for a ceiling to bound,                              and a ceiling that bounds nothing reads as a granted authority",
                        ),
                        (
                            "when",
                            !func.when.is_empty(),
                            "a `when` predicate is evaluated against a row image, and a                              request-serving invocation has none: it is handed the query's                              arguments",
                        ),
                        (
                            "re_runnable",
                            func.re_runnable,
                            "it opts out of durable dispatch, and a request-serving invocation                              is not dispatched — it answers a client that is waiting",
                        ),
                        (
                            "retry",
                            func.retry.is_some(),
                            "a retry policy governs durable dispatch; a failed read is reported                              to the caller, who decides whether to ask again",
                        ),
                    ] {
                        if declared {
                            return Err(RegistryError {
                                message: format!(
                                    "function `{}` trigger `request:query`: `{setting}` cannot be                                      declared on a request-serving function — {why}",
                                    func.name
                                ),
                            });
                        }
                    }
                    // No matcher and no schedule: the compiled query that declares
                    // `function = "<name>"` is what reaches this function, and that
                    // pairing is checked by `validate_query_bindings`, which needs
                    // the schema this call does not have.
                    registry.request_query_functions.push(func.name.clone());
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
