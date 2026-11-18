#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! A `fraiseql.toml` beside an SDK `schema.json` takes the runtime sections the TOML
//! workflow takes (#1719), through the real binary.
//!
//! The TOML workflow lowers `[mcp]`, `[rest]`, `[grpc]` and the rest into the compiled
//! schema. Beside a `schema.json` the same file was read by a narrower grammar, and every
//! one of those sections was refused as an unknown field. Each section is now either
//! honoured, compiling to what the TOML workflow compiles it to and reconciled with a copy
//! the schema document may carry, or refused by name with where it belongs. A walk over the
//! TOML workflow's own sections pins which, so one added later cannot be forgotten here.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The schema an SDK emits: one type and its list query.
fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": []
    })
}

/// A runtime section the project config honours.
struct Section {
    /// The TOML table, as an author writes it in either workflow.
    toml:      &'static str,
    /// What the section needs beside it, in every case below: not part of the section.
    needs:     &'static str,
    /// Its name in a refusal: `fraiseql.toml [<table>]`.
    table:     &'static str,
    /// The key a schema document carries its copy under.
    key:       &'static str,
    /// A copy that says the same thing, spelling only what the author set.
    agreeing:  fn() -> Value,
    /// A copy that differs, and the first key it differs on.
    differing: fn() -> (Value, &'static str),
    /// The compiled keys the section decides.
    compiled:  &'static [&'static str],
}

impl Section {
    /// The section and what it needs, as one TOML text.
    fn both(&self) -> String {
        format!("{}\n{}", self.needs, self.toml)
    }
}

const SECTIONS: &[Section] = &[
    Section {
        toml:      "[mcp]\nenabled = true\n",
        needs:     "",
        table:     "mcp",
        key:       "mcp_config",
        agreeing:  || json!({"enabled": true}),
        differing: || (json!({"enabled": true, "path": "/tools"}), "path"),
        compiled:  &["mcp_config"],
    },
    Section {
        toml:      "[rest]\nenabled = true\npath = \"/api\"\n",
        needs:     "",
        table:     "rest",
        key:       "rest_config",
        agreeing:  || json!({"enabled": true, "path": "/api"}),
        differing: || (json!({"enabled": true, "path": "/v2"}), "path"),
        compiled:  &["rest_config"],
    },
    Section {
        toml:      "[grpc]\nenabled = true\ndescriptor_path = \"proto/app.binpb\"\n",
        needs:     "",
        table:     "grpc",
        key:       "grpc_config",
        agreeing:  || json!({"enabled": true, "descriptor_path": "proto/app.binpb"}),
        differing: || {
            (
                json!({"enabled": true, "descriptor_path": "proto/other.binpb"}),
                "descriptor_path",
            )
        },
        compiled:  &["grpc_config"],
    },
    Section {
        toml:      "[debug]\nenabled = true\n",
        needs:     "",
        table:     "debug",
        key:       "debug_config",
        agreeing:  || json!({"enabled": true}),
        differing: || (json!({"enabled": true, "database_explain": true}), "database_explain"),
        compiled:  &["debug_config"],
    },
    Section {
        toml:      "[validation]\nmax_query_depth = 7\n",
        needs:     "",
        table:     "validation",
        key:       "validation_config",
        agreeing:  || json!({"max_query_depth": 7}),
        differing: || (json!({"max_query_depth": 9}), "max_query_depth"),
        compiled:  &["validation_config"],
    },
    Section {
        toml:      "[subscriptions]\nmax_subscriptions_per_connection = 5\n",
        needs:     "",
        table:     "subscriptions",
        key:       "subscriptions_config",
        agreeing:  || json!({"max_subscriptions_per_connection": 5}),
        differing: || {
            (
                json!({"max_subscriptions_per_connection": 6}),
                "max_subscriptions_per_connection",
            )
        },
        compiled:  &["subscriptions_config"],
    },
    Section {
        toml:      "[federation]\nenabled = true\nservice_name = \"orders\"\n\n\
                    [[federation.entities]]\nname = \"Order\"\nkey_fields = [\"id\"]\n",
        needs:     "",
        table:     "federation",
        key:       "federation",
        agreeing:  || {
            json!({"enabled": true, "service_name": "orders", "version": "v2",
                   "entities": [{"name": "Order", "key_fields": ["id"]}]})
        },
        differing: || {
            (
                json!({"enabled": true, "service_name": "billing", "version": "v2",
                       "entities": [{"name": "Order", "key_fields": ["id"]}]}),
                "service_name",
            )
        },
        compiled:  &["federation"],
    },
    Section {
        toml:      "[changelog]\nexpose = true\n",
        needs:     "[observers]\nenabled = true\n",
        table:     "changelog",
        key:       "changelog_config",
        agreeing:  || json!({"expose": true}),
        differing: || (json!({"expose": true, "max_limit": 10}), "max_limit"),
        compiled:  &["changelog", "queries", "mutations", "types"],
    },
    Section {
        toml:      "[hierarchies.org]\ntable = \"tb_org\"\npath_column = \"path\"\n",
        needs:     "",
        table:     "hierarchies",
        key:       "hierarchies_config",
        agreeing:  || json!({"org": {"table": "tb_org", "path_column": "path"}}),
        differing: || (json!({"org": {"table": "tb_org", "path_column": "p"}}), "org.path_column"),
        compiled:  &["hierarchies_config"],
    },
    Section {
        toml:      "[query_defaults]\nlimit = false\n",
        needs:     "",
        table:     "query_defaults",
        key:       "query_defaults",
        // A document spells every default: the section has no optional key there.
        agreeing:  || {
            json!({"where_clause": true, "order_by": true, "limit": false, "offset": true,
                   "pagination_order": "identity"})
        },
        differing: || {
            (
                json!({"where_clause": true, "order_by": true, "limit": false, "offset": false,
                       "pagination_order": "identity"}),
                "offset",
            )
        },
        compiled:  &["queries"],
    },
    Section {
        toml:      "[auth]\ndiscovery_url = \"https://id.example.test/.well-known/openid-configuration\"\n\
                    client_id = \"app\"\nclient_secret_env = \"APP_SECRET\"\n\
                    server_redirect_uri = \"https://app.example.test/auth/callback\"\n",
        needs:     "",
        table:     "auth",
        key:       "auth",
        agreeing:  || {
            json!({"pkce": {
                "discovery_url": "https://id.example.test/.well-known/openid-configuration",
                "client_id": "app", "client_secret_env": "APP_SECRET",
                "server_redirect_uri": "https://app.example.test/auth/callback"}})
        },
        differing: || {
            (
                json!({"pkce": {
                    "discovery_url": "https://id.example.test/.well-known/openid-configuration",
                    "client_id": "other", "client_secret_env": "APP_SECRET",
                    "server_redirect_uri": "https://app.example.test/auth/callback"}}),
                "pkce.client_id",
            )
        },
        compiled:  &["auth"],
    },
];

/// Sections the project config takes, each with its own test file. `[caching]` is one of
/// them (`project_config_caching_test.rs`): it lowers onto operations, not a section.
const ALREADY_HONOURED: &[&str] = &[
    "caching",
    "observers",
    "functions",
    "locale",
    "inject_defaults",
    "server",
    "database",
    "project",
    "security",
    "tenancy",
    "session_variables",
    "naming",
    "cost_weights",
    "mutation_defaults",
];

/// Sections refused beside a schema document, each with what its refusal points to.
const REFUSED: &[(&str, &str, &str)] = &[
    ("types", "[types.Order]\nsql_source = \"v_order\"\n", "compile input"),
    ("queries", "[queries.orders]\nreturn_type = \"Order\"\n", "compile input"),
    ("mutations", "[mutations.touch]\nreturn_type = \"Order\"\n", "compile input"),
    ("includes", "[includes]\ntypes = [\"types/*.toml\"]\n", "compile input"),
    ("domain_discovery", "[domain_discovery]\nenabled = true\n", "compile input"),
    ("crud", "[crud]\nfunction_schema = \"app\"\n", "compile input"),
    ("analytics", "[analytics]\nenabled = true\n", "compile input"),
];

/// What a compile did: whether it succeeded, its output, and the compiled schema.
struct Compiled {
    ok:     bool,
    log:    String,
    schema: Value,
}

fn run(dir: &TempDir, args: &[&str]) -> Compiled {
    let out = dir.path().join("schema.compiled.json");
    let result = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .arg("compile")
        .args(args)
        .args(["--output", out.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let schema = fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    Compiled {
        ok: result.status.success(),
        log,
        schema,
    }
}

/// `toml` as the project config beside the schema document, which carries `copy` if given.
fn beside_a_document(toml: &str, copy: Option<(&str, Value)>) -> Compiled {
    let dir = TempDir::new().unwrap();
    let mut schema = document();
    if let Some((key, value)) = copy {
        schema[key] = value;
    }
    fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    // The section first: a top-level key written after a table would belong to the table.
    fs::write(dir.path().join("fraiseql.toml"), format!("{toml}\n[project]\nname = \"app\"\n"))
        .unwrap();
    run(&dir, &["schema.json"])
}

/// `toml` as a TOML schema, its types from the same document (`--types`), which carries
/// `copy` if given.
fn as_a_toml_schema(toml: &str, copy: Option<(&str, Value)>) -> Compiled {
    let dir = TempDir::new().unwrap();
    let mut types = document();
    if let Some((key, value)) = copy {
        types[key] = value;
    }
    fs::write(dir.path().join("types.json"), types.to_string()).unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!("{toml}\n[project]\nname = \"app\"\nversion = \"1.0.0\"\n"),
    )
    .unwrap();
    run(&dir, &["fraiseql.toml", "--types", "types.json"])
}

/// The top-level keys a grammar accepts, as the refusal of a key nothing near lists them.
fn accepted_keys(compiled: &Compiled) -> Vec<String> {
    let listed = compiled
        .log
        .split("That table takes: ")
        .nth(1)
        .unwrap_or_else(|| panic!("no field list in: {}", compiled.log));
    listed
        .lines()
        .next()
        .unwrap()
        .split(", ")
        .map(|key| key.trim().trim_matches('`').to_string())
        .filter(|key| !key.is_empty())
        .collect()
}

// ── honoured: one lowering, both workflows ───────────────────────────────────

#[test]
fn a_section_beside_a_schema_document_compiles_to_what_the_toml_workflow_compiles() {
    for section in SECTIONS {
        let reference = as_a_toml_schema(&section.both(), None);
        assert!(reference.ok, "[{}] in the TOML workflow: {}", section.table, reference.log);
        let beside = beside_a_document(&section.both(), None);
        assert!(beside.ok, "[{}] beside a schema document: {}", section.table, beside.log);

        let without = beside_a_document(section.needs, None);
        for key in section.compiled {
            assert_eq!(
                beside.schema[*key], reference.schema[*key],
                "[{}] compiles `{key}` alike in both workflows",
                section.table
            );
        }
        // And it is the section's doing: without it the same key reads otherwise.
        assert!(
            section.compiled.iter().any(|key| beside.schema[*key] != without.schema[*key]),
            "[{}] changes none of {:?}",
            section.table,
            section.compiled
        );
    }
}

#[test]
fn a_document_copy_that_agrees_with_the_section_compiles() {
    for section in SECTIONS {
        let alone = beside_a_document(&section.both(), None);
        let with_copy =
            beside_a_document(&section.both(), Some((section.key, (section.agreeing)())));

        assert!(with_copy.ok, "[{}] with an agreeing copy: {}", section.table, with_copy.log);
        for key in section.compiled {
            assert_eq!(with_copy.schema[*key], alone.schema[*key], "[{}] `{key}`", section.table);
        }
    }
}

#[test]
fn a_document_copy_that_differs_is_refused_naming_both_sources_and_the_key() {
    for section in SECTIONS {
        let (copy, key) = (section.differing)();

        let refused = beside_a_document(&section.both(), Some((section.key, copy)));

        assert!(!refused.ok, "[{}] with a differing copy compiled", section.table);
        for needle in [
            format!("fraiseql.toml [{}]", section.table),
            format!("schema.json {}", section.key),
            format!("`{key}`"),
        ] {
            assert!(
                refused.log.contains(&needle),
                "[{}] names `{needle}`: {}",
                section.table,
                refused.log
            );
        }
    }
}

/// With no table in `fraiseql.toml`, the document's own copy is the section.
#[test]
fn a_document_copy_alone_still_applies() {
    for section in SECTIONS {
        let (copy, _) = (section.differing)();
        // What the section compiles to, from the workflow that has always taken it.
        let reference = as_a_toml_schema(&section.both(), None);

        let alone = beside_a_document(section.needs, Some((section.key, (section.agreeing)())));
        let other = beside_a_document(section.needs, Some((section.key, copy)));

        assert!(alone.ok && other.ok, "[{}]: {}{}", section.table, alone.log, other.log);
        for key in section.compiled {
            assert_eq!(alone.schema[*key], reference.schema[*key], "[{}] `{key}`", section.table);
        }
    }
}

/// The checks the lowering makes are the section's, whichever file carries it.
#[test]
fn a_document_copy_passes_the_checks_the_section_passes() {
    for (key, copy, needle) in [
        ("rest_config", json!({"enabled": true, "path": "api"}), "must start with '/'"),
        (
            "grpc_config",
            json!({"enabled": true, "descriptor_path": ""}),
            "descriptor_path",
        ),
    ] {
        let refused = beside_a_document("", Some((key, copy.clone())));
        assert!(!refused.ok, "`{key}` compiled");
        assert!(refused.log.contains(needle), "`{key}`: {}", refused.log);

        // And in the types document of a TOML schema.
        let refused = as_a_toml_schema("", Some((key, copy)));
        assert!(!refused.ok, "`{key}` compiled from a types document");
        assert!(refused.log.contains(needle), "`{key}`: {}", refused.log);
    }
}

/// A section is checked beside a schema document as the TOML workflow checks it: the same
/// table is refused in both, for the same reason.
#[test]
fn a_section_is_refused_beside_a_schema_document_for_what_the_toml_workflow_refuses() {
    for (toml, needle) in [
        ("[rest]\nenabled = true\npath = \"api\"\n", "must start with '/'"),
        ("[grpc]\nenabled = true\n", "descriptor_path"),
        (
            "[hierarchies.org]\ntable = \"\"\npath_column = \"path\"\n",
            "table must not be empty",
        ),
        (
            "[federation]\nenabled = true\n\n[federation.circuit_breaker]\nfailure_threshold = 0\n",
            "failure_threshold must be greater than 0",
        ),
        ("[auth]\nclient_id = \"app\"\n", "client_id"),
        // A mail template is a file read beside the config, in both workflows.
        ("[auth.local.email.otp]\ntext_template = \"otp.txt\"\n", "cannot read"),
    ] {
        let reference = as_a_toml_schema(toml, None);
        assert!(
            !reference.ok && reference.log.contains(needle),
            "TOML workflow: {}",
            reference.log
        );

        let refused = beside_a_document(toml, None);

        assert!(!refused.ok, "compiled beside a schema document: {toml}");
        assert!(refused.log.contains(needle), "{toml}: {}", refused.log);
    }
}

// ── the TOML workflow with an external document reconciles too ──────────────

#[test]
fn a_toml_schema_and_its_types_document_are_reconciled_the_same_way() {
    for section in SECTIONS {
        let (copy, key) = (section.differing)();
        let agreeing = as_a_toml_schema(&section.both(), Some((section.key, (section.agreeing)())));
        assert!(agreeing.ok, "[{}] with an agreeing copy: {}", section.table, agreeing.log);

        let refused = as_a_toml_schema(&section.both(), Some((section.key, copy)));

        assert!(!refused.ok, "[{}] overwrote a differing copy", section.table);
        assert!(
            refused.log.contains(&format!("[{}]", section.table))
                && refused.log.contains(&format!("`{key}`")),
            "[{}]: {}",
            section.table,
            refused.log
        );
    }
}

// ── session variables: the project config's copy against the document's ─────

#[test]
fn session_variables_declared_in_both_places_must_agree() {
    let table = "[[session_variables.variables]]\nname = \"app.region\"\n\
                 source = \"literal\"\nvalue = \"eu\"\n";
    let variables = |value: &str| json!({"variables": [{"name": "app.region", "source": "literal", "value": value}]});

    let agreeing = beside_a_document(table, Some(("session_variables", variables("eu"))));
    assert!(agreeing.ok, "{}", agreeing.log);
    let refused = beside_a_document(table, Some(("session_variables", variables("us"))));

    assert!(!refused.ok, "a differing copy was replaced silently");
    for needle in [
        "fraiseql.toml [session_variables]",
        "schema.json session_variables",
    ] {
        assert!(refused.log.contains(needle), "names `{needle}`: {}", refused.log);
    }
}

// ── refused by name ──────────────────────────────────────────────────────────

#[test]
fn a_section_that_does_not_belong_beside_a_schema_document_is_refused_by_name() {
    for (name, toml, pointer) in REFUSED {
        let refused = beside_a_document(toml, None);

        assert!(!refused.ok, "`{name}` beside a schema document compiled");
        assert!(refused.log.contains(&format!("`{name}`")), "names `{name}`: {}", refused.log);
        assert!(refused.log.contains(pointer), "`{name}` points to {pointer}: {}", refused.log);
        assert!(
            !refused.log.contains("expected one of"),
            "`{name}` is refused by name, not as an unknown field: {}",
            refused.log
        );
    }
}

// ── every section of the TOML workflow is accounted for ─────────────────────

/// The TOML workflow's sections are read from its own grammar, so one added there later
/// fails here until it is honoured or refused beside a schema document.
#[test]
fn every_section_of_the_toml_workflow_is_honoured_or_refused_by_name() {
    let toml_workflow = accepted_keys(&as_a_toml_schema("not_a_section = 1\n", None));
    assert!(toml_workflow.len() > 25, "the grammar lists its sections: {toml_workflow:?}");

    let decided: Vec<&str> = SECTIONS
        .iter()
        .map(|section| section.table)
        .chain(ALREADY_HONOURED.iter().copied())
        .chain(REFUSED.iter().map(|(name, ..)| *name))
        .collect();

    let undecided: Vec<&String> =
        toml_workflow.iter().filter(|key| !decided.contains(&key.as_str())).collect();
    assert!(undecided.is_empty(), "neither honoured nor refused: {undecided:?}");
    let stale: Vec<&&str> = decided
        .iter()
        .filter(|name| !toml_workflow.iter().any(|key| key == **name))
        .collect();
    assert!(stale.is_empty(), "no longer a section of the TOML workflow: {stale:?}");
}

/// And the project config's own grammar offers every honoured section by name.
#[test]
fn the_project_config_lists_every_honoured_section() {
    let project = accepted_keys(&beside_a_document("not_a_section = 1\n", None));

    for name in SECTIONS.iter().map(|s| s.table).chain(ALREADY_HONOURED.iter().copied()) {
        assert!(project.iter().any(|key| key == name), "`{name}` in {project:?}");
    }
}

// ── declared is not the same as enabled ──────────────────────────────────────

/// A table that only turns its transport off is still the author's word on it: a document
/// that turns it on disagrees, and is not preferred.
#[test]
fn a_table_that_turns_a_transport_off_disagrees_with_a_copy_that_turns_it_on() {
    for (toml, key) in [
        ("[mcp]\nenabled = false\n", "mcp_config"),
        ("[rest]\nenabled = false\n", "rest_config"),
        ("[debug]\nenabled = false\n", "debug_config"),
    ] {
        let off = beside_a_document(toml, None);
        assert!(off.ok && off.schema[key].is_null(), "`{key}` off writes nothing: {}", off.log);

        let refused = beside_a_document(toml, Some((key, json!({"enabled": true}))));

        assert!(!refused.ok, "`{key}`: the document's copy turned it on");
        assert!(refused.log.contains("`enabled`"), "`{key}`: {}", refused.log);
    }
}

// ── the naming convention: decided by the project config when it says so ────

/// An SDK writes its own convention into every document, and `[naming] convention`
/// is the documented way to choose another: the two are not reconciled. The project config
/// decides when it writes the key, and only then.
#[test]
fn the_naming_convention_is_the_project_configs_when_declared_and_the_documents_otherwise() {
    let naming = |toml: &str, document: Option<&str>| {
        let compiled = beside_a_document(toml, document.map(|c| ("naming_convention", json!(c))));
        assert!(compiled.ok, "{}", compiled.log);
        compiled.schema["naming_convention"].clone()
    };
    let preserve = "[naming]\nconvention = \"preserve\"\n";
    let acronyms_only = "[naming]\nacronyms = [\"s3\"]\n";

    // Declared: it decides, over what the SDK wrote.
    assert_eq!(naming(preserve, Some("camelCase")), json!("preserve"));
    // Not declared: the document's own stands, with or without a `[naming]` table.
    assert_eq!(naming("", Some("preserve")), json!("preserve"));
    assert_eq!(naming(acronyms_only, Some("preserve")), json!("preserve"));
    // Neither says: the workflow's default.
    assert_eq!(naming("", None), json!("camelCase"));
}

// ── #2013: a captured table that keeps its entity document in a column ───────

/// Compile `capture` (TOML, under `[subscriptions]`) with a schema whose `Order` is
/// captured from `tb_order`, in the workflow `as_toml_schema` selects, then generate the
/// capture triggers from the compiled schema. Returns the compile and the trigger DDL.
fn captured(capture: &str, as_toml_schema: bool) -> (Compiled, String) {
    let dir = TempDir::new().unwrap();
    let mut schema = document();
    schema["types"][0]["subscribable_tables"] = json!(["tb_order"]);
    let compiled = if as_toml_schema {
        fs::write(dir.path().join("types.json"), schema.to_string()).unwrap();
        fs::write(
            dir.path().join("fraiseql.toml"),
            format!("{capture}\n[project]\nname = \"app\"\nversion = \"1.0.0\"\n"),
        )
        .unwrap();
        run(&dir, &["fraiseql.toml", "--types", "types.json"])
    } else {
        fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
        fs::write(
            dir.path().join("fraiseql.toml"),
            format!("{capture}\n[project]\nname = \"app\"\n"),
        )
        .unwrap();
        run(&dir, &["schema.json"])
    };
    let generated = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["generate-capture-triggers", "--no-function", "--schema"])
        .arg(dir.path().join("schema.compiled.json"))
        .output()
        .unwrap();
    (compiled, String::from_utf8_lossy(&generated.stdout).into_owned())
}

#[test]
fn a_capture_entry_reaches_the_triggers_generated_from_the_compiled_schema() {
    for as_toml_schema in [false, true] {
        let (compiled, ddl) = captured(
            "[subscriptions.capture.tb_order]\ndocument_column = \"data\"\n",
            as_toml_schema,
        );
        assert!(compiled.ok, "{}", compiled.log);
        assert_eq!(
            ddl.matches(
                "core.fn_entity_change_log_capture('Order', 'id', 'tenant_id', 'false', 'data');"
            )
            .count(),
            3,
            "toml schema: {as_toml_schema}\n{ddl}"
        );

        // Without the entry, the triggers are the ones generated before it existed.
        let (compiled, ddl) = captured("", as_toml_schema);
        assert!(compiled.ok, "{}", compiled.log);
        assert_eq!(
            ddl.matches("core.fn_entity_change_log_capture('Order', 'id', 'tenant_id');")
                .count(),
            3,
            "toml schema: {as_toml_schema}\n{ddl}"
        );
    }
}

#[test]
fn a_capture_entry_that_cannot_be_generated_from_is_refused_naming_it() {
    let refused = [
        // A table no type declares: the entry would be dropped unread.
        ("[subscriptions.capture.tb_ordr]\ndocument_column = \"data\"\n", "`tb_ordr`"),
        // Not a column name.
        (
            "[subscriptions.capture.tb_order]\ndocument_column = \"data->'x'\"\n",
            "`data->'x'`",
        ),
        // A key the entry does not take.
        (
            "[subscriptions.capture.tb_order]\ndocument_colum = \"data\"\n",
            "document_colum",
        ),
    ];
    for as_toml_schema in [false, true] {
        for (capture, named) in refused {
            let (compiled, _) = captured(capture, as_toml_schema);
            assert!(!compiled.ok, "compiled: {capture}");
            assert!(compiled.log.contains(named), "{capture}: {}", compiled.log);
        }
    }
}
