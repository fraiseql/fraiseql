#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! The `[security]` sections a TOML schema takes, spelled `[fraiseql.security]` in a
//! project `fraiseql.toml` beside an SDK `schema.json`, through the real binary.
//!
//! `pkce`, `api_keys`, `cost_budget`, `token_revocation`, `trusted_documents`,
//! `persisted_queries_only` and `state_encryption`'s `key_env` could be written only in a
//! TOML schema: a project config refused the first six as unknown fields, and read
//! `state_encryption` through a narrower table without `key_env`. Each now compiles in a
//! project config to what it compiles to in a TOML schema, is refused there for what a TOML
//! schema refuses, and is reconciled with a copy the schema document carries.
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

/// A `[security]` key both files take.
struct Key {
    /// Its name under `[security]`, and its compiled `security` key.
    name:      &'static str,
    /// A sub-table (`[security.<name>]`), or a scalar of `[security]`.
    table:     bool,
    /// The body an author writes, every value away from its default.
    written:   &'static str,
    /// The body at the key's defaults.
    defaults:  &'static str,
    /// A document copy that differs from `written`, as the document spells it.
    differing: fn() -> Value,
}

const KEYS: &[Key] = &[
    Key {
        name:      "pkce",
        table:     true,
        written:   "enabled = true\ncode_challenge_method = \"plain\"\nstate_ttl_secs = 300\n\
                    redis_url = \"redis://cache:6379\"\n",
        defaults:  "",
        differing: || json!({"enabled": true, "state_ttl_secs": 900}),
    },
    Key {
        name:      "api_keys",
        table:     true,
        written:   "enabled = true\nheader = \"X-Service-Key\"\nstorage = \"env\"\n\
                    [[{S}.api_keys.static]]\nkey_hash = \"sha256:abc\"\nscopes = [\"read:*\"]\n\
                    name = \"backend\"\n",
        defaults:  "",
        differing: || json!({"enabled": true, "header": "X-Other"}),
    },
    Key {
        name:      "cost_budget",
        table:     true,
        written:   "per_request_max = 500\nper_tenant_per_minute_default = 9000\n",
        defaults:  "",
        differing: || json!({"per_request_max": 501}),
    },
    Key {
        name:      "token_revocation",
        table:     true,
        written:   "enabled = true\nbackend = \"postgres\"\nrequire_jti = false\nfail_open = true\n\
                    revoke_all_ttl_secs = 172800\n",
        defaults:  "",
        differing: || json!({"enabled": true, "backend": "memory"}),
    },
    Key {
        name:      "trusted_documents",
        table:     true,
        written:   "enabled = true\nmode = \"strict\"\nmanifest_path = \"trusted.json\"\n\
                    reload_interval_secs = 30\n",
        defaults:  "",
        differing: || json!({"enabled": true, "mode": "permissive"}),
    },
    Key {
        name:      "persisted_queries_only",
        table:     false,
        written:   "persisted_queries_only = true\n",
        defaults:  "persisted_queries_only = false\n",
        differing: || json!(false),
    },
    Key {
        name:      "state_encryption",
        table:     true,
        written:   "enabled = true\nalgorithm = \"aes-256-gcm\"\nkey_env = \"OAUTH_STATE_KEY\"\n",
        defaults:  "",
        differing: || json!({"enabled": true, "key_env": "OTHER_KEY"}),
    },
];

/// The two spellings of the security table.
const PROJECT: &str = "fraiseql.security";
const TOML_SCHEMA: &str = "security";

impl Key {
    /// `body` written under this key in the table spelled `table`.
    fn spelled(&self, table: &str, body: &str) -> String {
        let body = body.replace("{S}", table);
        if self.table {
            format!("[{table}.{}]\n{body}", self.name)
        } else {
            format!("[{table}]\n{body}")
        }
    }
}

struct Compiled {
    ok:     bool,
    log:    String,
    schema: Value,
}

impl Compiled {
    fn security(&self, key: &str) -> &Value {
        &self.schema["security"][key]
    }
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

/// `toml` as the project config beside the schema document, which carries `security` if
/// given.
fn beside_a_document(toml: &str, security: Option<Value>) -> Compiled {
    let dir = TempDir::new().unwrap();
    let mut schema = document();
    if let Some(security) = security {
        schema["security"] = security;
    }
    fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    // The section first: a top-level key written after a table would belong to the table.
    fs::write(dir.path().join("fraiseql.toml"), format!("{toml}\n[project]\nname = \"app\"\n"))
        .unwrap();
    run(&dir, &["schema.json"])
}

/// `toml` as a TOML schema, its types from the same document (`--types`).
fn as_a_toml_schema(toml: &str) -> Compiled {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("types.json"), document().to_string()).unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!("{toml}\n[schema]\nname = \"app\"\nversion = \"1.0.0\"\n"),
    )
    .unwrap();
    run(&dir, &["fraiseql.toml", "--types", "types.json"])
}

// ── one lowering, both files ─────────────────────────────────────────────────

/// The compiled value is compared under the key, not as the whole `security` section: a
/// project config also emits the sections it always lowers (`enterprise`,
/// `rate_limiting`, …), which a TOML schema leaves out unless written.
#[test]
fn a_key_compiles_in_a_project_config_to_what_it_compiles_to_in_a_toml_schema() {
    for key in KEYS {
        let reference = as_a_toml_schema(&key.spelled(TOML_SCHEMA, key.written));
        assert!(reference.ok, "`{}` in a TOML schema: {}", key.name, reference.log);
        let project = beside_a_document(&key.spelled(PROJECT, key.written), None);
        assert!(project.ok, "`{}` in a project config: {}", key.name, project.log);

        assert_eq!(
            project.security(key.name),
            reference.security(key.name),
            "`{}` compiles alike in both files",
            key.name
        );
        // The fixture's doing: without it the key compiles otherwise.
        let without = beside_a_document("", None);
        assert_ne!(
            project.security(key.name),
            without.security(key.name),
            "`{}` changed nothing",
            key.name
        );
    }
}

#[test]
fn a_key_written_at_its_defaults_compiles_alike_in_both_files() {
    for key in KEYS {
        let reference = as_a_toml_schema(&key.spelled(TOML_SCHEMA, key.defaults));
        assert!(reference.ok, "`{}` defaults in a TOML schema: {}", key.name, reference.log);
        let project = beside_a_document(&key.spelled(PROJECT, key.defaults), None);
        assert!(project.ok, "`{}` defaults in a project config: {}", key.name, project.log);
        assert_eq!(
            project.security(key.name),
            reference.security(key.name),
            "`{}` at its defaults compiles alike in both files",
            key.name
        );
    }
}

// ── refused alike ────────────────────────────────────────────────────────────

/// A body each file must refuse, and the text both refusals carry.
const REFUSED: &[(&str, &str, &str)] = &[
    ("pkce", "bogus = 1\n", "unknown field `bogus`"),
    ("pkce", "code_challenge_method = \"S512\"\n", "unknown variant `S512`"),
    ("pkce", "state_ttl_secs = -1\n", "invalid value"),
    ("api_keys", "bogus = 1\n", "unknown field `bogus`"),
    ("api_keys", "header = 7\n", "invalid type"),
    (
        "api_keys",
        "enabled = true\nstorage = \"redis\"\n",
        "storage = \"redis\" is not implemented: supported values are \"env\"",
    ),
    ("cost_budget", "bogus = 1\n", "unknown field `bogus`"),
    ("cost_budget", "per_request_max = \"many\"\n", "invalid type"),
    ("token_revocation", "bogus = 1\n", "unknown field `bogus`"),
    ("token_revocation", "require_jti = \"yes\"\n", "invalid type"),
    ("trusted_documents", "bogus = 1\n", "unknown field `bogus`"),
    ("trusted_documents", "mode = \"lenient\"\n", "unknown variant `lenient`"),
    ("persisted_queries_only", "persisted_queries_only = \"yes\"\n", "invalid type"),
    ("state_encryption", "bogus = 1\n", "unknown field `bogus`"),
    ("state_encryption", "key_size = 32\n", "unknown field `key_size`"),
    (
        "state_encryption",
        "algorithm = \"rot13\"\n",
        "algorithm \"rot13\" is not supported; must be one of: chacha20-poly1305, aes-256-gcm",
    ),
];

#[test]
fn a_key_is_refused_in_a_project_config_for_what_a_toml_schema_refuses() {
    for (name, body, text) in REFUSED {
        let key = KEYS.iter().find(|key| key.name == *name).unwrap();
        let reference = as_a_toml_schema(&key.spelled(TOML_SCHEMA, body));
        let project = beside_a_document(&key.spelled(PROJECT, body), None);
        assert!(!reference.ok, "`{name}` {body:?} compiled in a TOML schema");
        assert!(!project.ok, "`{name}` {body:?} compiled in a project config");
        assert!(
            reference.log.contains(text),
            "TOML schema, `{name}` {body:?}: {}",
            reference.log
        );
        assert!(project.log.contains(text), "project config, `{name}` {body:?}: {}", project.log);
    }
}

#[test]
fn pkce_without_state_encryption_is_warned_alike_in_both_files() {
    let key = KEYS.iter().find(|key| key.name == "pkce").unwrap();
    let warning = "pkce.enabled = true but state_encryption.enabled = false";
    let reference = as_a_toml_schema(&key.spelled(TOML_SCHEMA, "enabled = true\n"));
    let project = beside_a_document(&key.spelled(PROJECT, "enabled = true\n"), None);
    assert!(reference.log.contains(warning), "TOML schema: {}", reference.log);
    assert!(project.log.contains(warning), "project config: {}", project.log);

    // Not with state encryption on.
    let both = |table: &str| {
        format!(
            "{}\n[{table}.state_encryption]\nenabled = true\n",
            key.spelled(table, "enabled = true\n")
        )
    };
    assert!(!as_a_toml_schema(&both(TOML_SCHEMA)).log.contains(warning));
    assert!(!beside_a_document(&both(PROJECT), None).log.contains(warning));
}

// ── a `[security]` table beside a schema document names the project spelling ─

#[test]
fn a_toml_schema_security_key_beside_a_document_is_told_its_project_spelling() {
    for key in KEYS {
        let refused = beside_a_document(&key.spelled(TOML_SCHEMA, key.written), None);
        assert!(!refused.ok, "`[security]` `{}` beside a schema document compiled", key.name);
        let spelling = format!("`{0}` is `[fraiseql.security]` `{0}`", key.name);
        assert!(refused.log.contains(&spelling), "`{}`: {}", key.name, refused.log);
    }
    // A key with no project spelling still says so.
    let rules = beside_a_document("[[security.rules]]\nname = \"r\"\nrule = \"true\"\n", None);
    assert!(
        rules.log.contains("`rules` has no project-configuration spelling"),
        "{}",
        rules.log
    );
}

// ── reconciled with the schema document's copy ───────────────────────────────

#[test]
fn a_document_copy_that_agrees_with_the_project_key_compiles() {
    for key in KEYS {
        let project = key.spelled(PROJECT, key.written);
        let alone = beside_a_document(&project, None);
        assert!(alone.ok, "`{}`: {}", key.name, alone.log);
        let copy = json!({ key.name: alone.security(key.name).clone() });
        let agreeing = beside_a_document(&project, Some(copy));
        assert!(agreeing.ok, "`{}` with an agreeing copy: {}", key.name, agreeing.log);
        assert_eq!(agreeing.security(key.name), alone.security(key.name));
    }
}

#[test]
fn a_document_copy_that_differs_from_the_project_key_is_refused_naming_it() {
    for key in KEYS {
        let copy = json!({ key.name: (key.differing)() });
        let refused = beside_a_document(&key.spelled(PROJECT, key.written), Some(copy));
        assert!(!refused.ok, "`{}` with a differing copy compiled", key.name);
        assert!(
            refused.log.contains(&format!("both set security `{}`", key.name)),
            "`{}`: {}",
            key.name,
            refused.log
        );
    }
}

/// A value the project config writes is the project's, even at the key's default: the
/// document cannot override `persisted_queries_only = false`.
#[test]
fn a_key_written_at_its_default_still_disagrees_with_a_document_that_sets_it() {
    let key = KEYS.iter().find(|key| key.name == "persisted_queries_only").unwrap();
    let refused = beside_a_document(
        &key.spelled(PROJECT, key.defaults),
        Some(json!({"persisted_queries_only": true})),
    );
    assert!(!refused.ok, "a written `false` was overridden by the document");
    assert!(
        refused.log.contains("both set security `persisted_queries_only`"),
        "{}",
        refused.log
    );
}

#[test]
fn a_document_copy_alone_still_applies() {
    for key in KEYS {
        let reference = as_a_toml_schema(&key.spelled(TOML_SCHEMA, key.written));
        let copy = json!({ key.name: reference.security(key.name).clone() });
        let kept = beside_a_document("", Some(copy));
        assert!(kept.ok, "`{}` from the document alone: {}", key.name, kept.log);
        assert_eq!(kept.security(key.name), reference.security(key.name), "`{}`", key.name);
    }
}
