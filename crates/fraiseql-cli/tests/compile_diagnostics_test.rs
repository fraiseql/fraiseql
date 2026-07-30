#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! Compile diagnostics must be produced, complete, and correct.
//!
//! Two issues, one theme: the compiler's *diagnostic* paths were the least trustworthy code
//! in the CLI.
//!
//! * `#723` — `commands::compile` chose a schema source with `if let Ok(schema) =
//!   SchemaMerger::merge_from_domains(toml_path)`, and the same for includes. A genuine failure in
//!   a **configured** `[domains]` or `[schema.includes]` section — bad path, unreadable file, parse
//!   error — was discarded, and compilation fell through to TOML-only definitions. The user got
//!   either a schema silently missing their domain types, or a later death with the misleading
//!   "Failed to load schema from TOML". This contradicts the project's own `#612` doctrine:
//!   configured input that fails must fail loud.
//!
//! * `#724` — four gaps in the validator's diagnostics, the first of which is a **panic on the
//!   diagnostic path itself**: `suggest_similar_type` sliced `&typo[0..1]`, a *byte* range, so an
//!   empty base type or a name beginning with a multi-byte character aborted the CLI *while it was
//!   composing an error message*.

use std::fs;

use fraiseql_cli::schema::{
    SchemaValidator,
    intermediate::IntermediateSchema,
    validator::{ErrorSeverity, ValidationReport},
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Validate a raw intermediate schema, returning the report.
fn validate(corpus: Value) -> ValidationReport {
    let schema: IntermediateSchema =
        serde_json::from_value(corpus).expect("corpus must deserialize");
    SchemaValidator::validate(&schema).expect("validation must return a report, not fail")
}

/// A one-type schema with a query whose return type the caller chooses.
fn schema_returning(return_type: &str) -> Value {
    json!({
        "types": [{
            "name": "User",
            "sql_source": "v_user",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "queries": [{
            "name": "users",
            "return_type": return_type,
            "returns_list": true,
            "sql_source": "v_user"
        }]
    })
}

// ===========================================================================
// #724.1 — the diagnostic path must not panic
// ===========================================================================

/// An empty `return_type` must produce a diagnostic, not abort the process.
///
/// `extract_base_type("")` returns `""`, and `&""[0..1]` panics with "byte index 1 is out of
/// bounds". The panic was even documented in a `# Panics` section — with no caller guarding
/// it.
#[test]
fn an_empty_return_type_is_diagnosed_not_panicked_on() {
    let report = validate(schema_returning(""));
    assert!(
        report.has_errors(),
        "an empty return_type must be reported as an error, not accepted"
    );
}

/// A type name starting with a multi-byte character must not panic.
///
/// `&"Ünknown"[0..1]` is not a char boundary: "byte index 1 is not a char boundary; it is
/// inside 'Ü' (bytes 0..2)". Non-ASCII type names are legal `GraphQL-adjacent` input and
/// ordinary in non-English codebases.
#[test]
fn a_multibyte_type_name_is_diagnosed_not_panicked_on() {
    let report = validate(schema_returning("Ünknown"));
    assert!(
        report.has_errors(),
        "an unknown multi-byte return_type must be reported, not accepted"
    );
}

/// The same hazard from the other direction: a *known* type whose name is multi-byte, with
/// an ASCII typo referencing it. The old code sliced `&name[0..1]` on every candidate too.
#[test]
fn a_multibyte_candidate_name_is_safe_to_suggest_against() {
    let report = validate(json!({
        "types": [{
            "name": "Éclair",
            "sql_source": "v_eclair",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "queries": [{
            "name": "eclairs", "return_type": "Eclair", "returns_list": true,
            "sql_source": "v_eclair"
        }]
    }));
    assert!(report.has_errors(), "the unknown 'Eclair' must be reported");
}

/// A suggestion must actually be *similar*, not "the first three names starting with the
/// same letter".
///
/// The old implementation was first-letter matching behind a comment calling it
/// "Levenshtein-style". For a typo of `User` it would happily suggest `Universe` and
/// `Umbrella` while ranking the real answer nowhere.
#[test]
fn a_suggestion_names_the_actually_similar_type() {
    let report = validate(json!({
        "types": [
            {"name": "User", "sql_source": "v_user",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]},
            {"name": "Universe", "sql_source": "v_universe",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]},
            {"name": "Umbrella", "sql_source": "v_umbrella",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ],
        "queries": [{"name": "q", "return_type": "Usr", "returns_list": true,
                     "sql_source": "v_user"}]
    }));

    let suggestion = report
        .errors
        .iter()
        .find_map(|e| e.suggestion.as_deref())
        .expect("an unknown return type must carry a suggestion");

    assert!(
        suggestion.contains("User"),
        "the suggestion for the typo 'Usr' must name 'User' (edit distance 1); got {suggestion:?}"
    );
    assert!(
        !suggestion.contains("Umbrella") && !suggestion.contains("Universe"),
        "the suggestion must exclude names that merely share a first letter. 'Umbrella' is 7 \
         edits from 'Usr'; offering it as a candidate is what made the old first-letter match \
         useless on any schema with more than a handful of types. got {suggestion:?}"
    );
}

// ===========================================================================
// #724.3 — the duplicate diagnostic must point at the offending element
// ===========================================================================

/// Every duplicate must be reported at **its own** index.
///
/// The path was `types[{type_names.len()}].name` — the count of *unique names seen so far*,
/// because the loop did not `enumerate()`. For a single duplicate the two often coincide,
/// which is why this needs more than one: with `[A, B, A, A]` the unique count is stuck at 2
/// for both repeats, so the old code reported `types[2]` twice and element 3 was never named.
#[test]
fn every_duplicate_type_is_reported_at_its_own_index() {
    let report = validate(json!({
        "types": [
            {"name": "A", "sql_source": "v_a",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]},
            {"name": "B", "sql_source": "v_b",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]},
            {"name": "A", "sql_source": "v_a2",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]},
            {"name": "A", "sql_source": "v_a3",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ]
    }));

    let paths: Vec<&str> = report
        .errors
        .iter()
        .filter(|e| e.message.contains("Duplicate type"))
        .map(|e| e.path.as_str())
        .collect();

    assert_eq!(
        paths,
        ["types[2].name", "types[3].name"],
        "the duplicates are elements 2 and 3. The unique-name count is 2 at both, so a \
         count-based path reports types[2] twice and never names element 3 — sending the \
         reader to an element that is not the problem"
    );
}

// ===========================================================================
// #724.2 — a field-type typo must not be silently legalized
// ===========================================================================

/// A misspelled scalar in a field type must be surfaced.
///
/// The validator registered *every* type name appearing in any object field as an implicit
/// custom scalar, so `"type": "Strng"` validated cleanly — and worse, the typo then also
/// legalized `Strng` as a query return type, so the mistake propagated instead of being
/// caught.
#[test]
fn a_misspelled_field_scalar_is_surfaced() {
    let report = validate(json!({
        "types": [{
            "name": "User",
            "sql_source": "v_user",
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "name", "type": "Strng", "nullable": false}
            ]
        }],
        "queries": [{"name": "users", "return_type": "User", "returns_list": true,
                     "sql_source": "v_user"}]
    }));

    let mentions_typo = report.errors.iter().any(|e| e.message.contains("Strng"));

    assert!(
        mentions_typo,
        "a field typed 'Strng' must produce a diagnostic naming it. Implicit custom-scalar \
         registration made every typo a legal scalar. diagnostics={:?}",
        report.errors.iter().map(|e| (&e.message, e.severity)).collect::<Vec<_>>()
    );
}

/// A genuinely declared custom scalar must **not** be flagged — the point is to catch typos
/// without breaking the custom-scalar ergonomics that made implicit registration attractive.
#[test]
fn a_declared_custom_scalar_is_not_flagged_as_a_typo() {
    let report = validate(json!({
        "types": [{
            "name": "User",
            "sql_source": "v_user",
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "email", "type": "Email", "nullable": false}
            ]
        }],
        "custom_scalars": [{"name": "Email", "base_type": "String"}],
        "queries": [{"name": "users", "return_type": "User", "returns_list": true,
                     "sql_source": "v_user"}]
    }));

    let complains = report.errors.iter().any(|e| e.message.contains("Email"));

    assert!(
        !complains,
        "a declared custom scalar must be accepted silently; got diagnostics={:?}",
        report.errors.iter().map(|e| (&e.message, e.severity)).collect::<Vec<_>>()
    );
}

/// A declared custom scalar must participate in typo suggestions.
///
/// `#724` item 2's other half: custom scalars were invisible to `suggest_similar_type`, so a
/// near-miss on one got a suggestion list that omitted the very name the author meant.
#[test]
fn a_custom_scalar_participates_in_typo_suggestions() {
    let report = validate(json!({
        "types": [{
            "name": "User", "sql_source": "v_user",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "custom_scalars": [{"name": "EmailAddress", "base_type": "String"}],
        "queries": [{"name": "q", "return_type": "EmailAddres", "returns_list": true,
                     "sql_source": "v_user"}]
    }));

    let suggestion = report
        .errors
        .iter()
        .find_map(|e| e.suggestion.as_deref())
        .expect("the unknown return type must carry a suggestion");

    assert!(
        suggestion.contains("EmailAddress"),
        "a declared custom scalar must be a suggestion candidate; got {suggestion:?}"
    );
}

// ===========================================================================
// #724.4 — the report must be complete, not first-error-only
// ===========================================================================

/// `SchemaValidator` already collects every error — pinned so the good tier stays good.
#[test]
fn the_validator_tier_reports_every_independent_error() {
    let report = validate(json!({
        "types": [{
            "name": "User", "sql_source": "v_user",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "queries": [
            {"name": "a", "return_type": "Missing1", "returns_list": true, "sql_source": "v_a"},
            {"name": "b", "return_type": "Missing2", "returns_list": true, "sql_source": "v_b"},
            {"name": "c", "return_type": "Missing3", "returns_list": true, "sql_source": "v_c"}
        ]
    }));

    let named: Vec<&str> = ["Missing1", "Missing2", "Missing3"]
        .into_iter()
        .filter(|m| {
            report
                .errors
                .iter()
                .any(|e| e.severity == ErrorSeverity::Error && e.message.contains(m))
        })
        .collect();

    assert_eq!(
        named.len(),
        3,
        "all three unknown return types must be reported in one pass; got {named:?} from {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

/// The **converter's** validation tier must also report everything, with suggestions.
///
/// This is `#724` item 4, and it is the tier a user is more likely to meet: errors that only
/// surface after synthesis (relay, cascade, changelog-injected types) are validated here.
/// `SchemaConverter::validate` bailed on the first error with no suggestion and redundantly
/// `warn!`d the same text — so the same class of mistake got a materially worse experience
/// depending on which tier caught it. A user with three typos fixed one, recompiled, found
/// the next: three cycles for information the compiler had on the first.
#[test]
fn the_converter_tier_reports_every_error_with_suggestions() {
    let corpus = json!({
        "types": [{
            "name": "User", "sql_source": "v_user",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "queries": [
            {"name": "a", "return_type": "Usr", "returns_list": true, "sql_source": "v_a"},
            {"name": "b", "return_type": "Nonexistent2", "returns_list": true,
             "sql_source": "v_b"},
            {"name": "c", "return_type": "Nonexistent3", "returns_list": true,
             "sql_source": "v_c"}
        ]
    });

    let schema: IntermediateSchema = serde_json::from_value(corpus).unwrap();
    let err = fraiseql_cli::schema::SchemaConverter::convert(schema)
        .expect_err("three unknown return types must fail conversion");

    let msg = format!("{err:#}");
    for missing in ["Usr", "Nonexistent2", "Nonexistent3"] {
        assert!(
            msg.contains(missing),
            "the converter must report every unknown type in one pass, not bail on the first. \
             {missing:?} is missing from: {msg}"
        );
    }
    assert!(
        msg.contains("User"),
        "the converter's diagnostics must carry suggestions like the validator's — 'Usr' is one \
         edit from 'User' and the old tier offered nothing. got: {msg}"
    );
    assert!(
        !msg.contains("Unknown"),
        "`extract_type_name`'s `_ => \"Unknown\"` fallback produced the baffling \"references \
         unknown type 'Unknown'\"; the real type name must appear instead. got: {msg}"
    );
}

// ===========================================================================
// #723 — a configured-but-failing schema source must fail the compile
// ===========================================================================

/// `[domain_discovery]` pointing at a missing root must fail, not fall through.
///
/// `if let Ok(schema) = merge_from_domains(...)` discarded the error and continued to
/// TOML-only definitions. `resolve_domains` already produces a precise message ("Domain
/// discovery root not found: …") — it simply never reached the user.
#[test]
fn a_configured_domain_root_that_is_missing_fails_the_compile() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "d"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[domain_discovery]
enabled = true
root_dir = "no_such_directory"
"#,
    )
    .unwrap();

    let err = load(dir.path().join("fraiseql.toml").to_str().unwrap())
        .expect_err("a configured domain root that does not exist must fail the compile");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("no_such_directory"),
        "the error must name the configured path so the user can fix it; got: {msg}"
    );
}

/// A configured domain whose `types.json` is malformed must fail, not fall through.
///
/// This is the more dangerous half: the root exists and the domain is discovered, so the
/// user has every reason to believe their types were loaded. A parse error inside one domain
/// file used to leave them with a schema quietly missing that domain.
#[test]
fn a_malformed_domain_file_fails_the_compile() {
    let dir = TempDir::new().unwrap();
    let domain = dir.path().join("schema").join("users");
    fs::create_dir_all(&domain).unwrap();
    fs::write(domain.join("types.json"), "{ this is not valid json").unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "d"
version = "1.0.0"
database_target = "postgresql"

[types.Placeholder]
sql_source = "v_placeholder"
[types.Placeholder.fields.id]
type = "ID"
nullable = false

[domain_discovery]
enabled = true
root_dir = "schema"
"#,
    )
    .unwrap();

    // `root_dir` is resolved relative to the process CWD, so run from the temp dir.
    let err = with_cwd(dir.path(), || load("fraiseql.toml"))
        .expect_err("a malformed file in a configured domain must fail the compile");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("types.json"),
        "the error must name the file that failed to parse; got: {msg}"
    );
}

/// `[schema.includes]` naming a file that does not exist must fail, not fall through.
#[test]
fn a_configured_include_that_is_missing_fails_the_compile() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "i"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[includes]
types = ["definitely_missing_types.json"]
"#,
    )
    .unwrap();

    let result = with_cwd(dir.path(), || load("fraiseql.toml"));

    // A glob matching nothing is not itself an error; a *literal* path that does not exist
    // is. Either way the compile must not silently produce a schema without the include.
    if let Ok(schema) = result {
        panic!(
            "a configured include that resolves to nothing must not compile silently — got a \
             schema with {} type(s) and no diagnostic",
            schema.types.len()
        );
    }
}

/// A configured include whose file is malformed must fail the compile.
#[test]
fn a_malformed_include_fails_the_compile() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("broken.json"), "{ nope").unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "i"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[includes]
types = ["broken.json"]
"#,
    )
    .unwrap();

    let err = with_cwd(dir.path(), || load("fraiseql.toml"))
        .expect_err("a malformed included file must fail the compile");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("broken.json"),
        "the error must name the file that failed to parse; got: {msg}"
    );
}

/// A TOML with neither domains nor includes configured must still compile from TOML-only
/// definitions — the fall-through must survive for the "not configured" case.
///
/// This is the test that keeps the `#723` fix honest: distinguishing "not configured"
/// (fall through) from "configured but failed" (propagate) is the whole point, and a fix
/// that propagated both would break every TOML-only project.
#[test]
fn a_toml_with_no_domains_or_includes_still_compiles() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "plain"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"
"#,
    )
    .unwrap();

    let schema = with_cwd(dir.path(), || load("fraiseql.toml"))
        .expect("a TOML-only project must still compile when nothing else is configured");
    assert_eq!(schema.types.len(), 1, "the TOML-declared type must be present");
    assert_eq!(schema.queries.len(), 1, "the TOML-declared query must be present");
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Load an intermediate schema the way `fraiseql compile <toml>` does.
fn load(toml_path: &str) -> anyhow::Result<IntermediateSchema> {
    fraiseql_cli::commands::compile::load_intermediate_schema(toml_path, &[], &[], &[], None, None)
}

/// Run a closure with the process CWD set to `dir`, restoring it afterwards.
///
/// `root_dir` and include globs resolve relative to the CWD. Tests that need this are
/// serialized by a mutex because the CWD is process-global.
fn with_cwd<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, OnceLock};
    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = CWD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    let result = f();
    std::env::set_current_dir(original).unwrap();
    result
}
