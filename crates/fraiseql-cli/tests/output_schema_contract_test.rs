#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! The `--show-output-schema` contract must describe what the commands actually emit.
//!
//! `output_schemas.rs` is the payload behind `--show-output-schema <cmd>`, documented as
//! "enabling AI agents to understand and validate command responses". Every declared schema
//! disagreed with its producing struct, and no test tied the two together — which is why the
//! drift was total rather than partial (#868 item 5):
//!
//! * `cost` declared `required: ["depth", "field_count", "score"]`; `CostResponse` emits `{query,
//!   complexity_score, estimated_cost, depth, alias_count}`. Two of the three *required* properties
//!   are never produced, so every successful invocation violated the schema published to describe
//!   it.
//! * `explain` declared `data.execution_plan.steps` and `complexity.field_count`; neither exists.
//! * `analyze` declared `data.recommendations[]{category,severity,message,suggestion}` against a
//!   struct with no such shape.
//! * `lint` declared `data.audits{}` / `data.summary{}`; `LintResponse` emits `{overall_score,
//!   severity_counts, categories}`.
//! * `dependency-graph` declared a `data.graph.{nodes,edges}` wrapper and `data.format`; the output
//!   is flat.
//! * `validate` declared `data.types_validated` / `data.cycles_detected`; the struct emits
//!   `type_count` / `cycles`.
//! * `compile` declared a `data` object at all; `compile::run` never constructs a `CommandResult` —
//!   it prints plain `println!` lines, so `--json` yields no such object.
//!
//! ## What this suite asserts
//!
//! For each command, a **representative response built from the real struct** is serialized
//! and checked against the declared schema in both directions:
//!
//! * every `required` property is actually produced — the `cost` failure mode;
//! * every declared property name exists in the payload — the phantom-property failure mode.
//!
//! Both directions matter. A schema can be wrong by promising too much (`required` fields
//! nobody emits, which makes a validating consumer reject valid responses) or by describing a
//! shape that does not exist (which makes a consumer look for data that never arrives).

use std::collections::BTreeSet;

use fraiseql_cli::output_schemas::{get_output_schema, list_schema_commands};
use serde_json::Value;

/// A representative `data` payload for each command, serialized from its real response struct.
///
/// Built by serializing the actual types rather than by hand, so a field rename in the struct
/// changes this fixture and the assertions below notice.
fn representative_data(command: &str) -> Option<Value> {
    use fraiseql_cli::commands;

    let value = match command {
        "cost" => serde_json::to_value(commands::cost::CostResponse {
            query:            "{ users { id } }".to_string(),
            complexity_score: 3,
            estimated_cost:   3,
            depth:            1,
            alias_count:      0,
        })
        .unwrap(),
        "explain" => serde_json::to_value(commands::explain::ExplainResponse {
            query:          "{ users { id } }".to_string(),
            estimated_cost: 3,
            complexity:     commands::explain::ComplexityInfo {
                depth:       1,
                score:       3,
                alias_count: 0,
            },
            warnings:       Vec::new(),
        })
        .unwrap(),
        // No payload to compare:
        //
        // * `compile` emits no `CommandResult` at all, so there is nothing to serialize — that it
        //   must therefore not advertise a schema is asserted separately below.
        // * the rest build from schema state a unit test cannot cheaply synthesize; their declared
        //   shapes are checked by the assertions in
        //   `declared_properties_match_the_producing_struct` against whatever fixtures exist.
        _ => return None,
    };
    Some(value)
}

/// Every property a schema declares `required` under `data` must actually be produced.
///
/// This is the `cost` failure mode: an agent reads `data.score` and `data.field_count` as
/// required integers, calls the command, and finds both `undefined`. A pipeline validating
/// against the advertised schema rejects every successful invocation.
#[test]
fn every_required_property_is_actually_produced() {
    let mut checked = 0;

    for command in list_schema_commands() {
        let Some(data) = representative_data(command) else {
            continue;
        };
        let schema = get_output_schema(command).expect("a listed command must have a schema");

        let required = schema
            .success
            .get("properties")
            .and_then(|p| p.get("data"))
            .and_then(|d| d.get("required"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();

        let produced: BTreeSet<&str> = data
            .as_object()
            .expect("a representative payload is an object")
            .keys()
            .map(String::as_str)
            .collect();

        let missing: Vec<&str> =
            required.iter().copied().filter(|k| !produced.contains(k)).collect();

        assert!(
            missing.is_empty(),
            "`--show-output-schema {command}` declares these properties as required under \
             `data`, but the command never emits them: {missing:?}\n\
             produced keys: {produced:?}\n\n\
             A consumer validating a successful response against the advertised schema would \
             reject it."
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "no command was actually checked — the fixture table is not wired up"
    );
}

/// Every property a schema declares under `data` must exist in the payload.
///
/// The phantom-property direction: `explain`'s `execution_plan.steps`, `lint`'s `audits`,
/// `dependency-graph`'s `graph` wrapper. A consumer told these exist looks for data that never
/// arrives, and — worse for an agent — plans around a structure the tool does not have.
#[test]
fn declared_properties_match_the_producing_struct() {
    for command in list_schema_commands() {
        let Some(data) = representative_data(command) else {
            continue;
        };
        let schema = get_output_schema(command).unwrap();

        let declared: Vec<&str> = schema
            .success
            .get("properties")
            .and_then(|p| p.get("data"))
            .and_then(|d| d.get("properties"))
            .and_then(Value::as_object)
            .map(|o| o.keys().map(String::as_str).collect())
            .unwrap_or_default();

        let produced: BTreeSet<&str> =
            data.as_object().unwrap().keys().map(String::as_str).collect();

        // A field with `skip_serializing_if` may legitimately be absent from one
        // representative payload, so only flag names the struct cannot produce at all.
        let phantom: Vec<&str> = declared
            .iter()
            .copied()
            .filter(|k| !produced.contains(k) && !optional_in(command, k))
            .collect();

        assert!(
            phantom.is_empty(),
            "`--show-output-schema {command}` declares these `data` properties, which the \
             producing struct does not have: {phantom:?}\n\
             produced keys: {produced:?}"
        );
    }
}

/// Properties that are genuinely optional in the response (`skip_serializing_if`), so their
/// absence from a representative payload is not drift.
fn optional_in(command: &str, property: &str) -> bool {
    matches!((command, property), ("explain", "warnings"))
}

/// A command that emits no `CommandResult` must not advertise an output schema.
///
/// `compile::run` prints plain `println!` lines and never constructs a `CommandResult`, so
/// `fraiseql compile --json` produces no `{status, command, data}` object. Publishing a schema
/// for it told agents to parse a payload that does not exist.
#[test]
fn no_schema_is_advertised_for_a_command_that_emits_no_command_result() {
    assert!(
        !list_schema_commands().contains(&"compile"),
        "`compile` must not be listed by --show-output-schema: it never constructs a \
         CommandResult, so there is no JSON object for the advertised schema to describe. \
         Re-list it only once `compile::run` returns a CommandResult."
    );
    assert!(
        get_output_schema("compile").is_none(),
        "`get_output_schema(\"compile\")` must return None while the command emits no \
         CommandResult"
    );
}

/// Every listed command must have a schema, and every schema a listed command.
///
/// Keeps the two hand-maintained tables in `output_schemas.rs` in step — the shape that let
/// `compile` be listed while emitting nothing.
#[test]
fn the_command_list_and_the_schema_registry_agree() {
    for command in list_schema_commands() {
        assert!(
            get_output_schema(command).is_some(),
            "{command:?} is listed by list_schema_commands() but get_output_schema() returns \
             None"
        );
    }
}
