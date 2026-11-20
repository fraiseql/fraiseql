//! Declared observers, compiled into the shape the observer runtime reads.
//!
//! The server stores each compiled observer in `core.tb_observer` at boot and the runtime
//! loads it from there, reading an action through [`fraiseql_observers::ActionConfig`] and
//! the retry policy through [`fraiseql_observers::RetryConfig`]. A declaration is checked
//! against those types here, so what compiles is what delivers. An action goes through
//! [`ActionConfig::from_authored`], the check the admin API applies to the actions it is
//! sent.

use anyhow::{Context, Result, bail};
use fraiseql_core::schema::{CompiledSchema, ConditionRead, ObserverDefinition};
use fraiseql_observers::{
    ActionConfig, ConditionParser, FieldRead, body_template_placeholders, is_secret_header_name,
};
use tracing::warn;

use crate::schema::intermediate::IntermediateObserver;

/// Compile `declared` against `schema`, which holds every type and the `[observers]`
/// section.
///
/// # Errors
///
/// Returns the first declaration the runtime could not run as written, naming the observer
/// and the key at fault.
pub(super) fn compile(
    declared: &[IntermediateObserver],
    schema: &CompiledSchema,
) -> Result<Vec<ObserverDefinition>> {
    if declared.is_empty() {
        return Ok(Vec::new());
    }
    if !schema.observers_config.as_ref().is_some_and(|o| o.enabled) {
        let names: Vec<&str> = declared.iter().map(|o| o.name.as_str()).collect();
        bail!(
            "this schema declares {} observer(s) ({}), and [observers] is not enabled: the \
             server would start no observer runtime and none of them would fire. Set \
             `enabled = true` under [observers], or remove the declarations",
            declared.len(),
            names.join(", ")
        );
    }
    declared.iter().map(|observer| compile_one(observer, schema)).collect()
}

fn compile_one(
    observer: &IntermediateObserver,
    schema: &CompiledSchema,
) -> Result<ObserverDefinition> {
    let name = observer.name.as_str();
    let condition_warnings = check_condition(observer, schema)
        .map_err(|refusal| anyhow::anyhow!("observer `{name}`: {refusal}"))?;
    for (index, action) in observer.actions.iter().enumerate() {
        check_action(action, &observer.entity, schema)
            .with_context(|| format!("observer `{name}`, action {index}"))?;
    }
    for message in condition_warnings.iter().chain(&captured_row_warnings(observer, schema)) {
        warn!(observer = name, "{message}");
    }
    let retry = observer.retry.resolved();
    let document = serde_json::to_value(&retry)
        .with_context(|| format!("observer `{name}`: `retry` does not serialize"))?;
    serde_json::from_value::<fraiseql_observers::RetryConfig>(document).map_err(|e| {
        anyhow::anyhow!("observer `{name}`: `retry` is not a policy the runtime reads: {e}")
    })?;

    Ok(ObserverDefinition {
        name: observer.name.clone(),
        entity: observer.entity.clone(),
        event: observer.event.clone(),
        condition: observer.condition.clone(),
        actions: observer.actions.clone(),
        retry,
    })
}

/// Parse the observer's condition, and check each key it reads against the entity
/// ([`CompiledSchema::condition_key`]): the warnings to print once the observer is
/// accepted, one per message.
///
/// # Errors
///
/// Returns why the condition does not parse, or the refusal of the first key no event of
/// the entity can hold.
fn check_condition(
    observer: &IntermediateObserver,
    schema: &CompiledSchema,
) -> std::result::Result<Vec<String>, String> {
    let Some(condition) = observer.condition.as_deref() else {
        return Ok(Vec::new());
    };
    let ast = ConditionParser::new()
        .parse(condition)
        .map_err(|e| format!("`condition` {condition:?} does not parse: {e}"))?;
    // An entity the schema does not declare is refused by the validator before this runs.
    let Some(entity) = schema.find_type(&observer.entity) else {
        return Ok(Vec::new());
    };
    let mut warnings: Vec<String> = Vec::new();
    for (key, read) in ast.field_references() {
        if let Some(warning) = schema.condition_key(entity, key, condition_read(read))? {
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }
    Ok(warnings)
}

/// The schema's name for how a condition reads a key.
const fn condition_read(read: FieldRead) -> ConditionRead {
    match read {
        FieldRead::Value => ConditionRead::Value,
        FieldRead::Presence => ConditionRead::Presence,
        FieldRead::Change => ConditionRead::Change,
    }
}

/// What the compile says of a declared observer it accepts: each webhook `body_template`
/// placeholder a captured row of the entity cannot fill
/// ([`CompiledSchema::captured_row_warning`]).
///
/// Values, apart from their emission, so the rule can be asserted.
pub(super) fn captured_row_warnings(
    observer: &IntermediateObserver,
    schema: &CompiledSchema,
) -> Vec<String> {
    let Some(entity) = schema.find_type(&observer.entity) else {
        return Vec::new();
    };
    observer
        .actions
        .iter()
        .filter_map(|action| ActionConfig::from_authored(action).ok())
        .filter_map(|action| match action {
            ActionConfig::Webhook { body_template, .. } => body_template,
            _ => None,
        })
        .flat_map(|template| body_template_placeholders(&template))
        .filter_map(|path| schema.captured_row_warning(entity, &path))
        .collect()
}

/// Refuse an action the runtime would not run as written.
fn check_action(action: &serde_json::Value, entity: &str, schema: &CompiledSchema) -> Result<()> {
    let Some(authored) = action.as_object() else {
        bail!("an action is an object with a `type`");
    };
    // Before the action is parsed, so the refusal cannot repeat the secret.
    if authored.get("signing_secret").is_some_and(|secret| !secret.is_null()) {
        bail!(
            "`signing_secret` writes the signing key into the compiled schema, a build \
             artifact. Name the environment variable that holds it with `signing_secret_env`"
        );
    }
    // The same for a header whose name says it carries a credential.
    if let Some(name) = authored
        .get("headers")
        .and_then(serde_json::Value::as_object)
        .and_then(|headers| headers.keys().find(|name| is_secret_header_name(name)))
    {
        bail!(
            "header `{name}` under `headers` writes its value into the compiled schema, a build \
             artifact. Name the environment variable that holds it under `headers_env`"
        );
    }
    let read = ActionConfig::from_authored(action).map_err(|e| anyhow::anyhow!("{e}"))?;

    let ActionConfig::Webhook {
        body_template: Some(template),
        ..
    } = &read
    else {
        return Ok(());
    };
    // An entity the schema does not declare is refused by the validator before this runs.
    let Some(entity) = schema.find_type(entity) else {
        return Ok(());
    };
    for path in body_template_placeholders(template) {
        if let Err(unfillable) = schema.document_path(entity, &path) {
            let held = unfillable.held.map_or_else(String::new, |key| {
                format!(" The document holds that field as `{key}`.")
            });
            bail!(
                "`body_template` placeholder `{{{{ {path} }}}}` names no field of `{}`: every \
                 event would be dead-lettered at delivery.{held}",
                entity.name
            );
        }
    }
    Ok(())
}
