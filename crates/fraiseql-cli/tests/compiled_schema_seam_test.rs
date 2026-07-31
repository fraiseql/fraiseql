#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! The compiled-schema seam contract.
//!
//! One invariant, across the authoring → compile → runtime boundary:
//!
//! > **Anything an SDK can author either arrives at the runtime intact, or the compile
//! > fails loudly. There is no third outcome.**
//!
//! The third outcome — *silent empty default* — was the single most productive bug source
//! in this codebase. `#[serde(default)]` with no `deny_unknown_fields` means a renamed,
//! misspelled or unread key deserializes into an empty default and the compile reports
//! success. Nine issues were one mechanism:
//!
//! * `#755` — the merger rebuilt the merged JSON from scratch with only
//!   `version`/`types`/`queries`/`mutations`, so eight whole categories (enums, input types,
//!   interfaces, unions, subscriptions, observers, custom scalars, sources) were dropped on
//!   **every** TOML-workflow compile path.
//! * `#756` — the merger emitted `args`/`required`; the consumer read `arguments`/`nullable`. Every
//!   TOML-declared operation argument vanished.
//! * `#779` — SDK-authored observers were validated by ~220 lines of validator and then discarded
//!   by `observers: Vec::new()`.
//! * `#847` — `[inject_defaults]`, implemented in seven SDKs, had no consumer at all.
//! * `#848` — types marked `is_input` compiled as **object** types, producing a schema that used an
//!   output type in an input position.
//!
//! ## How this suite is built
//!
//! One corpus declaring every authorable construct, driven through every compile entry
//! path, with one probe per construct asserting it reached the **compiled** schema — the
//! runtime's actual input, not the intermediate representation.
//!
//! Each path test reports **every** failing construct rather than the first, so a
//! regression names the full inventory of what was dropped in one run. That is deliberate:
//! `#755` was eight simultaneous drops, and a first-fail assertion would have reported one.
//!
//! Adding an authorable construct without a probe here fails
//! `seam_coverage_manifest_test.rs`, which walks `IntermediateSchema`'s fields and demands
//! each one be either probed below or explicitly recorded as carrying no author intent.

use std::{fs, path::Path};

use fraiseql_cli::schema::{SchemaConverter, SchemaMerger, intermediate::IntermediateSchema};
use fraiseql_core::schema::{CompiledSchema, InjectedParamSource};
use serde_json::{Value, json};
use tempfile::TempDir;

// ===========================================================================
// The corpus — every authorable construct, in one SDK-shaped `types.json`
// ===========================================================================

/// An SDK-style `types.json` declaring every construct a language SDK can author.
///
/// Shapes match what the shipped SDKs actually emit (verified against the Python and
/// `TypeScript` exporters): enum values are `{"name": …}` objects, not bare strings.
fn sdk_corpus() -> Value {
    json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "implements": ["Node"],
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "email", "type": "Email", "nullable": false},
                    {"name": "status", "type": "UserStatus", "nullable": false}
                ]
            },
            {
                "name": "Post",
                "sql_source": "v_post",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "title", "type": "String", "nullable": false}
                ]
            }
        ],
        "enums": [
            {
                "name": "UserStatus",
                "description": "Lifecycle state of a user account",
                "values": [{"name": "ACTIVE"}, {"name": "BANNED"}]
            }
        ],
        "input_types": [
            {
                "name": "UserFilter",
                "description": "Filter criteria for users",
                "fields": [
                    {"name": "status", "type": "UserStatus", "nullable": true}
                ]
            }
        ],
        "interfaces": [
            {
                "name": "Node",
                "description": "An object with a globally unique ID",
                "fields": [{"name": "id", "type": "ID", "nullable": false}]
            }
        ],
        "unions": [
            {
                "name": "SearchResult",
                "member_types": ["User", "Post"],
                "description": "A result from a search"
            }
        ],
        "queries": [
            {
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "sql_source": "v_user"
            }
        ],
        "mutations": [
            {
                "name": "createUser",
                "return_type": "User",
                "sql_source": "fn_create_user",
                "operation": "INSERT",
                "arguments": [{"name": "email", "type": "String", "nullable": false}]
            }
        ],
        "subscriptions": [
            {
                "name": "userUpdated",
                "return_type": "User",
                "topic": "user_events"
            }
        ],
        "custom_scalars": [
            {
                "name": "Email",
                "base_type": "String",
                "description": "An RFC 5322 email address"
            }
        ],
        "sources": [
            {
                "name": "user_feed",
                "schedule": "*/5 * * * *",
                "function": "fn_ingest_user_feed"
            }
        ]
    })
}

/// A `fraiseql.toml` that configures nothing the corpus depends on.
///
/// Deliberately minimal: every construct asserted below is authored in the SDK JSON, so a
/// drop is attributable to the merge/convert seam rather than to TOML precedence.
const MINIMAL_TOML: &str = r#"
[schema]
name = "seam"
version = "1.0.0"
database_target = "postgresql"
"#;

// ===========================================================================
// The probes — one per authorable construct
// ===========================================================================

/// A construct probe: a human name and a check against the **compiled** schema.
type Probe = (&'static str, fn(&CompiledSchema) -> Result<(), String>);

/// Every construct the corpus authors, and the question each one asks of the compiled
/// schema. A probe asserts *identity*, not just non-emptiness — `#755` would have
/// survived a length check on `input_types`, which is pre-populated with 48 built-in
/// `*WhereInput` entries.
const PROBES: &[Probe] = &[
    ("types", |c| {
        want(c.types.iter().any(|t| t.name == "User"), "type User absent from compiled.types")
    }),
    ("queries", |c| {
        want(c.queries.iter().any(|q| q.name == "users"), "query users absent")
    }),
    ("mutations", |c| {
        want(c.mutations.iter().any(|m| m.name == "createUser"), "mutation createUser absent")
    }),
    ("enums", |c| {
        let e = c.enums.iter().find(|e| e.name == "UserStatus");
        let Some(e) = e else {
            return Err("enum UserStatus absent from compiled.enums".into());
        };
        want(
            e.values.iter().any(|v| v.name == "ACTIVE"),
            "enum UserStatus present but value ACTIVE missing",
        )
    }),
    ("input_types", |c| {
        want(
            c.input_types.iter().any(|i| i.name == "UserFilter"),
            "input type UserFilter absent (note: input_types is pre-seeded with built-in \
             *WhereInput entries, so a length check would not have caught this)",
        )
    }),
    ("interfaces", |c| {
        want(c.interfaces.iter().any(|i| i.name == "Node"), "interface Node absent")
    }),
    ("unions", |c| {
        want(c.unions.iter().any(|u| u.name == "SearchResult"), "union SearchResult absent")
    }),
    ("subscriptions", |c| {
        want(
            c.subscriptions.iter().any(|s| s.name == "userUpdated"),
            "subscription userUpdated absent",
        )
    }),
    ("custom_scalars", |c| {
        want(
            c.custom_scalars.get("Email").is_some(),
            "custom scalar Email absent from the compiled CustomTypeRegistry — its \
             validation_rules would silently never run",
        )
    }),
    ("sources", |c| {
        want(
            c.sources.iter().any(|s| s.name == "user_feed"),
            "ingress source user_feed absent",
        )
    }),
];

/// Turn a boolean expectation into a named failure.
fn want(ok: bool, msg: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(msg.to_string()) }
}

/// Run every probe and report **all** failures, so one run names the full inventory.
fn assert_all_constructs_survive(path_name: &str, compiled: &CompiledSchema) {
    let failures: Vec<String> = PROBES
        .iter()
        .filter_map(|(name, probe)| probe(compiled).err().map(|e| format!("  [{name}] {e}")))
        .collect();

    assert!(
        failures.is_empty(),
        "compile path `{path_name}` dropped {} of {} authorable constructs:\n{}",
        failures.len(),
        PROBES.len(),
        failures.join("\n")
    );
}

// ===========================================================================
// The compile entry paths
// ===========================================================================

/// Write the corpus + TOML into a scratch dir and return it.
fn scratch(corpus: &Value) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), serde_json::to_string_pretty(corpus).unwrap())
        .unwrap();
    fs::write(dir.path().join("fraiseql.toml"), MINIMAL_TOML).unwrap();
    dir
}

fn as_str(p: &Path) -> String {
    p.to_str().unwrap().to_string()
}

fn compile(intermediate: IntermediateSchema) -> CompiledSchema {
    SchemaConverter::convert(intermediate).expect("conversion of a valid corpus must succeed")
}

/// `fraiseql compile schema.json` — the legacy JSON workflow.
fn via_legacy_json(dir: &TempDir) -> CompiledSchema {
    let raw = fs::read_to_string(dir.path().join("types.json")).unwrap();
    compile(serde_json::from_str::<IntermediateSchema>(&raw).expect("corpus must deserialize"))
}

/// `fraiseql compile fraiseql.toml --types types.json`
fn via_toml_plus_types(dir: &TempDir) -> CompiledSchema {
    compile(
        SchemaMerger::merge_files(
            &as_str(&dir.path().join("types.json")),
            &as_str(&dir.path().join("fraiseql.toml")),
        )
        .expect("merge_files must succeed"),
    )
}

/// `fraiseql compile fraiseql.toml --schema-dir <dir>`
fn via_schema_dir(dir: &TempDir) -> CompiledSchema {
    let sub = dir.path().join("schemas");
    fs::create_dir_all(&sub).unwrap();
    fs::copy(dir.path().join("types.json"), sub.join("types.json")).unwrap();
    compile(
        SchemaMerger::merge_from_directory(
            &as_str(&dir.path().join("fraiseql.toml")),
            &as_str(&sub),
        )
        .expect("merge_from_directory must succeed"),
    )
}

/// `fraiseql compile fraiseql.toml --type-files types.json`
fn via_explicit_files(dir: &TempDir) -> CompiledSchema {
    compile(
        SchemaMerger::merge_explicit_files(
            &as_str(&dir.path().join("fraiseql.toml")),
            &[as_str(&dir.path().join("types.json"))],
            &[],
            &[],
        )
        .expect("merge_explicit_files must succeed"),
    )
}

// ===========================================================================
// #755 — every construct survives every compile path
// ===========================================================================

#[test]
fn legacy_json_path_carries_every_construct() {
    let dir = scratch(&sdk_corpus());
    assert_all_constructs_survive("legacy JSON (compile schema.json)", &via_legacy_json(&dir));
}

#[test]
fn toml_plus_types_path_carries_every_construct() {
    let dir = scratch(&sdk_corpus());
    assert_all_constructs_survive(
        "TOML + --types (the workflow the Python SDK prints)",
        &via_toml_plus_types(&dir),
    );
}

#[test]
fn schema_dir_path_carries_every_construct() {
    let dir = scratch(&sdk_corpus());
    assert_all_constructs_survive("TOML + --schema-dir", &via_schema_dir(&dir));
}

#[test]
fn explicit_files_path_carries_every_construct() {
    let dir = scratch(&sdk_corpus());
    assert_all_constructs_survive("TOML + explicit --type-files", &via_explicit_files(&dir));
}

/// The TOML-workflow paths must agree with the legacy JSON path construct for construct.
///
/// `#755` was precisely a divergence between them: the same authored file compiled to a
/// richer schema through `compile schema.json` than through `compile fraiseql.toml
/// --types schema.json`, and nothing compared the two.
#[test]
fn toml_workflow_agrees_with_legacy_json_path() {
    let dir = scratch(&sdk_corpus());
    let legacy = via_legacy_json(&dir);
    let toml = via_toml_plus_types(&dir);

    let divergences: Vec<String> = PROBES
        .iter()
        .filter_map(|(name, probe)| match (probe(&legacy).is_ok(), probe(&toml).is_ok()) {
            (true, false) => {
                Some(format!("  [{name}] survives legacy JSON but not the TOML workflow"))
            },
            (false, true) => {
                Some(format!("  [{name}] survives the TOML workflow but not legacy JSON"))
            },
            _ => None,
        })
        .collect();

    assert!(
        divergences.is_empty(),
        "the two documented compile workflows disagree on {} construct(s):\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

// ===========================================================================
// #756 — TOML-declared operation arguments survive
// ===========================================================================

/// A TOML-declared query argument must reach the compiled query.
///
/// The producer emitted `args`/`required`; the consumer read `arguments`/`nullable`. Both
/// carried `#[serde(default)]`, so the mismatch compiled cleanly to zero arguments — and
/// a query whose declared `id` filter was never bound returns rows the author excluded.
#[test]
fn toml_declared_query_argument_reaches_compiled_schema() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "argtest"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[queries.get_user]
return_type = "User"
return_array = false
sql_source = "v_user"
args = [{ name = "id", type = "ID", required = true }]
"#,
    )
    .unwrap();

    let compiled =
        compile(SchemaMerger::merge_toml_only(&as_str(&dir.path().join("fraiseql.toml"))).unwrap());

    let query = compiled
        .queries
        .iter()
        .find(|q| q.name == "get_user" || q.name == "getUser")
        .expect("query get_user must be compiled");

    let arg = query.arguments.iter().find(|a| a.name == "id").unwrap_or_else(|| {
        panic!(
            "TOML-declared argument `id` is absent from the compiled query (arguments = {:?}). \
             The declared filter would never be bound.",
            query.arguments.iter().map(|a| &a.name).collect::<Vec<_>>()
        )
    });

    assert!(
        !arg.nullable,
        "`required = true` in TOML must compile to a non-nullable argument, got nullable"
    );
}

/// The same for mutations — the SQL function is otherwise called with no client argument.
#[test]
fn toml_declared_mutation_argument_reaches_compiled_schema() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "argtest"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[mutations.create_user]
return_type = "User"
operation = "INSERT"
sql_source = "fn_create_user"
args = [{ name = "email", type = "String", required = true }]
"#,
    )
    .unwrap();

    let compiled =
        compile(SchemaMerger::merge_toml_only(&as_str(&dir.path().join("fraiseql.toml"))).unwrap());

    let mutation = compiled
        .mutations
        .iter()
        .find(|m| m.name == "create_user" || m.name == "createUser")
        .expect("mutation create_user must be compiled");

    assert!(
        mutation.arguments.iter().any(|a| a.name == "email"),
        "TOML-declared mutation argument `email` is absent from the compiled mutation \
         (arguments = {:?})",
        mutation.arguments.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
}

// ===========================================================================
// #848 — `is_input` types compile as input types
// ===========================================================================

/// Four SDKs let an author mark a type as a GraphQL input and emit `is_input: true`.
///
/// Compiling it as an object type produces a **spec-violating** schema (GraphQL §3.10:
/// arguments must be input types): introspection-driven clients reject it and federation
/// composition fails.
#[test]
fn is_input_type_compiles_as_an_input_type() {
    let corpus = json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "fields": [{"name": "id", "type": "ID", "nullable": false}]
            },
            {
                "name": "CreateUserInput",
                "is_input": true,
                "fields": [{"name": "email", "type": "String", "nullable": false}]
            }
        ],
        "queries": [
            {"name": "users", "return_type": "User", "returns_list": true, "sql_source": "v_user"}
        ],
        "mutations": [
            {
                "name": "createUser",
                "return_type": "User",
                "sql_source": "fn_create_user",
                "operation": "INSERT",
                "arguments": [{"name": "input", "type": "CreateUserInput", "nullable": false}]
            }
        ]
    });

    let intermediate: IntermediateSchema = serde_json::from_value(corpus).unwrap();
    let compiled = compile(intermediate);

    assert!(
        compiled.input_types.iter().any(|i| i.name == "CreateUserInput"),
        "a type marked `is_input: true` must compile into `input_types`, not `types`"
    );
    assert!(
        !compiled.types.iter().any(|t| t.name == "CreateUserInput"),
        "a type marked `is_input: true` must NOT also appear as an object type — a mutation \
         argument typed with it would violate GraphQL §3.10"
    );
}

// ===========================================================================
// #847 — `[inject_defaults]` reaches every operation
// ===========================================================================

/// `[inject_defaults]` is parsed by seven SDKs' `ConfigLoader`s and emitted as a
/// top-level key. It must reach every operation's injected parameters.
///
/// Its whole purpose is stamping a tenant predicate on operations the author did not
/// annotate individually; dropping it compiles a query with **no tenant filter** and no
/// diagnostic.
#[test]
fn inject_defaults_reaches_every_operation() {
    let corpus = json!({
        "types": [
            {
                "name": "Order",
                "sql_source": "v_order",
                "fields": [{"name": "id", "type": "ID", "nullable": false}]
            }
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order"}
        ],
        "mutations": [
            {"name": "createOrder", "return_type": "Order", "sql_source": "fn_create_order",
             "operation": "INSERT"}
        ],
        "inject_defaults": {
            "base": {"tenant_id": "jwt:tenant_id"},
            "queries": {"read_scope": "jwt:scope"},
            "mutations": {"actor_id": "jwt:sub"}
        }
    });

    let intermediate: IntermediateSchema = serde_json::from_value(corpus).unwrap();
    let compiled = compile(intermediate);

    let query = compiled.queries.iter().find(|q| q.name == "orders").unwrap();
    let mutation = compiled.mutations.iter().find(|m| m.name == "createOrder").unwrap();

    assert!(
        query.inject_params.contains_key("tenant_id"),
        "`[inject_defaults].base` must reach queries — got {:?}",
        query.inject_params.keys().collect::<Vec<_>>()
    );
    assert!(
        query.inject_params.contains_key("read_scope"),
        "`[inject_defaults].queries` must reach queries — got {:?}",
        query.inject_params.keys().collect::<Vec<_>>()
    );
    assert!(
        !query.inject_params.contains_key("actor_id"),
        "`[inject_defaults].mutations` must NOT leak onto queries"
    );

    assert!(
        mutation.inject_params.contains_key("tenant_id"),
        "`[inject_defaults].base` must reach mutations — got {:?}",
        mutation.inject_params.keys().collect::<Vec<_>>()
    );
    assert!(
        mutation.inject_params.contains_key("actor_id"),
        "`[inject_defaults].mutations` must reach mutations — got {:?}",
        mutation.inject_params.keys().collect::<Vec<_>>()
    );
    assert!(
        !mutation.inject_params.contains_key("read_scope"),
        "`[inject_defaults].queries` must NOT leak onto mutations"
    );
}

/// A per-operation `inject_params` entry wins over the default for the same key.
///
/// Without this, a default would silently override an explicit per-operation decision —
/// the same silent-override class, one level up.
#[test]
fn per_operation_inject_params_wins_over_inject_defaults() {
    let corpus = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order",
             "inject_params": {"tenant_id": "jwt:explicit_tenant_claim"}}
        ],
        "inject_defaults": {"base": {"tenant_id": "jwt:default_tenant_claim"}}
    });

    let intermediate: IntermediateSchema = serde_json::from_value(corpus).unwrap();
    let compiled = compile(intermediate);
    let query = compiled.queries.iter().find(|q| q.name == "orders").unwrap();

    let injected = query.inject_params.get("tenant_id").expect("tenant_id must be injected");
    assert!(
        matches!(injected, InjectedParamSource::Jwt(claim) if claim == "explicit_tenant_claim"),
        "an explicit per-operation inject_params entry must win over [inject_defaults]; the \
         default silently replaced the claim the author chose for this operation, got \
         {injected:?}"
    );
}

// ===========================================================================
// #779 — SDK-authored observers must not be validated and then discarded
// ===========================================================================

/// Observers declared in `schema.json` are validated by ~220 lines of validator and then
/// dropped by `observers: Vec::new()`, so no webhook ever fires and nothing warns.
///
/// The runtime loads observers exclusively from `tb_observer` / the admin API, so making
/// them *reach the compiled schema* would only move the silent drop one layer down. The
/// contract asserted here is therefore the honest-loud one: the compile **fails** and
/// names the supported mechanism. Wiring a real runtime consumer is tracked separately;
/// when that lands, this test inverts to assert the observers arrive.
#[test]
fn sdk_authored_observers_fail_the_compile_rather_than_vanishing() {
    let corpus = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order"}
        ],
        "observers": [
            {
                "name": "order_created",
                "entity": "Order",
                "event": "INSERT",
                "actions": [{"type": "webhook", "url": "https://example.test/hook"}],
                "retry": {"max_attempts": 3, "backoff_strategy": "exponential",
                          "initial_delay_ms": 100, "max_delay_ms": 5000}
            }
        ]
    });

    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("observers must still deserialize");
    let err = SchemaConverter::convert(intermediate)
        .expect_err("a schema declaring observers with no runtime consumer must not compile");

    let msg = err.to_string();
    assert!(
        msg.contains("tb_observer") || msg.contains("/api/observers"),
        "the refusal must name the supported mechanism so the author can act on it; got: {msg}"
    );
}

/// #631: `observers_config.handlers` arriving through the seam must fail the compile too.
///
/// The TOML path already bails on a non-empty `[[observers.handlers]]` (#612), but an
/// SDK-authored `schema.json` carries `observers_config` straight through the P14
/// pass-through seam — bypassing the TOML validator — and the handlers landed in the
/// compiled schema as decoration the runtime never reads. `tb_observer` / the admin API is
/// the single observer concept by decision (#631); every compiled-handler route must be
/// rejected with a message that names it.
#[test]
fn sdk_authored_observers_config_handlers_fail_the_compile_rather_than_vanishing() {
    let corpus = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order"}
        ],
        "observers_config": {
            "enabled": true,
            "backend": "redis",
            "handlers": [
                {"name": "notify", "event": "Order.created", "action": "webhook",
                 "webhook_url": "https://example.test/hook"}
            ]
        }
    });

    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("observers_config must still deserialize");
    let err = SchemaConverter::convert(intermediate).expect_err(
        "#631: observers_config.handlers has no runtime consumer and must not compile silently",
    );

    let msg = err.to_string();
    assert!(
        msg.contains("tb_observer") || msg.contains("/api/observers"),
        "the refusal must name the supported mechanism so the author can act on it; got: {msg}"
    );
}

/// #631 companion: an *empty* `handlers` array is inert boilerplate, not a declaration —
/// it must not fail the compile (the CLI's own TOML path and older SDK exporters emit it).
#[test]
fn empty_observers_config_handlers_still_compile() {
    let corpus = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order"}
        ],
        "observers_config": {"enabled": true, "backend": "redis", "handlers": []}
    });

    let intermediate: IntermediateSchema =
        serde_json::from_value(corpus).expect("observers_config must deserialize");
    let compiled = compile(intermediate);
    let cfg = compiled.observers_config.expect("observers_config must survive the seam");
    assert!(cfg.enabled, "enabled flag must pass through");
}

// ===========================================================================
// #780 — `[grpc]` has a producer
// ===========================================================================

/// A `[grpc]` TOML section must reach `CompiledSchema.grpc_config`.
///
/// `grpc_config` gates the server's entire gRPC transport and documented itself as "compiled
/// from `[grpc]` in `fraiseql.toml`" — but nothing in the CLI parsed such a section, and
/// `TomlSchema` is `deny_unknown_fields`, so an operator following that documentation got
/// "unknown field `grpc`". Removing the section compiled, and the server then silently never
/// mounted gRPC. Only a hand-edited `schema.compiled.json` (which additionally breaks the
/// `_content_hash`) could enable a shipped, e2e-tested transport.
#[test]
fn a_grpc_toml_section_reaches_the_compiled_schema() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "g"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[grpc]
enabled = true
descriptor_path = "api.binpb"
reflection = false
stream_batch_size = 250
include_types = ["User"]
"#,
    )
    .unwrap();

    let compiled =
        compile(SchemaMerger::merge_toml_only(&as_str(&dir.path().join("fraiseql.toml"))).unwrap());

    let grpc = compiled
        .grpc_config
        .as_ref()
        .expect("[grpc] must reach CompiledSchema.grpc_config — it is what gates the transport");

    assert!(grpc.enabled, "enabled must survive");
    assert_eq!(grpc.descriptor_path, "api.binpb", "descriptor_path must survive");
    assert!(!grpc.reflection, "a non-default reflection = false must survive");
    assert_eq!(grpc.stream_batch_size, 250, "a non-default batch size must survive");
    assert_eq!(grpc.include_types, ["User"], "include_types must survive");
}

/// `[grpc] enabled = true` without a `descriptor_path` must fail the compile.
///
/// `build_grpc_service` reads the `FileDescriptorSet` from that path, so an empty one
/// guarantees a server that cannot mount the transport it was told to enable. Failing at
/// compile time is the difference between a message naming the TOML key and a runtime file
/// read error on boot.
#[test]
fn grpc_enabled_without_a_descriptor_path_fails_the_compile() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "g"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[grpc]
enabled = true
"#,
    )
    .unwrap();

    let err = SchemaMerger::merge_toml_only(&as_str(&dir.path().join("fraiseql.toml")))
        .expect_err("gRPC enabled with no descriptor path must not compile");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("descriptor_path"),
        "the error must name the missing key; got: {msg}"
    );
}

/// A disabled `[grpc]` section must leave `grpc_config` absent.
///
/// The transport is gated on `Some(..) && enabled`, so embedding a disabled block is harmless
/// — but leaving it absent keeps the compiled artifact byte-identical for the overwhelming
/// majority of projects that never touch gRPC.
#[test]
fn a_disabled_grpc_section_leaves_the_compiled_config_absent() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        r#"
[schema]
name = "g"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"
[types.User.fields.id]
type = "ID"
nullable = false

[grpc]
enabled = false
"#,
    )
    .unwrap();

    let compiled =
        compile(SchemaMerger::merge_toml_only(&as_str(&dir.path().join("fraiseql.toml"))).unwrap());
    assert!(
        compiled.grpc_config.is_none(),
        "a disabled [grpc] section must not be embedded in the compiled schema"
    );
}

// ===========================================================================
// The class guard — an unknown key on the seam is an error, never a default
// ===========================================================================

/// A key the compiler does not read must fail the compile.
///
/// This is the guard that makes the whole class non-recurring. `#847` was seven SDKs
/// emitting `inject_defaults` into a compiler that had never heard of it, and
/// `fraiseql compile` printing `✓ Schema compiled successfully`. With
/// `deny_unknown_fields` on the boundary, the eighth such key cannot be silent.
#[test]
fn an_unknown_top_level_seam_key_fails_the_compile() {
    let mut corpus = sdk_corpus();
    corpus["totally_unknown_feature"] = json!({"enabled": true});

    let err = serde_json::from_value::<IntermediateSchema>(corpus)
        .expect_err("an unknown top-level key must not deserialize into a silent default");

    assert!(
        err.to_string().contains("totally_unknown_feature"),
        "the error must name the offending key; got: {err}"
    );
}

/// The same guard, one level down: an unknown key inside a type definition.
///
/// `#848`'s `is_input` lived here — an unread key on `IntermediateType`. Now that
/// `is_input` is honoured, a *misspelling* of it must still be loud.
#[test]
fn an_unknown_key_inside_a_type_fails_the_compile() {
    let corpus = json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "is_imput": true,
                "fields": [{"name": "id", "type": "ID", "nullable": false}]
            }
        ]
    });

    let err = serde_json::from_value::<IntermediateSchema>(corpus)
        .expect_err("a misspelled key inside a type must not deserialize into a default");

    assert!(
        err.to_string().contains("is_imput"),
        "the error must name the offending key; got: {err}"
    );
}
