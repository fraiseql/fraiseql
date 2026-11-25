#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! The Redis URL of `[security.rate_limiting]`, `[security.pkce]` and
//! `[security.token_revocation]`, through the real binary.
//!
//! A section names its URL either as written (`redis_url`) or by the environment variable
//! holding it (`redis_url_env`), which the server reads at startup: the compiled schema
//! carries the variable's name, never its value. A written URL holding user info or a
//! password is refused at compile, naming the key and pointing at `redis_url_env`, from
//! fraiseql.toml and from a schema document's `security` block alike.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The three sections that take a Redis URL, with the keys each needs written to be valid.
const SECTIONS: [(&str, &str); 3] = [
    ("rate_limiting", "enabled = true"),
    ("pkce", "enabled = false"),
    ("token_revocation", "enabled = true\nbackend = \"redis\""),
];

/// A value no output may show.
const PASSWORD: &str = "pw-REDIS-SENTINEL";

fn document() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "type": "ID", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": []
    })
}

/// The outcome of one compile: whether it succeeded, its output, and the compiled schema.
struct Run {
    ok:     bool,
    log:    String,
    schema: Option<Value>,
}

/// `fraiseql compile fraiseql.toml --types types.json`, with `section_toml` appended to the
/// TOML, `document_security` as the document's `security` block, and `env` set.
fn compile(section_toml: &str, document_security: Option<Value>, env: &[(&str, &str)]) -> Run {
    let dir = TempDir::new().unwrap();
    let mut doc = document();
    if let Some(security) = document_security {
        doc["security"] = security;
    }
    fs::write(dir.path().join("types.json"), doc.to_string()).unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[project]\nname = \"app\"\nversion = \"1.0.0\"\n\n{section_toml}\n"),
    )
    .unwrap();
    let out = dir.path().join("schema.compiled.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(dir.path())
        .args(["compile", "fraiseql.toml", "--types", "types.json", "-o"])
        .arg(&out)
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = fs::read_to_string(&out).ok().map(|s| serde_json::from_str(&s).unwrap());
    Run {
        ok: output.status.success(),
        log,
        schema: compiled,
    }
}

fn written(section: &str, extra: &str, redis: &str) -> String {
    format!("[security.{section}]\n{extra}\n{redis}")
}

#[test]
fn a_redis_url_holding_user_info_is_refused_naming_the_key_not_the_password() {
    for (section, extra) in SECTIONS {
        for url in [
            format!("redis://:{PASSWORD}@redis:6379/0"),
            format!("redis://app:{PASSWORD}@redis:6379"),
            format!("rediss://app:{PASSWORD}@redis:6380"),
            format!("redis+unix:///run/redis.sock?db=0&pass={PASSWORD}"),
            format!("redis://redis:6379/0?password={PASSWORD}"),
        ] {
            let run =
                compile(&written(section, extra, &format!("redis_url = \"{url}\"")), None, &[]);
            assert!(!run.ok, "[security.{section}] {url}: compiled\n{}", run.log);
            assert!(
                run.log.contains(&format!("[security.{section}] redis_url")),
                "[security.{section}] {url}: the key is named\n{}",
                run.log
            );
            assert!(
                run.log.contains("redis_url_env"),
                "{section}: points at redis_url_env\n{}",
                run.log
            );
            assert!(!run.log.contains(PASSWORD), "{section}: the password is shown\n{}", run.log);
        }
    }
}

#[test]
fn a_redis_url_with_user_info_in_a_schema_document_is_refused_too() {
    for (section, extra) in SECTIONS {
        let mut copy = json!({"redis_url": format!("redis://:{PASSWORD}@redis:6379")});
        for line in extra.lines() {
            let (key, value) = line.split_once(" = ").unwrap();
            copy[key] = serde_json::from_str(value).unwrap();
        }
        let run = compile("", Some(json!({ section: copy })), &[]);
        assert!(!run.ok, "{section}: compiled\n{}", run.log);
        assert!(
            run.log.contains(&format!("[security.{section}] redis_url")),
            "{section}\n{}",
            run.log
        );
        assert!(!run.log.contains(PASSWORD), "{section}: the password is shown\n{}", run.log);
    }
}

#[test]
fn a_redis_url_naming_only_a_host_compiles_as_written() {
    for (section, extra) in SECTIONS {
        let run =
            compile(&written(section, extra, "redis_url = \"redis://redis:6379/0\""), None, &[]);
        assert!(run.ok, "{section}\n{}", run.log);
        let compiled = run.schema.unwrap();
        assert_eq!(compiled["security"][section]["redis_url"], "redis://redis:6379/0", "{section}");
    }
}

#[test]
fn redis_url_env_is_compiled_as_the_variables_name_not_its_value() {
    for (section, extra) in SECTIONS {
        let value = format!("redis://:{PASSWORD}@redis:6379");
        let run = compile(
            &written(section, extra, "redis_url_env = \"APP_REDIS_URL\""),
            None,
            &[("APP_REDIS_URL", value.as_str())],
        );
        assert!(run.ok, "{section}\n{}", run.log);
        let compiled = run.schema.unwrap();
        assert_eq!(compiled["security"][section]["redis_url_env"], "APP_REDIS_URL", "{section}");
        assert!(compiled["security"][section].get("redis_url").is_none(), "{section}");
        assert!(!compiled.to_string().contains(PASSWORD), "{section}: {compiled}");
    }
}

#[test]
fn a_section_naming_its_url_twice_is_refused() {
    for (section, extra) in SECTIONS {
        let redis = "redis_url = \"redis://redis:6379\"\nredis_url_env = \"APP_REDIS_URL\"";
        let run = compile(&written(section, extra, redis), None, &[]);
        assert!(!run.ok, "{section}\n{}", run.log);
        assert!(run.log.contains(&format!("[security.{section}]")), "{section}\n{}", run.log);
        assert!(run.log.contains("both"), "{section}\n{}", run.log);
    }
}

#[test]
fn redis_url_env_must_name_an_environment_variable() {
    for (section, extra) in SECTIONS {
        for name in ["", "APP REDIS", "1APP", "redis://redis:6379"] {
            let run = compile(
                &written(section, extra, &format!("redis_url_env = \"{name}\"")),
                None,
                &[],
            );
            assert!(!run.ok, "{section} {name:?}\n{}", run.log);
            assert!(
                run.log.contains(&format!(
                    "[security.{section}] redis_url_env must name an environment variable"
                )),
                "{section} {name:?}\n{}",
                run.log
            );
        }
    }
}
