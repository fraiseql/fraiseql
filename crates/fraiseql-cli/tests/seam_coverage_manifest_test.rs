#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! Seam field-coverage gate — the durable half of the compiled-schema seam contract.
//!
//! `#755`/`#756`/`#779`/`#847`/`#848` were not five bugs; they were one shape recurring. The
//! fixes close today's instances. **This file is what stops instance N+1**, by refusing to
//! let a field exist on the authoring → compile boundary without an answer to two questions:
//!
//! 1. **Is it classified?** Every field of `IntermediateSchema` must be an array section, a
//!    singleton section, or explicitly listed in [`KNOWN_UNAUTHORED`] with a reason. An
//!    unclassified field is one the loaders and merger do not carry — the `#755` shape.
//! 2. **Is it probed?** Every array section must have a probe in `compiled_schema_seam_test.rs`
//!    asserting it survives to the compiled schema. A carried field with no round-trip assertion is
//!    a field nobody has checked arrives.
//!
//! ## How the gate binds
//!
//! [`every_field_is_classified`] constructs `IntermediateSchema` as an **exhaustive struct
//! literal** — no `..Default::default()`. Adding a field to the struct therefore fails this
//! file at *compile* time, before any assertion runs, and the author cannot proceed without
//! classifying it. That is deliberate and it is the whole mechanism: a runtime key-set
//! comparison can be satisfied by a field that serializes to nothing, and `#[serde(default)]`
//! plus `skip_serializing_if` means most of these fields do exactly that when unset.
//!
//! Modelled on `config_coverage_manifest_test.rs`, which does the same job for TOML config
//! keys (`#612`).

use std::{collections::BTreeSet, fs, path::Path};

use fraiseql_cli::schema::{
    intermediate::{
        IntermediateInjectDefaults, IntermediateQueryDefaults, IntermediateScalar,
        IntermediateSchema,
    },
    seam::{AUTHORABLE_ARRAY_SECTIONS, AUTHORABLE_SINGLETON_SECTIONS},
};
use fraiseql_core::schema::{
    ChangelogConfig, DebugConfig, GrpcConfig, HierarchiesConfig, McpConfig, NamingConvention,
    RestConfig, SessionVariablesConfig, SubscriptionsConfig, ValidationConfig,
};

/// Fields of `IntermediateSchema` that are **not** authored at the seam, and why.
///
/// A field belongs here only when an SDK cannot emit it — it is populated by the compiler
/// itself. Anything an SDK *can* write must be a carried section instead, or it is the `#755`
/// silent drop by another name.
const KNOWN_UNAUTHORED: &[(&str, &str)] = &[(
    "query_defaults",
    "injected by the merger from the TOML [query_defaults] section; never present in \
         schema.json (documented on the field)",
)];

/// The path to the round-trip suite whose probes this gate cross-checks.
const ROUND_TRIP_SUITE: &str = "tests/compiled_schema_seam_test.rs";

/// Array sections with no round-trip probe, and why.
///
/// Empty is the goal. An entry here is a section carried through the seam that nothing
/// asserts arrives — acceptable only with a stated reason.
const KNOWN_UNPROBED: &[(&str, &str)] = &[
    (
        "fragments",
        "client-side selection reuse; the compiled schema has no fragments field, so there is \
         nothing to assert arrival in — carried so an authored block cannot vanish silently",
    ),
    (
        "fact_tables",
        "analytics metadata, covered end-to-end by the analytics compile tests rather than by \
         a presence probe",
    ),
    (
        "aggregate_queries",
        "refused at compile time (#956) rather than carried — the round-trip suite asserts \
         the refusal, not arrival. This excuse previously read 'analytics metadata, same as \
         fact_tables', which was false in the one way that mattered: `fact_tables` reaches \
         the compiled schema and `aggregate_queries` did not, so the manifest was covering \
         for the drop it exists to expose",
    ),
    (
        "directives",
        "custom directive definitions, covered by the directive conversion tests",
    ),
    (
        "observers",
        "refused at compile time (#779) rather than carried — the round-trip suite asserts the \
         refusal, not arrival",
    ),
];

/// Every field of `IntermediateSchema` must be classified.
///
/// The exhaustive literal below is the gate: a new field breaks this file at compile time.
#[test]
fn every_field_is_classified() {
    // Exhaustive on purpose — do NOT add `..Default::default()`. See the module docs.
    let schema = IntermediateSchema {
        version:              "2.0.0".to_string(),
        types:                Vec::new(),
        enums:                Vec::new(),
        input_types:          Vec::new(),
        interfaces:           Vec::new(),
        unions:               Vec::new(),
        queries:              Vec::new(),
        mutations:            Vec::new(),
        subscriptions:        Vec::new(),
        fragments:            Some(Vec::new()),
        directives:           Some(Vec::new()),
        fact_tables:          Some(Vec::new()),
        aggregate_queries:    Some(Vec::new()),
        observers:            Some(Vec::new()),
        sources:              Some(Vec::new()),
        custom_scalars:       Some(Vec::new()),
        security:             Some(serde_json::json!({})),
        auth:                 Some(serde_json::json!({})),
        observers_config:     Some(serde_json::json!({})),
        federation_config:    Some(serde_json::json!({})),
        subscriptions_config: Some(SubscriptionsConfig::default()),
        validation_config:    Some(ValidationConfig::default()),
        debug_config:         Some(DebugConfig::default()),
        mcp_config:           Some(McpConfig::default()),
        rest_config:          Some(RestConfig::default()),
        grpc_config:          Some(GrpcConfig::default()),
        query_defaults:       Some(IntermediateQueryDefaults::default()),
        inject_defaults:      Some(IntermediateInjectDefaults::default()),
        naming_convention:    NamingConvention::default(),
        session_variables:    Some(SessionVariablesConfig::default()),
        hierarchies_config:   Some(HierarchiesConfig::default()),
        changelog_config:     Some(ChangelogConfig::default()),
    };

    let value = serde_json::to_value(&schema).expect("a fully-populated schema must serialize");
    let emitted: BTreeSet<&str> = value
        .as_object()
        .expect("schema serializes to an object")
        .keys()
        .map(String::as_str)
        .collect();

    let classified: BTreeSet<&str> = AUTHORABLE_ARRAY_SECTIONS
        .iter()
        .chain(AUTHORABLE_SINGLETON_SECTIONS.iter())
        .copied()
        .chain(KNOWN_UNAUTHORED.iter().map(|(field, _)| *field))
        .collect();

    let unclassified: Vec<&str> = emitted.difference(&classified).copied().collect();
    assert!(
        unclassified.is_empty(),
        "these fields of IntermediateSchema are not classified in \
         crates/fraiseql-cli/src/schema/seam.rs: {unclassified:?}\n\n\
         An unclassified field is one the loaders and the merger do not carry, which is \
         exactly the #755 silent drop. Add it to AUTHORABLE_ARRAY_SECTIONS (an array of named \
         items), AUTHORABLE_SINGLETON_SECTIONS (a config block or scalar setting), or — only \
         if an SDK genuinely cannot author it — to KNOWN_UNAUTHORED in this file with a reason."
    );
}

/// SDK-side serde aliases: authorable names that are not themselves struct fields.
///
/// `federation` is bound to `federation_config` via `#[serde(alias)]`, so it is legitimately
/// absent from the serialized form while still being a key an SDK may emit.
const SERDE_ALIASES: &[&str] = &["federation"];

/// No stale classification: every name in the seam lists must be a real field.
///
/// The mirror of the test above. A section listed but no longer present means the loaders are
/// carrying a key nothing reads, and a reader of `seam.rs` would take it for the contract.
#[test]
fn no_classified_section_is_stale() {
    let value = serde_json::to_value(fully_populated()).unwrap();
    let emitted: BTreeSet<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();

    let stale: Vec<&str> = AUTHORABLE_ARRAY_SECTIONS
        .iter()
        .chain(AUTHORABLE_SINGLETON_SECTIONS.iter())
        .copied()
        .filter(|s| !emitted.contains(s) && !SERDE_ALIASES.contains(s))
        .collect();

    assert!(
        stale.is_empty(),
        "these sections are classified in seam.rs but are not fields of IntermediateSchema: \
         {stale:?}. Remove them, or add the field — a listed section that does not exist reads \
         as contract to anyone consulting that file."
    );
}

/// Every carried array section must have a round-trip probe, or a stated reason not to.
///
/// A field the seam carries but nothing asserts arrives is a field nobody has checked. The
/// cross-check reads the sibling suite's source so a claim here cannot go stale silently.
#[test]
fn every_array_section_is_probed_by_the_round_trip_suite() {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROUND_TRIP_SUITE);
    let source = fs::read_to_string(&suite).unwrap_or_else(|e| {
        panic!(
            "cannot read the round-trip suite at {}: {e}. If it moved, update ROUND_TRIP_SUITE \
             — this cross-check is the only thing keeping KNOWN_UNPROBED honest.",
            suite.display()
        )
    });

    let excused: BTreeSet<&str> = KNOWN_UNPROBED.iter().map(|(section, _)| *section).collect();

    let unprobed: Vec<&str> = AUTHORABLE_ARRAY_SECTIONS
        .iter()
        .copied()
        .filter(|section| {
            // Probes are declared as `("<section>", |c| …)` entries in the PROBES table.
            !source.contains(&format!("(\"{section}\"")) && !excused.contains(section)
        })
        .collect();

    assert!(
        unprobed.is_empty(),
        "these authorable sections have no probe in {ROUND_TRIP_SUITE}: {unprobed:?}\n\n\
         Add a probe asserting the construct reaches the compiled schema, or add the section \
         to KNOWN_UNPROBED in this file with a reason. Carrying a section without asserting it \
         arrives is how eight categories were dropped for four minor releases (#755)."
    );
}

/// `KNOWN_UNPROBED` must not name a section that is in fact probed.
///
/// Without this, an excuse outlives the gap it excused and the next author reads a false
/// statement about what is covered.
#[test]
fn no_known_unprobed_excuse_is_stale() {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROUND_TRIP_SUITE);
    let source = fs::read_to_string(&suite).unwrap();

    let now_probed: Vec<&str> = KNOWN_UNPROBED
        .iter()
        .map(|(section, _)| *section)
        .filter(|section| source.contains(&format!("(\"{section}\"")))
        .collect();

    assert!(
        now_probed.is_empty(),
        "these sections are listed in KNOWN_UNPROBED but now have a probe: {now_probed:?}. \
         Remove the excuse."
    );
}

/// Every entry in the excuse tables must carry a non-trivial reason.
///
/// A one-word reason is not a reason; it is a way to silence the gate. Both tables exist to
/// make an omission a deliberate, reviewable act.
#[test]
fn every_excuse_states_a_reason() {
    for (name, reason) in KNOWN_UNAUTHORED.iter().chain(KNOWN_UNPROBED.iter()) {
        assert!(
            reason.len() >= 30,
            "the reason for {name:?} is too short to be one ({reason:?}). State why the field \
             cannot be authored, or why the section cannot be probed."
        );
    }
}

/// A fully-populated schema, shared by the tests that only need its key set.
fn fully_populated() -> IntermediateSchema {
    IntermediateSchema {
        version: "2.0.0".to_string(),
        fragments: Some(Vec::new()),
        directives: Some(Vec::new()),
        fact_tables: Some(Vec::new()),
        aggregate_queries: Some(Vec::new()),
        observers: Some(Vec::new()),
        sources: Some(Vec::new()),
        custom_scalars: Some(vec![IntermediateScalar {
            name:             "Probe".to_string(),
            description:      None,
            specified_by_url: None,
            validation_rules: Vec::new(),
            base_type:        None,
        }]),
        security: Some(serde_json::json!({})),
        auth: Some(serde_json::json!({})),
        observers_config: Some(serde_json::json!({})),
        federation_config: Some(serde_json::json!({})),
        subscriptions_config: Some(SubscriptionsConfig::default()),
        validation_config: Some(ValidationConfig::default()),
        debug_config: Some(DebugConfig::default()),
        mcp_config: Some(McpConfig::default()),
        rest_config: Some(RestConfig::default()),
        grpc_config: Some(GrpcConfig::default()),
        query_defaults: Some(IntermediateQueryDefaults::default()),
        inject_defaults: Some(IntermediateInjectDefaults::default()),
        session_variables: Some(SessionVariablesConfig::default()),
        hierarchies_config: Some(HierarchiesConfig::default()),
        changelog_config: Some(ChangelogConfig::default()),
        ..Default::default()
    }
}
