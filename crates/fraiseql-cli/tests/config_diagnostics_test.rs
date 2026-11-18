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
