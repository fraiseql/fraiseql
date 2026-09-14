#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
//! #1325 — a bad function declaration fails `fraiseql compile`, not server boot.
//!
//! One test per check, each built from **the exact declaration it must reject**.
//! That framing matters: a test that asserts "some error" passes when an unrelated
//! check fires, and every one of these declarations is rejected by *something*
//! eventually — the question each test asks is whether the compiler rejects it, and
//! for the stated reason.
//!
//! Where each check lived before:
//!
//! | check | before | now |
//! |---|---|---|
//! | trigger grammar | server boot (`validate_functions_config`, its own prefix list) | `TriggerRegistry::validate_definitions`, called by compiler **and** loader |
//! | `when` / `changed_to` | server boot (`load_from_definitions`) | same rule, called at compile time first |
//! | `http:` / `after:storage` | server boot (#871) | same |
//! | `before:mutation:` names a mutation | **nowhere** | compile time |
//! | `after:mutation:` names a returned type | **nowhere** | compile time |
//! | module present, extension matches runtime | server boot | compile time when `module_dir` exists |
//!
//! The grammar checks are two call sites of one rule, not two copies: the server
//! keeps validating because a compiled schema is an input it does not produce, and
//! a hand-written or stale artifact must still fail at boot.
//!
//! **Execution engine:** in-memory · **Infrastructure:** none · **Parallelism:** safe

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A schema with one type, one mutation returning it, and the given function.
fn schema_with_function(function: Value) -> Value {
    let mut schema = json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "is_input": false,
            "fields": [{"name": "id", "type": "ID", "nullable": false},
                       {"name": "status", "type": "String", "nullable": false}]
        }],
        "mutations": [{
            "name": "updateOrder",
            "return_type": "Order",
            "sql_source": "fn_update_order",
            "operation": "update",
            "invalidates_views": ["v_order"],
            "arguments": []
        }],
    });
    // Assigned rather than interpolated: `json!` reads an interpolated value by
    // reference, and a by-value parameter it never consumes is a clippy error.
    schema["functions"] = Value::Array(vec![function]);
    schema
}

/// Compile the corpus and return the error message, failing if it compiled.
fn refusal(corpus: Value) -> String {
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    match SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default()) {
        Ok(_) => panic!("this declaration must not compile"),
        Err(error) => format!("{error:#}"),
    }
}

/// Compile the corpus, expecting success.
fn accepts(corpus: Value) {
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .expect("this declaration must compile");
}

// ── Absence, in both spellings ───────────────────────────────────────────────

/// An empty `functions` list produces **no** compiled section.
///
/// Two producers reach this, and the second is the load-bearing one:
///
/// * SDKs disagree about how to say "nothing here" — F#'s record has no optional fields, so it
///   emits `"functions": []` where Python omits the key.
/// * `seam::empty_accumulator` seeds **every** array section with `[]`, so *every* TOML-workflow
///   compile hands the converter an empty list whether or not the project has ever heard of
///   functions. Without this rule, every such artifact in the repository grew a `"functions":
///   {"module_dir": "functions", "definitions": []}` section — which is how this was found: two
///   gitignored example artifacts had one after a compile.
///
/// The conformance suite cannot see either case: an empty list and an absent key both project
/// to `{}`.
#[test]
fn an_empty_functions_list_produces_no_compiled_section() {
    let corpus = json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "is_input": false,
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "functions": []
    });
    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("the corpus must deserialize");
    let artifact = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .expect("an empty list must compile");

    assert!(
        artifact.functions.is_none(),
        "an empty list is absence, not an empty section; got {:?}",
        artifact.functions
    );
}

// ── The trigger grammar ──────────────────────────────────────────────────────

#[test]
fn an_unparseable_trigger_fails_the_compile() {
    let message = refusal(schema_with_function(json!({
        "name": "notify",
        "trigger": "whenever:something:happens",
        "runtime": "Deno"
    })));
    assert!(
        message.contains("whenever:something:happens") && message.contains("notify"),
        "the refusal must name the function and its trigger; got: {message}"
    );
}

/// The grammar the compiler enforces is the dispatcher's, not a summary of it.
///
/// The server's loader used to keep its own prefix list, and it had fallen two
/// trigger kinds behind. A compiler that copied *that* list would refuse a valid
/// declaration — so this asserts the two kinds the stale copy lost.
#[test]
fn capture_and_ingest_triggers_compile() {
    for trigger in ["after:capture:Order:update", "after:ingest:email"] {
        accepts(schema_with_function(json!({
            "name": "notify",
            "trigger": trigger,
            "runtime": "Deno"
        })));
    }
}

// ── `when` predicates (#597) ─────────────────────────────────────────────────

#[test]
fn changed_to_on_a_non_update_trigger_fails_the_compile() {
    let message = refusal(schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:insert",
        "runtime": "Deno",
        "when": [{"field": "status", "changed_to": "approved"}]
    })));
    assert!(
        message.contains("changed_to"),
        "the refusal must name the predicate operator; got: {message}"
    );
}

// ── #871: triggers nothing mounts ────────────────────────────────────────────

#[test]
fn an_http_trigger_fails_the_compile() {
    let message = refusal(schema_with_function(json!({
        "name": "webhook",
        "trigger": "http:POST:/hooks/stripe",
        "runtime": "Deno"
    })));
    assert!(
        message.contains("http triggers are not mounted"),
        "the refusal must carry the #871 message; got: {message}"
    );
}

#[test]
fn an_after_storage_trigger_fails_the_compile() {
    let message = refusal(schema_with_function(json!({
        "name": "thumbnail",
        "trigger": "after:storage:uploads:upload",
        "runtime": "Deno"
    })));
    assert!(
        message.contains("after:storage"),
        "the refusal must name the trigger kind; got: {message}"
    );
}

// ── Cross-references into the schema (checked nowhere before) ────────────────

/// `before:mutation:` matches the mutation **name**, so one that names no declared
/// mutation is a gate that never runs — on a hook whose stated purpose is
/// enforcement (#1327).
#[test]
fn a_before_mutation_trigger_naming_no_mutation_fails_the_compile() {
    let message = refusal(schema_with_function(json!({
        "name": "authorize",
        "trigger": "before:mutation:deleteOrder",
        "runtime": "Deno"
    })));
    assert!(
        message.contains("names no declared mutation") && message.contains("updateOrder"),
        "the refusal must say what is wrong and list the real mutations; got: {message}"
    );
}

/// `after:mutation:` matches the mutation's **return type** — the dispatcher builds
/// its entity event from `definition.return_type`. Naming the mutation instead is
/// the natural mistake, and it produces a function that simply never fires.
#[test]
fn an_after_mutation_trigger_naming_a_mutation_rather_than_its_return_type_fails() {
    let message = refusal(schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:updateOrder",
        "runtime": "Deno"
    })));
    assert!(
        message.contains("no declared mutation returns") && message.contains("Order"),
        "the refusal must explain that after:mutation matches the return type, and name the \
         types that are returned; got: {message}"
    );
}

#[test]
fn an_after_mutation_trigger_naming_a_returned_type_compiles() {
    accepts(schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno"
    })));
}

/// A `when` predicate naming a field the entity type does not have fails the compile.
///
/// The predicate is evaluated against the row image — the GraphQL projection of the
/// mutation's return type — so a misspelled field simply never matches, and a
/// function that never fires is indistinguishable from one whose condition held
/// false. This is the shape #597 left open: the operator was validated, the field
/// was not.
#[test]
fn a_when_predicate_naming_an_unknown_field_fails_the_compile() {
    let message = refusal(schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno",
        "when": [{"field": "statuss", "changed_to": "approved"}]
    })));
    assert!(
        message.contains("statuss") && message.contains("status"),
        "the refusal must name the bad field and list the real ones; got: {message}"
    );
}

#[test]
fn a_when_predicate_naming_a_real_field_compiles() {
    accepts(schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno",
        "when": [{"field": "status", "changed_to": "approved"}]
    })));
}

// ── The module on disk ───────────────────────────────────────────────────────

/// With `module_dir` present, a function whose module is missing — or present under
/// an extension its runtime cannot load — fails the compile.
///
/// The server resolves `<module_dir>/<name>.<ext>` and aborts startup when it finds
/// nothing. Catching it here turns a production boot failure into a compile error
/// that names the paths tried.
#[test]
fn a_declared_module_that_is_not_on_disk_fails_the_compile() {
    let dir = TempDir::new().unwrap();
    let modules = dir.path().join("mods");
    std::fs::create_dir(&modules).unwrap();
    // A Deno module is present, but the declaration says WASM — the runtime and the
    // extension disagree, which is the same failure as a missing file.
    std::fs::write(modules.join("notify.ts"), "export default () => {};\n").unwrap();

    let mut corpus = schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Wasm"
    }));
    corpus["functions_config"] = json!({"module_dir": modules.to_str().unwrap()});

    let message = refusal(corpus);
    assert!(
        message.contains("no module for the Wasm runtime") && message.contains("notify"),
        "the refusal must name the function and the runtime it cannot load; got: {message}"
    );
}

#[test]
fn a_module_present_under_a_supported_extension_compiles() {
    let dir = TempDir::new().unwrap();
    let modules = dir.path().join("mods");
    std::fs::create_dir(&modules).unwrap();
    std::fs::write(modules.join("notify.ts"), "export default () => {};\n").unwrap();

    let mut corpus = schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Deno"
    }));
    corpus["functions_config"] = json!({"module_dir": modules.to_str().unwrap()});

    accepts(corpus);
}

/// A `module_dir` the compiler cannot see is not a compile error.
///
/// A CI job that compiles before fetching build artifacts is a legitimate workflow,
/// and the compiler cannot tell that layout from a typo — it can only observe that
/// the directory is not there. The server still checks at boot. This is the same
/// trade `--database` makes for column validation, and it is the arm that would
/// silently invert if the check were written as "path exists".
#[test]
fn an_absent_module_dir_does_not_fail_the_compile() {
    let mut corpus = schema_with_function(json!({
        "name": "notify",
        "trigger": "after:mutation:Order:update",
        "runtime": "Wasm"
    }));
    corpus["functions_config"] = json!({"module_dir": "no/such/directory"});

    accepts(corpus);
}
