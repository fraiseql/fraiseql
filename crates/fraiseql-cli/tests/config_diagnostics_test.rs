#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! A refused configuration, through the real binary: exit code 2 ("input refused") and,
//! under `--json`, a diagnostic naming the code, the file, the table, the key and the
//! replacement.
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
                     "sql_source": "v_order"}]
    })
}

struct Refused {
    code:   Option<i32>,
    stderr: String,
}

impl Refused {
    /// The `error` object of the JSON printed on stderr under `--json`.
    fn error(&self) -> Value {
        let line =
            self.stderr.lines().rev().find(|l| l.get(..1) == Some("{")).unwrap_or_else(|| {
                panic!("no JSON error on stderr: {}", self.stderr);
            });
        serde_json::from_str::<Value>(line).unwrap()["error"].clone()
    }
}

/// `fraiseql [--json] compile` in a project whose `fraiseql.toml` is `toml`, read beside a
/// schema document.
fn compile_beside_a_document(toml: &str, json: bool) -> Refused {
    compile_beside(toml, &json!({}), json)
}

/// [`compile_beside_a_document`], the document carrying `extra`'s keys.
fn compile_beside(toml: &str, extra: &Value, json: bool) -> Refused {
    let dir = TempDir::new().unwrap();
    let mut document = document();
    for (key, value) in extra.as_object().into_iter().flatten() {
        document[key] = value.clone();
    }
    fs::write(dir.path().join("schema.json"), document.to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"));
    if json {
        command.arg("--json");
    }
    let out = command
        .args(["compile", "schema.json", "--output", "schema.compiled.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Refused {
        code:   out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn a_refused_configuration_exits_2() {
    let refused = compile_beside_a_document("[schema]\nname = \"app\"\n", false);
    assert_eq!(refused.code, Some(2), "{}", refused.stderr);
}

#[test]
fn a_refused_configuration_names_its_code_file_table_and_replacement_under_json() {
    let refused = compile_beside_a_document("[schema]\nname = \"app\"\n", true);
    let error = refused.error();
    assert_eq!(error["code"], 2, "{error}");
    let diagnostic = &error["diagnostic"];
    assert_eq!(diagnostic["code"], "E-CONFIG-RETIRED", "{error}");
    assert!(
        diagnostic["file"].as_str().is_some_and(|f| f.ends_with("fraiseql.toml")),
        "{error}"
    );
    assert_eq!(diagnostic["table"], "schema", "{error}");
    assert_eq!(diagnostic["key"], Value::Null, "{error}");
    assert_eq!(diagnostic["replacement"], "`[project]`", "{error}");
}

/// A refusal, by the site that raises it: the `fraiseql.toml`, what the document beside it
/// carries, a phrase of the refusal's own text (so the input is refused by that site), and
/// the diagnostic it must carry: code, table, key, replacement.
struct Case {
    site:        &'static str,
    toml:        &'static str,
    document:    Value,
    phrase:      &'static str,
    code:        &'static str,
    table:       &'static str,
    key:         Option<&'static str>,
    replacement: Option<&'static str>,
    belongs_in:  Option<&'static str>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            site:        "retired spelling, renamed key",
            toml:        "[security.rate_limiting]\nfailed_login_max_requests = 5\n",
            document:    json!({}),
            phrase:      "it is spelled `failed_login_max_attempts`",
            code:        "E-CONFIG-RETIRED",
            table:       "security.rate_limiting",
            key:         Some("failed_login_max_requests"),
            replacement: Some("`failed_login_max_attempts`"),
            belongs_in:  None,
        },
        Case {
            site:        "retired spelling nothing reads",
            toml:        "[project]\nname = \"app\"\ndescription = \"An app\"\n",
            document:    json!({}),
            phrase:      "nothing reads it",
            code:        "E-CONFIG-UNREAD",
            table:       "project",
            key:         Some("description"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "key `fraiseql run` never applied",
            toml:        "[server]\nkeep_alive_secs = 5\n",
            document:    json!({}),
            phrase:      "was never applied by `fraiseql run`",
            code:        "E-CONFIG-UNAPPLIED",
            table:       "server",
            key:         Some("keep_alive_secs"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "schema-authoring table beside a document",
            toml:        "[types.Order]\nsql_source = \"v_order\"\n",
            document:    json!({}),
            phrase:      "does not belong in a project configuration",
            code:        "E-CONFIG-MISPLACED",
            table:       "types",
            key:         None,
            replacement: None,
            belongs_in:  Some("the schema document"),
        },
        Case {
            site:        "server-file spelling fraiseql.toml spells otherwise",
            toml:        "bind_addr = \"0.0.0.0:8000\"\n",
            document:    json!({}),
            phrase:      "is how the server's configuration file",
            code:        "E-CONFIG-MISPLACED",
            table:       "",
            key:         Some("bind_addr"),
            replacement: Some("`[server] host` and `port`"),
            belongs_in:  Some("the server's configuration file (the file FRAISEQL_CONFIG names)"),
        },
        Case {
            site:        "server-file setting fraiseql.toml does not make",
            toml:        "[tenancy]\nruntime = {}\n",
            document:    json!({}),
            phrase:      "is read from the server's configuration file",
            code:        "E-CONFIG-MISPLACED",
            table:       "tenancy",
            key:         Some("runtime"),
            replacement: None,
            belongs_in:  Some("the server's configuration file (the file FRAISEQL_CONFIG names)"),
        },
        Case {
            site:        "runtime section and the document's copy",
            toml:        "[mcp]\nenabled = true\n",
            document:    json!({"mcp_config": {"enabled": false}}),
            phrase:      "disagree on",
            code:        "E-CONFIG-DISAGREES",
            table:       "mcp",
            key:         Some("enabled"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[functions] and the document's copy",
            toml:        "[functions]\nmodule_dir = \"functions\"\n",
            document:    json!({"functions_config": {"module_dir": "elsewhere"}}),
            phrase:      "functions_config disagree on",
            code:        "E-CONFIG-DISAGREES",
            table:       "functions",
            key:         Some("module_dir"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[observers] and the document's copy",
            toml:        "[observers]\nenabled = false\n",
            document:    json!({"observers_config": {"enabled": true}}),
            phrase:      "observers_config disagree on",
            code:        "E-CONFIG-DISAGREES",
            table:       "observers",
            key:         Some("enabled"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[locale] and the document's locale",
            toml:        "[locale]\ndefault = \"en\"\nallowed = [\"en\"]\n",
            document:    json!({"locale": {"default": "fr", "allowed": ["fr"]}}),
            phrase:      "disagrees with the `locale`",
            code:        "E-CONFIG-DISAGREES",
            table:       "locale",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[security] and the document's security",
            toml:        "[tenancy]\nmode = \"none\"\n",
            document:    json!({"security": {
                "tenancy": {"mode": "row", "tenant_claim": "org_id"},
                "rls": {"enabled": true},
                "multi_tenant": true
            }}),
            phrase:      "both set security `tenancy`",
            code:        "E-CONFIG-DISAGREES",
            table:       "tenancy",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[[caching.rules]] and the query's own TTL",
            toml:        "[caching]\nenabled = true\n\n[[caching.rules]]\nquery = \"orders\"\nttl_seconds = 60\ninvalidation_triggers = []\n",
            document:    json!({"queries": [{"name": "orders", "return_type": "Order",
                                             "returns_list": true, "sql_source": "v_order",
                                             "cache_ttl_seconds": 30}]}),
            phrase:      "already declares cache_ttl_seconds",
            code:        "E-CONFIG-DISAGREES",
            table:       "caching.rules",
            key:         Some("ttl_seconds"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[rest] key removed with its replacement",
            toml:        "[rest]\nmax_embedded_reads = 5\n",
            document:    json!({}),
            phrase:      "max_embedded_reads was removed",
            code:        "E-CONFIG-RETIRED",
            table:       "rest",
            key:         Some("max_embedded_reads"),
            replacement: Some("`[security.cost_budget] per_request_max`"),
            belongs_in:  None,
        },
        Case {
            site:        "[observers] redis_url nothing reads",
            toml:        "[observers]\nredis_url = \"redis://cache:6379\"\n",
            document:    json!({}),
            phrase:      "configures nothing",
            code:        "E-CONFIG-UNREAD",
            table:       "observers",
            key:         Some("redis_url"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[[observers.handlers]] nothing runs",
            toml:        "[[observers.handlers]]\nname = \"h\"\nevent = \"order.created\"\naction = \"webhook\"\n",
            document:    json!({}),
            phrase:      "[[observers.handlers]] is not supported",
            code:        "E-CONFIG-UNREAD",
            table:       "observers.handlers",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[caching] redis_url nothing reads",
            toml:        "[caching]\nredis_url = \"redis://cache:6379\"\n",
            document:    json!({}),
            phrase:      "no Redis-backed result cache",
            code:        "E-CONFIG-UNREAD",
            table:       "caching",
            key:         Some("redis_url"),
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[[security.rules]] the runtime does not enforce",
            toml:        "[[security.rules]]\nname = \"r\"\nrule = \"true\"\n",
            document:    json!({}),
            phrase:      "the FraiseQL runtime does NOT enforce",
            code:        "E-CONFIG-UNREAD",
            table:       "security.rules",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[[security.policies]] the runtime does not enforce",
            toml:        "[[security.policies]]\nname = \"p\"\ntype = \"rbac\"\n",
            document:    json!({}),
            phrase:      "the FraiseQL runtime does NOT enforce",
            code:        "E-CONFIG-UNREAD",
            table:       "security.policies",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[[security.field_auth]] the runtime does not enforce",
            toml:        "[[security.field_auth]]\ntype_name = \"Order\"\nfield_name = \"total\"\npolicy = \"p\"\n",
            document:    json!({}),
            phrase:      "the FraiseQL runtime does NOT enforce",
            code:        "E-CONFIG-UNREAD",
            table:       "security.field_auth",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[inject_defaults] and the document's copy",
            toml:        "[inject_defaults]\ntenant_id = \"jwt:tenant_id\"\n",
            document:    json!({"inject_defaults": {"base": {"tenant_id": "jwt:org_id"}}}),
            phrase:      "differs from `inject_defaults` in the",
            code:        "E-CONFIG-DISAGREES",
            table:       "inject_defaults",
            key:         None,
            replacement: None,
            belongs_in:  None,
        },
        Case {
            site:        "[auth.local.email] compiled body written inline",
            toml:        "[auth.local.email.otp]\ntext = \"Your code\"\n",
            document:    json!({}),
            phrase:      "text is the compiled body",
            code:        "E-CONFIG-MISPLACED",
            table:       "auth.local.email.otp",
            key:         Some("text"),
            replacement: Some("`text_template`"),
            belongs_in:  None,
        },
    ]
}

#[test]
fn each_refusal_site_carries_its_code_table_key_and_replacement() {
    let mut wrong = Vec::new();
    for case in cases() {
        let refused = compile_beside(case.toml, &case.document, true);
        let error = refused.error();
        let diagnostic = &error["diagnostic"];
        let got = json!({
            "exit": refused.code,
            "code": diagnostic["code"],
            "table": diagnostic["table"],
            "key": diagnostic["key"],
            "replacement": diagnostic["replacement"],
            "belongs_in": diagnostic["belongs_in"],
            "file_is_the_toml": diagnostic["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("fraiseql.toml")),
        });
        let want = json!({
            "exit": 2,
            "code": case.code,
            "table": case.table,
            "key": case.key,
            "replacement": case.replacement,
            "belongs_in": case.belongs_in,
            "file_is_the_toml": true,
        });
        if got != want {
            wrong.push(format!("{}:\n  got  {got}\n  want {want}", case.site));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The phrase check of [`each_refusal_site_carries_its_code_table_key_and_replacement`],
/// alone for every site: each input is refused by the site it names. Kept separate so a
/// site that stops refusing reads as that, not as a missing diagnostic.
#[test]
fn each_case_is_refused_by_the_site_it_names() {
    let unrefused: Vec<String> = cases()
        .into_iter()
        .filter_map(|case| {
            let refused = compile_beside(case.toml, &case.document, false);
            (!refused.stderr.contains(case.phrase))
                .then(|| format!("{}: {}", case.site, refused.stderr))
        })
        .collect();
    assert!(unrefused.is_empty(), "{unrefused:#?}");
}

/// The TOML schema as the compile input names its file too: that loader is not the one the
/// file beside a document goes through.
#[test]
fn a_refused_toml_schema_input_names_its_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), document().to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), "[schema]\nname = \"app\"\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "--json",
            "compile",
            "fraiseql.toml",
            "--types",
            "types.json",
        ])
        .args(["--output", "schema.compiled.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let refused = Refused {
        code:   out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    let error = refused.error();
    assert_eq!(refused.code, Some(2), "{error}");
    assert_eq!(error["diagnostic"]["code"], "E-CONFIG-RETIRED", "{error}");
    assert_eq!(error["diagnostic"]["file"], "fraiseql.toml", "{error}");
}

/// `fraiseql --json compile fraiseql.toml --types types.json`: the TOML file as the compile
/// input, a role in which it declares queries.
fn compile_toml_input(toml: &str) -> Refused {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), document().to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "--json",
            "compile",
            "fraiseql.toml",
            "--types",
            "types.json",
        ])
        .args(["--output", "schema.compiled.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    Refused {
        code:   out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A query named `defaults` is refused as the misspelt `[query_defaults]` it most likely is.
#[test]
fn a_query_named_defaults_is_refused_as_misplaced() {
    let refused = compile_toml_input("[queries.defaults]\nreturn_type = \"User\"\n");
    let error = refused.error();
    let diagnostic = &error["diagnostic"];
    assert!(error.to_string().contains("under [queries.defaults]"), "{error}");
    assert_eq!(refused.code, Some(2), "{error}");
    assert_eq!(diagnostic["code"], "E-CONFIG-MISPLACED", "{error}");
    assert_eq!(diagnostic["table"], "queries", "{error}");
    assert_eq!(diagnostic["key"], "defaults", "{error}");
    assert_eq!(diagnostic["replacement"], "`[query_defaults]`", "{error}");
    assert_eq!(diagnostic["file"], "fraiseql.toml", "{error}");
}

/// A failure that is not a refused configuration keeps exit 1 and carries no diagnostic:
/// a missing input, and a schema document that is not JSON.
#[test]
fn a_failure_that_is_not_a_configuration_refusal_exits_1_without_a_diagnostic() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), "{ not json").unwrap();
    for input in ["missing.json", "schema.json"] {
        let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
            .args([
                "--json",
                "compile",
                input,
                "--output",
                "schema.compiled.json",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let failed = Refused {
            code:   out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        };
        let error = failed.error();
        assert_eq!(failed.code, Some(1), "{input}: {error}");
        assert_eq!(error["code"], 1, "{input}: {error}");
        assert!(error.get("diagnostic").is_none(), "{input}: {error}");
    }
}

// ── the server's configuration file, read by `fraiseql run` ─────────────────────────

/// `fraiseql --json run` in a project with a schema document, `FRAISEQL_CONFIG` naming a
/// server file that is `server_toml`. The server file is read before any database is
/// reached, and no database URL is given, so nothing is served.
#[cfg(feature = "run-server")]
fn run_with_server_file(server_toml: &str) -> Refused {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("schema.json"), document().to_string()).unwrap();
    fs::write(dir.path().join("server.toml"), server_toml).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["--json", "run", "schema.json"])
        .env("FRAISEQL_CONFIG", dir.path().join("server.toml"))
        .env_remove("DATABASE_URL")
        .env_remove("FRAISEQL_DATABASE_URL")
        .current_dir(dir.path())
        .output()
        .unwrap();
    Refused {
        code:   out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A `fraiseql.toml` spelling in the server's file is refused by `fraiseql run` with the
/// code and exit a `fraiseql.toml` refusal has, naming the server's file.
#[cfg(feature = "run-server")]
#[test]
fn run_refuses_a_fraiseql_toml_spelling_in_the_server_file_as_misplaced() {
    let refused = run_with_server_file("[tls]\ncert_file = \"cert.pem\"\n");
    let error = refused.error();
    assert!(format!("{error}").contains("this file spells it `cert_path`"), "{error}");
    assert_eq!(refused.code, Some(2), "{error}");
    let diagnostic = &error["diagnostic"];
    assert_eq!(diagnostic["code"], "E-CONFIG-MISPLACED", "{error}");
    assert_eq!(diagnostic["table"], "tls", "{error}");
    assert_eq!(diagnostic["key"], "cert_file", "{error}");
    assert_eq!(diagnostic["replacement"], "`cert_path`", "{error}");
    assert!(
        diagnostic["file"].as_str().is_some_and(|f| f.ends_with("server.toml")),
        "{error}"
    );
}

/// A key the server's file does not take is refused by `fraiseql run` as
/// `E-CONFIG-UNKNOWN-KEY`, with the accepted spelling nearest to it, naming the server's file.
#[cfg(feature = "run-server")]
#[test]
fn run_refuses_an_unknown_key_in_the_server_file_with_its_nearest_spelling() {
    let refused = run_with_server_file(
        "[tls]
cert_pth = \"cert.pem\"\n",
    );
    let error = refused.error();
    assert_eq!(refused.code, Some(2), "{error}");
    let diagnostic = &error["diagnostic"];
    assert_eq!(diagnostic["code"], "E-CONFIG-UNKNOWN-KEY", "{error}");
    assert_eq!(diagnostic["table"], "tls", "{error}");
    assert_eq!(diagnostic["key"], "cert_pth", "{error}");
    assert_eq!(diagnostic["replacement"], "`cert_path`", "{error}");
    assert!(
        diagnostic["file"].as_str().is_some_and(|f| f.ends_with("server.toml")),
        "{error}"
    );
}

/// Each kind of refusal note the server's file gets carries its diagnostic: a grouping
/// table it never had, and a section it no longer takes.
#[cfg(feature = "run-server")]
#[test]
fn run_refuses_each_kind_of_server_file_spelling_with_its_code() {
    let cases = [
        ("[cors]\nenabled = true\n", "E-CONFIG-MISPLACED", "cors", "`cors_enabled`"),
        (
            "[auth_hs256]\nsecret = \"s\"\n",
            "E-CONFIG-RETIRED",
            "auth_hs256",
            "`[auth_issuer]`",
        ),
    ];
    for (toml, code, table, names) in cases {
        let refused = run_with_server_file(toml);
        let error = refused.error();
        assert_eq!(refused.code, Some(2), "{toml}: {error}");
        let diagnostic = &error["diagnostic"];
        assert_eq!(diagnostic["code"], code, "{toml}: {error}");
        assert_eq!(diagnostic["table"], table, "{toml}: {error}");
        assert_eq!(diagnostic["key"], Value::Null, "{toml}: {error}");
        assert!(
            diagnostic["replacement"].as_str().is_some_and(|r| r.contains(names)),
            "{toml}: {error}"
        );
    }
}

/// Two refused spellings in one server file: the diagnostic is the first the loader checks,
/// a table the file never had before a `fraiseql.toml` spelling, whatever order the file
/// writes them in. Every note is still printed.
#[cfg(feature = "run-server")]
#[test]
fn the_first_refusal_the_server_loader_checks_sets_the_diagnostic() {
    let refused =
        run_with_server_file("[database_tls]\nssl_mode = \"require\"\n\n[cors]\nenabled = true\n");
    let error = refused.error();
    let text = format!("{error}");
    assert!(text.contains("postgres_ssl_mode") && text.contains("cors_enabled"), "{error}");
    assert_eq!(error["diagnostic"]["table"], "cors", "{error}");
    assert_eq!(error["diagnostic"]["key"], Value::Null, "{error}");
}

// ── `fraiseql doctor` ────────────────────────────────────────────────────────────

/// The `TOML syntax valid` check of `doctor --json --config fraiseql.toml`, the file being
/// `toml`.
fn doctor_config_check(toml: &str) -> Value {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["doctor", "--json", "--config", "fraiseql.toml"])
        .env_remove("DATABASE_URL")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let checks: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    checks
        .as_array()
        .and_then(|checks| checks.iter().find(|c| c["name"] == "TOML syntax valid"))
        .cloned()
        .unwrap_or_else(|| panic!("no config check: {stdout}"))
}

/// `doctor --json` reports a refused `fraiseql.toml` on its config check with the same
/// diagnostic `compile` gives, the file named; the check still fails.
#[test]
fn doctor_json_carries_the_refusal_on_its_config_check() {
    let check = doctor_config_check("[schema]\nname = \"app\"\n");
    assert_eq!(check["status"], "fail", "{check}");
    let diagnostic = &check["diagnostic"];
    assert_eq!(diagnostic["code"], "E-CONFIG-RETIRED", "{check}");
    assert_eq!(diagnostic["table"], "schema", "{check}");
    assert_eq!(diagnostic["replacement"], "`[project]`", "{check}");
    assert_eq!(diagnostic["file"], "fraiseql.toml", "{check}");
}

/// The config check's hint says what to do about the refusal, by its code; only a file
/// that is not TOML is told to fix its syntax.
#[test]
fn doctor_hints_by_the_refusal_it_failed_on() {
    let cases = [
        ("[schema]\nname = \"app\"\n", "write `[project]` instead of `[schema]`"),
        (
            "[project]\nname = \"app\"\ndescription = \"An app\"\n",
            "nothing reads `[project] description`; remove it",
        ),
        (
            "[server]\nkeep_alive_secs = 5\n",
            "`fraiseql run` never applied `[server] keep_alive_secs`; remove it from fraiseql.toml",
        ),
        (
            "bind_addr = \"0.0.0.0:8000\"\n",
            "`bind_addr` belongs in the server's configuration file (the file FRAISEQL_CONFIG \
             names); here, write `[server] host` and `port`",
        ),
        (
            "[tenancy]\nruntime = {}\n",
            "`[tenancy] runtime` belongs in the server's configuration file (the file \
             FRAISEQL_CONFIG names)",
        ),
        ("[project\nname = \"app\"\n", "Fix TOML syntax in fraiseql.toml and retry"),
    ];
    let wrong: Vec<String> = cases
        .into_iter()
        .filter_map(|(toml, hint)| {
            let check = doctor_config_check(toml);
            (check["hint"] != hint)
                .then(|| format!("{toml:?}:\n  got  {}\n  want {hint}", check["hint"]))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ── what is documented ───────────────────────────────────────────────────────────

/// `docs/architecture/config-vs-settings.md`, where the codes and exit codes are listed.
fn documentation() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/architecture/config-vs-settings.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A row of a Markdown table's body: not its header (`| Code |`) or its rule.
fn is_body_row(line: &str) -> bool {
    line.get(..2) == Some("| ") && line.get(..6) != Some("| Code") && !line.contains("---")
}

/// The first cell of every row of the table under `heading`, backticks stripped.
fn first_cells_under(doc: &str, heading: &str) -> Vec<String> {
    let section = doc.split(heading).nth(1).unwrap_or_else(|| panic!("no {heading:?}"));
    // Up to the next heading, of any level.
    let section = section.split("\n#").next().unwrap_or_default();
    section
        .lines()
        .filter(|line| is_body_row(line))
        .filter_map(|line| line.split('|').nth(1))
        .map(|cell| cell.trim().trim_matches('`').to_owned())
        .collect()
}

/// Every diagnostic code is documented, and every documented code is one the CLI emits.
#[test]
fn the_documented_diagnostic_codes_are_the_codes() {
    let documented = first_cells_under(&documentation(), "\n### Diagnostic Codes\n");
    let emitted: Vec<String> = fraiseql_core::utils::config_diagnostic::DiagnosticCode::ALL
        .iter()
        .map(|code| code.as_str().to_owned())
        .collect();
    assert_eq!(documented, emitted);
}

/// The exit codes `--help` prints, the ones `--help-json` lists, and the documented ones
/// are the same codes with the same descriptions.
#[test]
fn the_exit_codes_are_documented_alike_everywhere() {
    let help = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli")).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    let printed: Vec<(i64, String)> = help
        .split("EXIT CODES:")
        .nth(1)
        .unwrap_or_else(|| panic!("no EXIT CODES: {help}"))
        .lines()
        .filter_map(|line| {
            let (code, rest) = line.trim().split_once(char::is_whitespace)?;
            let description = rest.split_once(" - ")?.1.trim().to_owned();
            Some((code.parse().ok()?, description))
        })
        .collect();

    let json = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .arg("--help-json")
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&json.stdout).unwrap();
    let listed: Vec<(i64, String)> = json["data"]["exit_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["code"].as_i64().unwrap(), e["description"].as_str().unwrap().to_owned()))
        .collect();

    let doc = documentation();
    let section = doc
        .split("\n### Exit Codes\n")
        .nth(1)
        .unwrap_or_else(|| panic!("no Exit Codes"));
    let documented: Vec<(i64, String)> = section
        .split("\n#")
        .next()
        .unwrap_or_default()
        .lines()
        .filter(|line| is_body_row(line))
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            Some((cells.get(1)?.parse().ok()?, (*cells.get(2)?).to_owned()))
        })
        .collect();

    assert_eq!(printed, listed, "--help and --help-json");
    assert_eq!(documented, listed, "the documentation and --help-json");
}

// ── unknown keys ─────────────────────────────────────────────────────────────────

/// A key `fraiseql.toml` does not take is refused with its own code, where it was written,
/// and the accepted spelling nearest to it.
#[test]
fn an_unknown_key_is_refused_with_its_nearest_spelling() {
    let refused = compile_beside_a_document("[project]\nname = \"app\"\nnmae = \"app\"\n", true);
    let error = refused.error();
    let diagnostic = &error["diagnostic"];
    assert_eq!(refused.code, Some(2), "{error}");
    assert_eq!(diagnostic["code"], "E-CONFIG-UNKNOWN-KEY", "{error}");
    assert_eq!(diagnostic["table"], "project", "{error}");
    assert_eq!(diagnostic["key"], "nmae", "{error}");
    assert_eq!(diagnostic["replacement"], "`name`", "{error}");
    assert!(
        diagnostic["file"].as_str().is_some_and(|f| f.ends_with("fraiseql.toml")),
        "{error}"
    );
}

/// An unknown key nowhere near an accepted one is refused with nothing to write instead, and
/// one inside an array of tables is placed in that array's table.
#[test]
fn an_unknown_key_with_no_near_spelling_names_no_replacement() {
    let refused = compile_beside_a_document(
        "[caching]\nenabled = true\n\n[[caching.rules]]\nquery = \"orders\"\nttl_seconds = 60\n\
         invalidation_triggers = []\ncompletely_unrelated = 1\n",
        true,
    );
    let error = refused.error();
    let diagnostic = &error["diagnostic"];
    assert_eq!(refused.code, Some(2), "{error}");
    assert_eq!(diagnostic["code"], "E-CONFIG-UNKNOWN-KEY", "{error}");
    assert_eq!(diagnostic["table"], "caching.rules", "{error}");
    assert_eq!(diagnostic["key"], "completely_unrelated", "{error}");
    assert_eq!(diagnostic["replacement"], Value::Null, "{error}");
}

/// `fraiseql [--json] run schema.json` in a project whose `fraiseql.toml` is `project_toml`,
/// with `FRAISEQL_CONFIG` naming a server file holding `server_toml`. Returns the outcome
/// and the two paths.
#[cfg(feature = "run-server")]
fn run_with_a_server_file(project_toml: &str, server_toml: &str, json: bool) -> (Refused, TempDir) {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join("schema.json"), document().to_string()).unwrap();
    fs::write(dir.path().join("fraiseql.toml"), project_toml).unwrap();
    fs::write(dir.path().join("server.toml"), server_toml).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"));
    if json {
        command.arg("--json");
    }
    let out = command
        .args([
            "run",
            "schema.json",
            "--database",
            "postgresql://nobody@127.0.0.1:1/none",
        ])
        .env("FRAISEQL_CONFIG", dir.path().join("server.toml"))
        .env_remove("DATABASE_URL")
        .env_remove("FRAISEQL_HOST")
        .env_remove("FRAISEQL_PORT")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let refused = Refused {
        code:   out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    (refused, dir)
}

/// Each setting `fraiseql run` reads from both files: what `fraiseql.toml` writes, what the
/// server file writes, and the diagnostic's table, key and server-file key. The unit test
/// `a_setting_made_in_both_files_is_refused` pins the same ten rows to `SETTINGS_IN_BOTH`.
#[cfg(feature = "run-server")]
const SET_TWICE: [(&str, &str, &str, &str, &str); 10] = [
    (
        "[server]\nhost = \"127.0.0.1\"\n",
        "bind_addr = \"127.0.0.1:9001\"\n",
        "server",
        "host",
        "bind_addr",
    ),
    (
        "[server]\nport = 9000\n",
        "bind_addr = \"127.0.0.1:9001\"\n",
        "server",
        "port",
        "bind_addr",
    ),
    (
        "[server.cors]\norigins = [\"https://a.example\"]\n",
        "cors_origins = [\"https://b.example\"]\n",
        "server.cors",
        "origins",
        "cors_origins",
    ),
    (
        "[server.tls]\nenabled = false\n",
        "[tls]\nenabled = false\ncert_path = \"c\"\nkey_path = \"k\"\n",
        "server",
        "tls",
        "tls",
    ),
    (
        "[database]\nurl = \"postgresql://a/db\"\n",
        "database_url = \"postgresql://b/db\"\n",
        "database",
        "url",
        "database_url",
    ),
    (
        "[database]\npool_min = 1\n",
        "pool_min_size = 3\n",
        "database",
        "pool_min",
        "pool_min_size",
    ),
    (
        "[database]\npool_max = 5\n",
        "pool_max_size = 7\n",
        "database",
        "pool_max",
        "pool_max_size",
    ),
    (
        "[database]\nconnect_timeout_ms = 1000\n",
        "pool_timeout_secs = 3\n",
        "database",
        "connect_timeout_ms",
        "pool_timeout_secs",
    ),
    (
        "[database]\nssl_mode = \"disable\"\n",
        "[database_tls]\npostgres_ssl_mode = \"require\"\n",
        "database",
        "ssl_mode",
        "database_tls",
    ),
    (
        "[database.transaction_retry]\nmax_attempts = 2\n",
        "[transaction_retry]\nmax_attempts = 3\n",
        "database",
        "transaction_retry",
        "transaction_retry",
    ),
];

#[cfg(feature = "run-server")]
#[test]
fn a_setting_made_in_both_files_exits_2_and_names_both_files_and_keys_under_json() {
    for (project_toml, server_toml, table, key, server_key) in SET_TWICE {
        let (refused, dir) = run_with_a_server_file(project_toml, server_toml, true);
        assert_eq!(refused.code, Some(2), "[{table}] {key}: {}", refused.stderr);
        let error = refused.error();
        assert_eq!(error["code"], 2, "{error}");
        let diagnostic = &error["diagnostic"];
        assert_eq!(diagnostic["code"], "E-CONFIG-SET-TWICE", "[{table}] {key}: {error}");
        assert_eq!(diagnostic["table"], table, "{error}");
        assert_eq!(diagnostic["key"], key, "{error}");
        assert!(
            diagnostic["file"].as_str().is_some_and(|f| f.ends_with("fraiseql.toml")),
            "[{table}] {key}: {error}"
        );
        assert_eq!(
            diagnostic["also_in"],
            json!({"file": dir.path().join("server.toml"), "key": server_key}),
            "[{table}] {key}: {error}"
        );
        assert_eq!(diagnostic["replacement"], Value::Null, "{error}");
        assert_eq!(diagnostic["belongs_in"], Value::Null, "{error}");
    }
}

#[cfg(feature = "run-server")]
#[test]
fn a_setting_made_in_both_files_names_its_code_in_text() {
    let (project_toml, server_toml, ..) = SET_TWICE[1];
    let (refused, _dir) = run_with_a_server_file(project_toml, server_toml, false);
    assert_eq!(refused.code, Some(2), "{}", refused.stderr);
    assert!(
        refused.stderr.contains("`[server] port` in fraiseql.toml"),
        "{}",
        refused.stderr
    );
    assert!(refused.stderr.contains("diagnostic: E-CONFIG-SET-TWICE"), "{}", refused.stderr);
}

#[test]
fn a_diagnostic_of_another_kind_carries_a_null_also_in() {
    let refused = compile_beside_a_document("[schema]\nname = \"app\"\n", true);
    assert_eq!(refused.error()["diagnostic"]["also_in"], Value::Null, "{}", refused.stderr);
}
