//! An action as someone wrote it, checked against what the runtime does with it.
//!
//! [`ActionConfig`] is lenient when it reads a stored row: a key it does not hold is
//! ignored, so a row written by an earlier release still loads. That leniency is wrong
//! where an action is written. A misspelt key, or one the type holds and the dispatcher
//! never uses, would be accepted and then do nothing. The compiler, for an observer the
//! schema declares, and the admin API, for one it creates or updates, both ask here.

use super::ActionConfig;
use crate::error::{ObserverError, Result};

/// Keys an action type holds and its dispatcher does not use, by action type.
///
/// An email is sent to `to` with `subject`: a recipient or subject template, or a reply
/// address, is read into the action and dropped. An action written with one would send
/// something else, or fail at delivery for want of `to`.
const UNDISPATCHED_KEYS: &[(&str, &[&str])] =
    &[("email", &["to_template", "subject_template", "reply_to"])];

/// Keys an action type holds and does not serialize when empty, by action type.
///
/// The keys an action holds are read from its serialization, and an empty one of these is
/// not there: written as an empty map, it is still a key the action holds.
const OMITTED_WHEN_EMPTY: &[(&str, &[&str])] = &[("webhook", &["headers_env"])];

impl ActionConfig {
    /// Read an action as written, refusing one the runtime would not run as written.
    ///
    /// Refused: a `type` the runtime does not dispatch, a key the action type does not
    /// hold, a key it holds and its dispatcher does not use, a webhook or Slack action
    /// naming two destinations, and whatever [`validate`](Self::validate) refuses.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidActionConfig`] naming the key at fault.
    pub fn from_authored(action: &serde_json::Value) -> Result<Self> {
        let invalid = |reason: String| ObserverError::InvalidActionConfig { reason };
        let Some(authored) = action.as_object() else {
            return Err(invalid("an action is an object with a `type`".to_string()));
        };
        let read: Self = serde_json::from_value(action.clone())
            .map_err(|e| invalid(format!("not an action the runtime dispatches: {e}")))?;
        read.validate()?;
        if let Some((written, from_env)) = read.two_destinations() {
            return Err(invalid(format!(
                "a `{}` action names two destinations, and the runtime would send to \
                 `{written}` and never read `{from_env}`: set one of `{written}` / \
                 `{from_env}`, not both",
                read.action_type()
            )));
        }

        let held = serde_json::to_value(&read)
            .map_err(|e| invalid(format!("the action does not serialize: {e}")))?;
        let mut held: Vec<&str> = held
            .as_object()
            .map_or_default(|keys| keys.keys().map(String::as_str).collect());
        let omitted: Vec<&str> = OMITTED_WHEN_EMPTY
            .iter()
            .filter(|(action_type, _)| *action_type == read.action_type())
            .flat_map(|(_, keys)| keys.iter().copied())
            .filter(|key| !held.contains(key))
            .collect();
        held.extend(omitted);
        if let Some(unread) = authored.keys().find(|key| !held.contains(&key.as_str())) {
            return Err(invalid(format!(
                "`{unread}` is not a key of a `{}` action, and the runtime would ignore it. \
                 Its keys are: {}",
                read.action_type(),
                held.join(", ")
            )));
        }
        if let Some(undispatched) = UNDISPATCHED_KEYS
            .iter()
            .filter(|(action_type, _)| *action_type == read.action_type())
            .flat_map(|(_, keys)| keys.iter())
            .find(|key| authored.get(**key).is_some_and(|value| !value.is_null()))
        {
            return Err(invalid(format!(
                "`{undispatched}` is a key the `{}` action accepts and its dispatcher does not \
                 use: the observer would not do what the key says",
                read.action_type()
            )));
        }
        Ok(read)
    }
}

impl ActionConfig {
    /// The two keys of a destination named both ways, written URL first.
    ///
    /// The runtime sends to the written URL and reads the environment variable only when
    /// there is none, so an action naming both delivers to one of them and says nothing.
    /// A row stored by an earlier release with both still loads: only an action as
    /// written is refused.
    const fn two_destinations(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Webhook {
                url: Some(_),
                url_env: Some(_),
                ..
            } => Some(("url", "url_env")),
            Self::Slack {
                webhook_url: Some(_),
                webhook_url_env: Some(_),
                ..
            } => Some(("webhook_url", "webhook_url_env")),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
