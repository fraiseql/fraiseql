//! The environment variables a declared observer's actions name, checked at boot.
//!
//! A webhook or Slack action can take its URL, its signing secret and its header values
//! from the environment, and dispatch reads them at each delivery. The environment of a
//! running process does not change, so a variable that is unset or empty at boot is
//! unset or empty at every delivery: each one would be dead-lettered.

use fraiseql_core::schema::ObserverDefinition;
use fraiseql_guard::deployment::Deployment;
use fraiseql_observers::ActionConfig;

/// One variable a declared action names that holds no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingVariable {
    /// The observer that declares the action.
    pub observer: String,
    /// The action's position in the observer's `actions`, from 0.
    pub action:   usize,
    /// The key naming the variable, as the declaration spells it (`headers_env.<header>`
    /// for a header).
    pub key:      String,
    /// The variable's name.
    pub variable: String,
}

impl std::fmt::Display for MissingVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "observer `{}`, action {}: `{}` names {}, which is not set or is empty",
            self.observer, self.action, self.key, self.variable
        )
    }
}

/// Every variable the declared observers' actions read at delivery that holds no value
/// now, in declaration order.
///
/// An action whose declaration does not parse is left to the runtime, which refuses it
/// when it loads the observer.
#[must_use]
pub fn missing_variables(declared: &[ObserverDefinition]) -> Vec<MissingVariable> {
    let mut missing = Vec::new();
    for observer in declared {
        for (index, action) in observer.actions.iter().enumerate() {
            let Ok(action) = serde_json::from_value::<ActionConfig>(action.clone()) else {
                continue;
            };
            for (key, variable) in variables_read_at_delivery(&action) {
                if !is_set(&variable) {
                    missing.push(MissingVariable {
                        observer: observer.name.clone(),
                        action: index,
                        key,
                        variable,
                    });
                }
            }
        }
    }
    missing
}

/// The `(key, variable)` pairs dispatch reads for `action`. A literal URL is used in place
/// of `url_env`, so the variable is read only when no literal is declared.
fn variables_read_at_delivery(action: &ActionConfig) -> Vec<(String, String)> {
    let mut read = Vec::new();
    match action {
        ActionConfig::Webhook {
            url,
            url_env,
            headers_env,
            signing_secret_env,
            ..
        } => {
            if let (None, Some(variable)) = (url, url_env) {
                read.push(("url_env".to_string(), variable.clone()));
            }
            if let Some(variable) = signing_secret_env {
                read.push(("signing_secret_env".to_string(), variable.clone()));
            }
            let mut headers: Vec<_> = headers_env.iter().collect();
            headers.sort();
            for (header, variable) in headers {
                read.push((format!("headers_env.{header}"), variable.clone()));
            }
        },
        ActionConfig::Slack {
            webhook_url: None,
            webhook_url_env: Some(variable),
            ..
        } => {
            read.push(("webhook_url_env".to_string(), variable.clone()));
        },
        _ => {},
    }
    read
}

fn is_set(variable: &str) -> bool {
    std::env::var_os(variable).is_some_and(|value| !value.is_empty())
}

/// Check the declared observers' variables before anything is stored or started.
///
/// # Errors
///
/// In production, a `ConfigError` naming every missing variable: the observer, the
/// action, the key and the variable, never a value. In development each one is logged
/// at `WARN` and the boot continues; its deliveries are dead-lettered.
pub fn check_declared_variables(
    declared: &[ObserverDefinition],
    deployment: Deployment,
) -> crate::Result<()> {
    let missing = missing_variables(declared);
    if missing.is_empty() {
        return Ok(());
    }
    if deployment.is_production() {
        let list: Vec<String> = missing.iter().map(|m| format!("  - {m}")).collect();
        return Err(crate::ServerError::ConfigError(format!(
            "a declared observer reads an environment variable that is not set or is \
             empty, so every delivery of its action would be dead-lettered. Set each of \
             these before starting the server:\n{}",
            list.join("\n")
        )));
    }
    for m in &missing {
        tracing::warn!(
            observer = %m.observer,
            action = m.action,
            key = %m.key,
            variable = %m.variable,
            "A declared observer reads an environment variable that is not set or is empty; \
             its deliveries will be dead-lettered"
        );
    }
    Ok(())
}
