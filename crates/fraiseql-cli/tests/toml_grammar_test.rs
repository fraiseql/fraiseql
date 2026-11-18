#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! One grammar for every `fraiseql.toml`, through the real binary.
//!
//! A `fraiseql.toml` is either the compile input (a TOML schema, which may author types and
//! operations) or the file read beside a schema document. Both are read by one grammar: a
//! setting has one spelling, top-level, whichever role the file plays. A spelling the
//! grammar no longer takes is refused by name, naming the one it takes instead.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The schema an SDK emits: one type, its list query, and a mutation returning it.
fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false},
                              {"name": "created_at", "type": "String", "nullable": true}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": [{"name": "create_order", "return_type": "Order",
                       "sql_source": "fn_create_order", "operation": "insert"}]
    })
}

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

/// [`document`] with `extra`'s keys added.
fn document_with(extra: &Value) -> Value {
    let mut document = document();
    for (key, value) in extra.as_object().into_iter().flatten() {
        document[key] = value.clone();
    }
    document
}

/// `toml`, verbatim, as the `fraiseql.toml` beside a schema document carrying `extra`.
fn beside_a_document_with(toml: &str, extra: &Value) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), document_with(extra).to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    run(&dir, &["schema.json"])
}

/// `toml`, verbatim, as the compile input, its types from the same document carrying
/// `extra` (`--types`).
fn as_the_input_with(toml: &str, extra: &Value) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), document_with(extra).to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    run(&dir, &["fraiseql.toml", "--types", "types.json"])
}

/// Both roles a `fraiseql.toml` plays, labelled for assertion messages.
fn in_both_roles(toml: &str) -> [(&'static str, Compiled); 2] {
    in_both_roles_with(toml, &json!({}))
}

/// [`in_both_roles`], the document carrying `extra`.
fn in_both_roles_with(toml: &str, extra: &Value) -> [(&'static str, Compiled); 2] {
    [
        ("beside a document", beside_a_document_with(toml, extra)),
        ("as the input", as_the_input_with(toml, extra)),
    ]
}

/// `refused` failed by the named refusal of `old`, pointing at `new` (empty: nothing to
/// write instead), not by serde's unknown-field list.
fn assert_refused_by_name(role: &str, refused: &Compiled, old: &str, new: &str) {
    assert!(!refused.ok, "{role}: {old} compiled");
    assert!(refused.log.contains("is no longer accepted"), "{role}: {old}: {}", refused.log);
    assert!(
        refused.log.contains(old),
        "{role}: the refusal does not name {old}: {}",
        refused.log
    );
    assert!(refused.log.contains(new), "{role}: {old} does not name {new}: {}", refused.log);
}

// ── metadata: `[project]` ─────────────────────────────────────────────────────

#[test]
fn project_is_the_metadata_table_in_both_roles() {
    let toml = "[project]\nname = \"app\"\nversion = \"1.2.0\"\ndatabase_target = \"postgresql\"\n";
    for (role, compiled) in in_both_roles(toml) {
        assert!(compiled.ok, "{role}: {}", compiled.log);
        assert!(compiled.schema["types"].is_array(), "{role}: no artifact: {}", compiled.log);
    }
}

#[test]
fn project_database_target_is_checked_in_both_roles() {
    let toml = "[project]\nname = \"app\"\ndatabase_target = \"mysql\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert!(!refused.ok, "{role}: a mysql target compiled");
        assert!(refused.log.contains("[project]"), "{role}: {}", refused.log);
        assert!(refused.log.contains("database_target"), "{role}: {}", refused.log);
    }
}

#[test]
fn the_schema_table_is_refused_naming_project() {
    let toml = "[schema]\nname = \"app\"\nversion = \"1.0.0\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert_refused_by_name(role, &refused, "[schema]", "[project]");
    }
}

#[test]
fn project_description_is_refused_as_read_by_nothing() {
    let toml = "[project]\nname = \"app\"\ndescription = \"An app\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert_refused_by_name(role, &refused, "[project] description", "nothing reads it");
    }
}

// ── the settings only a project config took ─────────────────────────────────

/// A service account and the role it runs as, under `{S}`.
const SERVICE_ACCOUNT: &str = "[{S}.service_accounts.gateway]\nsecret_env = \"GATEWAY_SECRET\"\n\
                               roles = [\"reader\"]\n[[{S}.role_definitions]]\nname = \"reader\"\n\
                               scopes = [\"read:*\"]\n";

/// Each setting only a project config took, in its one spelling, and the compiled values
/// (JSON pointers) both roles must agree on.
fn project_only() -> Vec<(String, &'static [&'static str])> {
    vec![
        (
            "[naming]\nconvention = \"preserve\"\nacronyms = [\"co2\"]\n".into(),
            &["/naming_convention", "/naming_acronyms"],
        ),
        ("[cost_weights]\norders = 7\n".into(), &["/operation_cost_weights"]),
        (
            "[mutation_defaults]\nauto_error_union = true\n".into(),
            &["/unions", "/mutations/0/return_type"],
        ),
        (
            SERVICE_ACCOUNT.replace("{S}", "security"),
            &["/security/service_accounts", "/security/role_definitions"],
        ),
    ]
}

#[test]
fn a_project_only_setting_compiles_alike_in_both_roles() {
    for (toml, pointers) in project_only() {
        let [(_, beside), (_, as_input)] = in_both_roles(&toml);
        assert!(as_input.ok, "{toml} as the input: {}", as_input.log);
        assert!(beside.ok, "{toml} beside a document: {}", beside.log);
        for pointer in pointers {
            let compiled = &as_input.schema.pointer(pointer);
            assert!(compiled.is_some_and(|v| !v.is_null()), "{toml}: no {pointer}");
            assert_eq!(*compiled, beside.schema.pointer(pointer), "{toml}: {pointer}");
        }
    }
}

#[test]
fn the_naming_convention_key_is_refused_naming_its_table() {
    for (role, refused) in in_both_roles("naming_convention = \"preserve\"\n") {
        assert_refused_by_name(role, &refused, "naming_convention", "[naming] convention");
    }
}

#[test]
fn constant_time_is_refused_as_read_by_nothing() {
    for table in ["security.constant_time", "fraiseql.security.constant_time"] {
        let toml = format!("[{table}]\napply_to_jwt = true\n");
        for (role, refused) in in_both_roles(&toml) {
            assert_refused_by_name(role, &refused, &format!("[{table}]"), "nothing reads it");
        }
    }
}

// ── `[fraiseql.*]`: the project config's former prefix ──────────────────────

/// Each table a project config wrote under `[fraiseql]`, as written, the spelling the
/// refusal names, and what it names instead.
const FRAISEQL_PREFIX: &[(&str, &str, &str)] = &[
    (
        "[fraiseql.security.pkce]\nenabled = true\n",
        "[fraiseql.security]",
        "`[security]`",
    ),
    ("[fraiseql.tenancy]\nmode = \"row\"\n", "[fraiseql.tenancy]", "`[tenancy]`"),
    (
        "[fraiseql.session_variables]\nvariables = []\n",
        "[fraiseql.session_variables]",
        "`[session_variables]`",
    ),
    (
        "[fraiseql.naming]\nconvention = \"preserve\"\n",
        "[fraiseql.naming]",
        "`[naming]`",
    ),
    (
        "[fraiseql.cost_weights]\norders = 7\n",
        "[fraiseql.cost_weights]",
        "`[cost_weights]`",
    ),
    (
        "[fraiseql.mutations]\nauto_error_union = true\n",
        "[fraiseql.mutations]",
        "`[mutation_defaults]`",
    ),
    (
        "[fraiseql]\nschema_file = \"schema.json\"\n",
        "[fraiseql] schema_file",
        "nothing reads it",
    ),
    (
        "[fraiseql]\noutput_file = \"schema.compiled.json\"\n",
        "[fraiseql] output_file",
        "nothing reads it",
    ),
];

#[test]
fn a_fraiseql_table_is_refused_naming_its_top_level_spelling() {
    for (toml, old, new) in FRAISEQL_PREFIX {
        for (role, refused) in in_both_roles(toml) {
            assert_refused_by_name(role, &refused, old, new);
        }
    }
}

/// The spellings retired before the prefix was, now under `[security]`.
const SECURITY_RETIRED: &[(&str, &str, &str)] = &[
    (
        "[security.audit_logging]\nenabled = true\n",
        "[security.audit_logging]",
        "[security.enterprise] audit_logging_enabled",
    ),
    (
        "[security.rate_limiting]\nfailed_login_max_requests = 10\n",
        "failed_login_max_requests",
        "failed_login_max_attempts",
    ),
    (
        "[security.rate_limiting]\nfailed_login_window_secs = 900\n",
        "failed_login_window_secs",
        "failed_login_lockout_secs",
    ),
];

#[test]
fn a_retired_security_spelling_is_refused_naming_its_replacement_in_both_roles() {
    for (toml, old, new) in SECURITY_RETIRED {
        for (role, refused) in in_both_roles(toml) {
            assert_refused_by_name(role, &refused, old, new);
        }
    }
}

// ── one settings grammar in both roles ───────────────────────────────────────

#[test]
fn a_settings_section_is_taken_in_both_roles_in_one_spelling() {
    let toml = "[project]\nname = \"app\"\n[tenancy]\nmode = \"schema\"\n\
                [session_variables]\nvariables = [{name = \"app.region\", source = \"literal\", \
                value = \"eu\"}]\n[security.pkce]\nenabled = true\n\
                [security.state_encryption]\nenabled = true\nkey_env = \"STATE_KEY\"\n";
    let [(_, beside), (_, as_input)] = in_both_roles(toml);
    assert!(beside.ok, "beside a document: {}", beside.log);
    assert!(as_input.ok, "as the input: {}", as_input.log);
    for pointer in ["/security/tenancy", "/session_variables", "/security/pkce"] {
        let compiled = as_input.schema.pointer(pointer);
        assert!(compiled.is_some_and(|v| !v.is_null()), "as the input: no {pointer}");
        assert_eq!(compiled, beside.schema.pointer(pointer), "{pointer}");
    }
}

/// A table written at its defaults is declared, in both roles, and so disagrees with a
/// document copy that sets it otherwise.
#[test]
fn a_table_written_at_its_defaults_is_declared_in_both_roles() {
    let copy = json!({"mcp_config": {"enabled": true}});
    for (role, refused) in in_both_roles_with("[mcp]\nenabled = false\n", &copy) {
        assert!(!refused.ok, "{role}: the document's `mcp_config` replaced `[mcp]` in silence");
        assert!(refused.log.contains("mcp"), "{role}: {}", refused.log);
    }
}

/// A service account that grants nothing is refused in both roles, not compiled into a
/// credential with no authority.
#[test]
fn a_service_account_with_no_ceiling_is_refused_in_both_roles() {
    let toml = "[security.service_accounts.ghost]\nsecret_env = \"GHOST_SECRET\"\n";
    for (role, refused) in in_both_roles(toml) {
        assert!(!refused.ok, "{role}: compiled");
        assert!(refused.log.contains("neither roles nor scopes"), "{role}: {}", refused.log);
    }
}

// ── `[project] name`: the schema's identity ──────────────────────────────────

/// A written `[project] name` is compiled as the schema's `project_name`, in both roles.
#[test]
fn a_written_project_name_is_compiled_in_both_roles() {
    for (role, compiled) in in_both_roles("[project]\nname = \"shop\"\n") {
        assert!(compiled.ok, "{role}: {}", compiled.log);
        assert_eq!(compiled.schema["project_name"], "shop", "{role}");
    }
}

/// An unwritten name is not compiled at its default: declared is written.
#[test]
fn an_unwritten_project_name_is_not_compiled() {
    for toml in ["", "[project]\nversion = \"1.0.0\"\n"] {
        for (role, compiled) in in_both_roles(toml) {
            assert!(compiled.ok, "{role}: {}", compiled.log);
            assert!(compiled.schema.get("project_name").is_none(), "{role}: {toml:?}");
        }
    }
}

// ── one merge: a document copy meets the file alike in both roles ───────────

/// A table the file writes, a document copy that disagrees with it, and a word the
/// refusal carries: refused in both roles, never replaced in silence.
const DISAGREEING: &[(&str, &str, &str)] = &[
    (
        "[[session_variables.variables]]\nname = \"app.region\"\nsource = \"literal\"\n\
         value = \"eu\"\n",
        r#"{"session_variables": {"variables": [{"name": "app.region", "source": "literal", "value": "us"}]}}"#,
        "session_variables",
    ),
    (
        "[locale]\ndefault = \"en-US\"\nallowed = [\"en-US\", \"fr-FR\"]\n",
        r#"{"locale": {"default": "fr-FR", "allowed": ["en-US", "fr-FR"]}}"#,
        "locale",
    ),
    (
        "[functions]\nmodule_dir = \"fns\"\n",
        r#"{"functions_config": {"module_dir": "elsewhere"}}"#,
        "module_dir",
    ),
    (
        "[observers]\nenabled = true\n",
        r#"{"observers_config": {"enabled": false}}"#,
        "observers",
    ),
];

#[test]
fn a_document_copy_that_disagrees_is_refused_in_both_roles() {
    let mut failures = Vec::new();
    for (toml, copy, word) in DISAGREEING {
        let copy: Value = serde_json::from_str(copy).unwrap();
        for (role, refused) in in_both_roles_with(toml, &copy) {
            if refused.ok {
                failures.push(format!("{role}: the copy of {word} was replaced in silence"));
            } else if !refused.log.contains(word) {
                failures.push(format!("{role}, {word}: {}", refused.log));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The document's naming convention stands where the file writes none.
#[test]
fn the_documents_naming_convention_stands_where_the_file_writes_none_in_both_roles() {
    let copy = json!({"naming_convention": "preserve"});
    for (role, compiled) in in_both_roles_with("[project]\nname = \"app\"\n", &copy) {
        assert!(compiled.ok, "{role}: {}", compiled.log);
        assert_eq!(compiled.schema["naming_convention"], "preserve", "{role}");
    }
}

/// A list query compiles alike in both roles where neither file declares
/// `[query_defaults]`.
#[test]
fn a_list_query_compiles_alike_in_both_roles_without_query_defaults() {
    let [(_, beside), (_, as_input)] = in_both_roles("[project]\nname = \"app\"\n");
    assert!(beside.ok && as_input.ok, "{}\n{}", beside.log, as_input.log);
    assert_eq!(beside.schema["queries"], as_input.schema["queries"]);
}

/// A refusal names the file the author wrote, whatever its name.
#[test]
fn a_disagreement_names_the_input_file_by_its_own_name() {
    let dir = TempDir::new().unwrap();
    let copy = json!({"security": {"multi_tenant": false}});
    fs::write(dir.path().join("types.json"), document_with(&copy).to_string()).unwrap();
    fs::write(dir.path().join("shop.toml"), "[security]\nmulti_tenant = true\n").unwrap();
    let refused = run(&dir, &["shop.toml", "--types", "types.json"]);
    assert!(!refused.ok, "the disagreement compiled");
    assert!(refused.log.contains("shop.toml and the schema document"), "{}", refused.log);
}

// ── the wrong file as the compile input ──────────────────────────────────────

/// A `fraiseql.toml` that authors nothing, given as the input with no document, is the
/// settings file of an SDK's `schema.json`: refused naming the command that was meant,
/// not compiled to an empty schema.
#[test]
fn a_settings_only_file_as_the_input_is_refused_naming_the_document() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("fraiseql.toml"), "[project]\nname = \"app\"\n").unwrap();
    let refused = run(&dir, &["fraiseql.toml"]);
    assert!(!refused.ok, "compiled an empty schema: {}", refused.log);
    assert!(refused.log.contains("authors no schema"), "{}", refused.log);
    assert!(refused.log.contains("fraiseql compile schema.json"), "{}", refused.log);
}

/// The same file with a types document is a schema, whether the document declares only
/// types or only a query.
#[test]
fn a_settings_only_file_with_a_types_document_compiles() {
    for document in [
        json!({"types": [{"name": "Order", "sql_source": "v_order",
                          "fields": [{"name": "id", "type": "ID", "nullable": false}]}]}),
        json!({"queries": [{"name": "now", "return_type": "String", "returns_list": false,
                            "sql_source": "fn_now"}]}),
    ] {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("types.json"), document.to_string()).unwrap();
        fs::write(dir.path().join("fraiseql.toml"), "[project]\nname = \"app\"\n").unwrap();
        let compiled = run(&dir, &["fraiseql.toml", "--types", "types.json"]);
        assert!(compiled.ok, "{document}: {}", compiled.log);
    }
}

// ── tables configured elsewhere ──────────────────────────────────────────────

/// `[observability]` has no reader in either role: refused by name, even written empty,
/// pointing at the keys the server's configuration file spells metrics and tracing with.
/// The server's file has no `[metrics]` or `[tracing]` table.
#[test]
fn observability_is_refused_in_both_roles_naming_the_server_keys() {
    for toml in [
        "[observability]\n",
        "[observability]\nprometheus_enabled = true\n",
    ] {
        for (role, refused) in in_both_roles(toml) {
            assert_refused_by_name(role, &refused, "[observability]", "metrics_enabled");
            assert!(refused.log.contains("tracing_enabled"), "{role}: {}", refused.log);
            assert!(!refused.log.contains("[metrics]"), "{role}: {}", refused.log);
        }
    }
}

/// `refused` failed because `spelling` is the server configuration file's, and the refusal
/// says what `fraiseql.toml` writes instead (`instead`), not serde's unknown-field list.
fn assert_refused_as_the_server_files(
    role: &str,
    refused: &Compiled,
    spelling: &str,
    instead: &str,
) {
    assert!(!refused.ok, "{role}: {spelling} compiled");
    assert!(
        refused.log.contains("the server's configuration file"),
        "{role}: {spelling}: the refusal does not name the server's file: {}",
        refused.log
    );
    assert!(
        refused.log.contains(spelling),
        "{role}: does not name {spelling}: {}",
        refused.log
    );
    assert!(
        refused.log.contains(instead),
        "{role}: {spelling} does not name {instead}: {}",
        refused.log
    );
}

/// A setting both files make, written with the server file's spelling, is refused naming the
/// `fraiseql.toml` spelling. Serde's unknown-field list names neither file.
#[test]
fn a_server_file_spelling_is_refused_naming_the_fraiseql_toml_spelling() {
    for (toml, spelling, instead) in [
        ("bind_addr = \"0.0.0.0:8080\"\n", "bind_addr", "`[server] host` and `port`"),
        ("database_url = \"postgres://db/app\"\n", "database_url", "`[database] url`"),
        ("pool_min_size = 5\n", "pool_min_size", "`[database] pool_min`"),
        ("pool_max_size = 20\n", "pool_max_size", "`[database] pool_max`"),
        (
            "pool_timeout_secs = 30\n",
            "pool_timeout_secs",
            "`[database] connect_timeout_ms`",
        ),
        ("[tls]\nenabled = true\n", "[tls]", "`[server.tls]`"),
        (
            "[database_tls]\npostgres_ssl_mode = \"require\"\n",
            "[database_tls]",
            "`[database] ssl_mode`",
        ),
        (
            "[transaction_retry]\nmax_attempts = 3\n",
            "[transaction_retry]",
            "`[database.transaction_retry]`",
        ),
        (
            "[server.tls]\ncert_path = \"/c.pem\"\n",
            "[server.tls] cert_path",
            "`cert_file`",
        ),
        ("[server.tls]\nkey_path = \"/k.pem\"\n", "[server.tls] key_path", "`key_file`"),
        (
            "[database]\npostgres_ssl_mode = \"require\"\n",
            "[database] postgres_ssl_mode",
            "`ssl_mode`",
        ),
    ] {
        for (role, refused) in in_both_roles(toml) {
            assert_refused_as_the_server_files(role, &refused, spelling, instead);
        }
    }
}

/// `[tenancy]` names a different table in each file. The server file's keys are refused
/// here naming that file, which is where they are read.
#[test]
fn the_server_files_tenancy_keys_are_refused_naming_that_file() {
    for (toml, spelling) in [
        ("[tenancy.runtime]\nenabled = true\n", "[tenancy] runtime"),
        ("[tenancy.storage_quota]\ninterval_secs = 60\n", "[tenancy] storage_quota"),
    ] {
        for (role, refused) in in_both_roles(toml) {
            assert_refused_as_the_server_files(role, &refused, spelling, "FRAISEQL_CONFIG");
        }
    }
}

/// A file that authors no type but exposes the change log compiles: the compiler injects
/// the change-log queries, so a schema results.
#[test]
fn a_file_exposing_only_the_change_log_compiles() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        "[project]\nname = \"log\"\n\n[observers]\nenabled = true\n\n[changelog]\nexpose = true\n",
    )
    .unwrap();
    let compiled = run(&dir, &["fraiseql.toml"]);
    assert!(compiled.ok, "{}", compiled.log);
}
