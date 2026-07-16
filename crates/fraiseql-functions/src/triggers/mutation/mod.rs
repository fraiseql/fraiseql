//! Mutation triggers: `after:mutation` and `before:mutation`.
//!
//! ## `after:mutation` Triggers
//!
//! Fire asynchronously after a mutation completes (insert, update, or delete).
//! The function receives the old and new row data. Failures do not block the mutation.
//!
//! ## `before:mutation` Triggers
//!
//! Fire synchronously before a mutation executes. The function can:
//! - Return `Proceed(modified_input)` to allow the mutation with possibly modified input
//! - Return `Abort(error_message)` to cancel the mutation
//!
//! Multiple before-hooks execute in declaration order. The first abort short-circuits remaining
//! hooks.
//!
//! **Timeout**: Defaults to 500ms (shorter than general function timeout of 5s)
//! because before-hooks are on the critical mutation path.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::EventPayload;

/// Types of mutations that can trigger events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventKind {
    /// Insert operation.
    Insert,
    /// Update operation.
    Update,
    /// Delete operation.
    Delete,
}

impl EventKind {
    /// Convert to string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            EventKind::Insert => "insert",
            EventKind::Update => "update",
            EventKind::Delete => "delete",
        }
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Entity event with old and new row data.
///
/// Represents a mutation event from the database. Used by the observer pipeline
/// to dispatch to `after:mutation` triggers asynchronously.
///
/// # Dispatch Semantics
///
/// - Fire after mutation completes (mutation response already sent)
/// - Async dispatch: doesn't block mutation response
/// - Failure doesn't affect mutation (error logged only)
/// - Execution order: in declaration order from schema
#[derive(Debug, Clone)]
pub struct EntityEvent {
    /// Entity type (e.g., "User", "Post").
    pub entity:     String,
    /// Kind of mutation.
    pub event_kind: EventKind,
    /// Old row data (None for Insert).
    pub old:        Option<serde_json::Value>,
    /// New row data (None for Delete).
    pub new:        Option<serde_json::Value>,
    /// Timestamp of the event.
    pub timestamp:  chrono::DateTime<chrono::Utc>,
}

/// A single declarative field/transition predicate on an after:mutation trigger
/// (#597).
///
/// The `when` condition that decides whether a function fires, evaluated by the
/// dispatcher against the built payload **before** any runtime spins.
///
/// Deliberately small: a `field` plus exactly one operator — `eq` (state) or
/// `changed_to` (transition). Anything richer stays guest code; this is a dispatch
/// filter, not a rules engine. A list of predicates is a **conjunction** (all must
/// hold); an empty list always fires (back-compat).
///
/// ```jsonc
/// { "field": "status", "changed_to": "approved" }  // UPDATE-only transition test
/// { "field": "kind",   "eq": "standard" }           // state test (INSERT + UPDATE)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerPredicate {
    /// The field (a JSON key in the row image) to test.
    pub field: String,

    /// **State test**: the field currently equals this value. Evaluated on the
    /// after-image (INSERT/UPDATE) or, for a DELETE, the pre-image. An absent field
    /// never equals a value (missing ⇒ `false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<serde_json::Value>,

    /// **Transition test** (UPDATE-only): the field *changed to* this value —
    /// `old.field != v && new.field == v`. A DELETE (no after-image) never matches;
    /// `changed_to` on a non-`update` trigger is a load error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_to: Option<serde_json::Value>,
}

impl TriggerPredicate {
    /// Evaluate this predicate against a row's `old`/`new` images.
    ///
    /// - `eq`: the current image (`new`, or `old` on a DELETE) has `field == v`. Missing field ⇒
    ///   `false`.
    /// - `changed_to`: `old.field != v && new.field == v`.
    ///
    /// A predicate with neither operator set (rejected at load) never matches.
    #[must_use]
    pub fn matches(
        &self,
        old: Option<&serde_json::Value>,
        new: Option<&serde_json::Value>,
    ) -> bool {
        if let Some(value) = &self.eq {
            // Prefer the after-image; fall back to the pre-image for a DELETE.
            let image = new.or(old);
            return image.and_then(|row| row.get(&self.field)) == Some(value);
        }
        if let Some(value) = &self.changed_to {
            let before = old.and_then(|row| row.get(&self.field));
            let after = new.and_then(|row| row.get(&self.field));
            return before != Some(value) && after == Some(value);
        }
        false
    }

    /// Validate the predicate at load time against the trigger's `operation`
    /// (`insert`/`update`/`delete`, or `None` for all).
    ///
    /// # Errors
    ///
    /// - Neither `eq` nor `changed_to` set, or both set (exactly one operator).
    /// - `changed_to` on a non-`update` trigger (a transition needs a before + after).
    pub fn validate(&self, operation: Option<&str>) -> Result<(), String> {
        match (&self.eq, &self.changed_to) {
            (Some(_), Some(_)) => Err(format!(
                "predicate on field `{}` sets both `eq` and `changed_to` — use exactly one",
                self.field
            )),
            (None, None) => Err(format!(
                "predicate on field `{}` sets neither `eq` nor `changed_to`",
                self.field
            )),
            (None, Some(_)) if operation != Some("update") => Err(format!(
                "predicate on field `{}` uses `changed_to`, which is UPDATE-only, but the \
                 trigger operation is `{}`",
                self.field,
                operation.unwrap_or("all")
            )),
            _ => Ok(()),
        }
    }
}

/// Whether *all* predicates in a conjunction hold for a row's `old`/`new` images.
/// An empty conjunction always holds (back-compat: no `when` ⇒ always fire).
#[must_use]
pub fn predicates_match(
    predicates: &[TriggerPredicate],
    old: Option<&serde_json::Value>,
    new: Option<&serde_json::Value>,
) -> bool {
    predicates.iter().all(|predicate| predicate.matches(old, new))
}

/// Trigger that fires after a mutation completes.
///
/// When a mutation completes, the observer pipeline emits an `EntityEvent`.
/// If an `AfterMutationTrigger` matches the entity type and event kind,
/// the corresponding function is invoked asynchronously without blocking
/// the mutation response.
///
/// # Matching
///
/// - Must match `entity_type` exactly
/// - If `event_filter` is `None`, matches all event kinds (Insert/Update/Delete)
/// - If `event_filter` is `Some`, matches only that specific event kind
/// - `predicates` (the `when` clause) must all hold on the row images (#597)
///
/// # Dispatch
///
/// - Invoked in declaration order from `schema.compiled.json`
/// - Spawned as an async task (mutation response returns immediately)
/// - Function execution timeout: 5s default (can be overridden per function)
/// - Failure doesn't affect mutation (error logged to tracing subscriber)
#[derive(Debug, Clone)]
pub struct AfterMutationTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Entity type to trigger on (e.g., "User").
    pub entity_type:   String,
    /// Optional filter on event kind (None = all).
    pub event_filter:  Option<EventKind>,
    /// The `when` conjunction (#597); empty ⇒ always fire (back-compat).
    pub predicates:    Vec<TriggerPredicate>,
}

impl AfterMutationTrigger {
    /// Check if this trigger matches the given entity and event kind. Does **not**
    /// evaluate the `when` predicates — the dispatcher applies
    /// [`predicates_hold`](Self::predicates_hold) against the payload afterwards.
    #[must_use]
    pub fn matches(&self, entity: &str, event_kind: EventKind) -> bool {
        self.entity_type == entity && self.event_filter.is_none_or(|filter| filter == event_kind)
    }

    /// Whether this trigger's `when` predicates all hold for the event's row images
    /// (#597). Evaluated by the dispatcher before spawning the runtime — a `false`
    /// result means the function does not fire (no dispatch record at all).
    #[must_use]
    pub fn predicates_hold(&self, event: &EntityEvent) -> bool {
        predicates_match(&self.predicates, event.old.as_ref(), event.new.as_ref())
    }

    /// Build an `EventPayload` from an entity event.
    #[must_use]
    pub fn build_payload(&self, event: &EntityEvent) -> EventPayload {
        EventPayload {
            trigger_type: format!("after:mutation:{}", self.function_name),
            entity:       event.entity.clone(),
            event_kind:   event.event_kind.to_string(),
            data:         serde_json::json!({
                "event_kind": event.event_kind.as_str(),
                "old": event.old,
                "new": event.new,
            }),
            timestamp:    event.timestamp,
        }
    }
}

/// Result of a before-mutation trigger execution.
///
/// # Semantics
///
/// - `Proceed`: Allows mutation to continue with the provided input
///   - Input may be modified from original
///   - Passed to next trigger in chain (if any)
/// - `Abort`: Prevents mutation from executing
///   - Returns error to client immediately
///   - Short-circuits remaining triggers in chain
///   - Side-effects from aborted triggers are NOT rolled back
///
/// # Important: Side-Effects Not Rolled Back
///
/// If a `before:mutation` trigger abort is triggered, any side-effects
/// (HTTP calls, storage writes, logs) from earlier triggers in the chain
/// are NOT rolled back. Only the mutation itself is prevented.
///
/// This is by design: function side-effects are intended to be independent
/// of mutation success. For example, if a function logs an audit entry and
/// then a later trigger aborts, the audit entry remains.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BeforeMutationResult {
    /// Proceed with the mutation using the provided (possibly modified) input.
    Proceed(serde_json::Value),
    /// Abort the mutation with an error message.
    Abort(String),
}

/// Trigger that fires before a mutation executes.
#[derive(Debug, Clone)]
pub struct BeforeMutationTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Name of the mutation to trigger on (e.g., "createUser").
    pub mutation_name: String,
}

impl BeforeMutationTrigger {
    /// Check if this trigger matches the given mutation.
    #[must_use]
    pub fn matches(&self, mutation: &str) -> bool {
        self.mutation_name == mutation
    }
}

/// Chain of before-mutation triggers for a single mutation.
///
/// Executes multiple `before:mutation` triggers in declaration order.
/// Each trigger can modify the input and pass it to the next trigger,
/// or abort the mutation by returning an error.
///
/// # Execution Semantics
///
/// - Synchronous: blocks the mutation (execution is on the hot path)
/// - Sequential: triggers execute in declaration order
/// - Propagating: each trigger receives the modified input from previous trigger
/// - Short-circuit: first abort stops the chain immediately
/// - Default timeout: 500ms per trigger (shorter than general 5s default)
/// - Side-effects: any side-effects from aborted triggers are NOT rolled back
///
/// # Example
///
/// ```ignore
/// let chain = BeforeMutationChain {
///     triggers: vec![
///         validateInput,  // checks required fields
///         checkDuplicates, // checks uniqueness
///         auditLog,       // logs the attempt
///     ]
/// };
///
/// let result = chain.execute(input, &observer).await?;
/// match result {
///     Proceed(modified) => { /* mutation continues */ }
///     Abort(error) => { /* mutation cancelled */ }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BeforeMutationChain {
    /// Triggers in declaration order.
    pub triggers: Vec<BeforeMutationTrigger>,
}

impl BeforeMutationChain {
    /// Execute the before-mutation chain with the given input.
    ///
    /// Runs all triggers in declaration order. Each trigger receives the
    /// (possibly modified) output of the previous trigger as its input.
    /// The first `Abort` short-circuits the chain.
    ///
    /// # Convention for function return values
    ///
    /// Functions signal their intent via the returned JSON object:
    /// - `{"abort": "message"}` → abort the mutation with `message`
    /// - `{"input": {...}}` → proceed with modified input
    /// - Any other value (or `null`) → proceed with the input unchanged
    ///
    /// # Errors
    ///
    /// Returns `Err` if a trigger's function name is not found in `modules`, or if
    /// function execution itself returns an error.
    pub async fn execute<H>(
        &self,
        input: serde_json::Value,
        modules: &std::collections::HashMap<String, crate::types::FunctionModule>,
        observer: &crate::observer::FunctionObserver,
        host: &H,
        limits: crate::types::ResourceLimits,
    ) -> fraiseql_error::Result<BeforeMutationResult>
    where
        H: crate::HostContext + ?Sized,
    {
        let mut current = input;
        for trigger in &self.triggers {
            let module = modules.get(&trigger.function_name).ok_or_else(|| {
                fraiseql_error::FraiseQLError::Validation {
                    message: format!(
                        "before:mutation function '{}' not found in module registry",
                        trigger.function_name,
                    ),
                    path:    None,
                }
            })?;

            let payload = crate::types::EventPayload {
                trigger_type: format!("before:mutation:{}", trigger.mutation_name),
                entity:       trigger.mutation_name.clone(),
                event_kind:   "before".to_string(),
                data:         current.clone(),
                timestamp:    chrono::Utc::now(),
            };

            let result = observer.invoke(module, payload, host, limits.clone()).await?;

            match result.value {
                Some(ref v) if v.get("abort").is_some() => {
                    let msg = v["abort"]
                        .as_str()
                        .unwrap_or("Aborted by before:mutation trigger")
                        .to_string();
                    return Ok(BeforeMutationResult::Abort(msg));
                },
                Some(ref v) if v.get("input").is_some() => {
                    current = v["input"].clone();
                },
                _ => {},
            }
        }
        Ok(BeforeMutationResult::Proceed(current))
    }
}

/// Matcher for efficiently finding triggers by (`entity_type`, `event_kind`).
///
/// Uses a nested `HashMap` for O(1) lookup:
/// - `entity_type` → `event_kind` → `Vec<AfterMutationTrigger>`
/// - When `event_kind` is None (matches all), stored separately for fallback
///
/// # Integration with `FunctionObserver`
///
/// When the `FunctionObserver` receives an `EntityEvent` from the mutation pipeline,
/// it calls `find()` to get all matching `AfterMutationTrigger`s. For each matching
/// trigger, the observer spawns an async task to invoke the function without blocking
/// the mutation response. Task completion is tracked to prevent leaks on shutdown.
///
/// # Example
///
/// ```ignore
/// let mut matcher = TriggerMatcher::new();
/// matcher.add(AfterMutationTrigger {
///     function_name: "onUserCreated".to_string(),
///     entity_type: "User".to_string(),
///     event_filter: Some(EventKind::Insert),
/// });
///
/// // Later, when a User insert occurs:
/// let triggers = matcher.find("User", EventKind::Insert);
/// for trigger in triggers {
///     // Spawn async task to invoke function
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TriggerMatcher {
    /// Map of `entity_type` → `event_kind` → triggers
    specific:  HashMap<String, HashMap<String, Vec<AfterMutationTrigger>>>,
    /// Map of `entity_type` → triggers that match all event kinds
    all_kinds: HashMap<String, Vec<AfterMutationTrigger>>,
}

impl TriggerMatcher {
    /// Create a new empty trigger matcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            specific:  HashMap::new(),
            all_kinds: HashMap::new(),
        }
    }

    /// Add a trigger to the matcher.
    pub fn add(&mut self, trigger: AfterMutationTrigger) {
        match trigger.event_filter {
            Some(event_kind) => {
                self.specific
                    .entry(trigger.entity_type.clone())
                    .or_default()
                    .entry(event_kind.as_str().to_string())
                    .or_default()
                    .push(trigger);
            },
            None => {
                self.all_kinds.entry(trigger.entity_type.clone()).or_default().push(trigger);
            },
        }
    }

    /// Find all triggers matching the given entity and event kind.
    #[must_use]
    pub fn find(&self, entity: &str, event_kind: EventKind) -> Vec<AfterMutationTrigger> {
        let event_str = event_kind.as_str();
        let mut result = Vec::new();

        // Get specific triggers for this event kind
        if let Some(entity_map) = self.specific.get(entity) {
            if let Some(triggers) = entity_map.get(event_str) {
                result.extend(triggers.clone());
            }
        }

        // Get all-kinds triggers for this entity
        if let Some(triggers) = self.all_kinds.get(entity) {
            result.extend(triggers.clone());
        }

        result
    }
}

impl Default for TriggerMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
