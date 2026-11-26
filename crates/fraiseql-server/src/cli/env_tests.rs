//! `ServerArgs::from_env` against the variables clap declares, each read in a child process
//! whose environment the test sets, so a real variable name races no test that parses `Cli`.
#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stdout)] // Reason: the probe reports to its parent on stdout

use std::{ffi::OsString, os::unix::ffi::OsStringExt};

use clap::Args;

use super::ServerArgs;

/// Every variable `ServerArgs` declares, as clap reads it.
fn declared() -> Vec<String> {
    ServerArgs::augment_args(clap::Command::new("fraiseql-server"))
        .get_arguments()
        .filter_map(|arg| arg.get_env().map(|env| env.to_string_lossy().into_owned()))
        .collect()
}

thread_local! {
    /// The configuration the last probe on this thread applied its environment to.
    static APPLIED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The default configuration with `set` applied as `fraiseql run` applies it.
fn applied_with(set: &[(&str, OsString)]) -> Option<serde_json::Value> {
    APPLIED.with(|applied| applied.borrow_mut().take());
    from_env_with(set).ok()?;
    APPLIED
        .with(|applied| applied.borrow_mut().take())
        .map(|c| serde_json::from_str(&c).unwrap())
}

const PROBE: &str = "cli::env_tests::probe_from_env";

/// `from_env()` in a child whose environment holds `set` and no other declared variable:
/// `Ok(debug)` or `Err(message)`.
fn from_env_with(set: &[(&str, OsString)]) -> Result<String, String> {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        PROBE,
        "--ignored",
        "--test-threads=1",
        "--nocapture",
    ]);
    for name in declared() {
        cmd.env_remove(name);
    }
    for (name, value) in set {
        cmd.env(name, value);
    }
    let out = cmd.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // libtest prints the test's name before its output, on the same line.
    let line =
        |tag: &str| text.lines().find_map(|l| l.split_once(tag)).map(|(_, rest)| rest.to_owned());
    if let Some(config) = line("FROM_ENV_CONFIG ") {
        APPLIED.with(|applied| *applied.borrow_mut() = Some(config));
    }
    let (ok, err) = (line("FROM_ENV_OK "), line("FROM_ENV_ERR "));
    assert!(ok.is_some() != err.is_some(), "the probe printed no single result: {text}");
    ok.ok_or_else(|| err.unwrap_or_default())
}

#[test]
#[ignore = "a probe: run by the tests below, in a child with the environment they set"]
fn probe_from_env() {
    match ServerArgs::from_env() {
        Ok(args) => {
            println!("FROM_ENV_OK {args:?}");
            let mut config = crate::ServerConfig::default();
            args.apply_to_config(&mut config);
            println!("FROM_ENV_CONFIG {}", serde_json::to_string(&config).unwrap());
        },
        Err(message) => println!("FROM_ENV_ERR {}", message.replace('\n', " ")),
    }
}

fn not_utf8() -> OsString {
    OsString::from_vec(vec![b'x', 0xff])
}

/// A value that is not valid UTF-8 refuses, naming the variable, for every variable: the
/// server's own parse refuses it, so `fraiseql run` must not read it as unset.
#[test]
fn a_value_that_is_not_valid_utf8_is_refused_naming_its_variable() {
    for name in declared() {
        let result = from_env_with(&[(&name, not_utf8())]);
        assert!(
            result.as_ref().is_err_and(|message| message.contains(&name)),
            "{name} set to a value that is not valid UTF-8: {result:?}"
        );
    }
}

/// A value its parser refuses is refused naming the variable, not the flag clap spells it as.
#[test]
fn an_unparseable_value_is_refused_naming_its_variable() {
    for (name, value) in [
        ("FRAISEQL_METRICS_ENABLED", "ture"),
        ("FRAISEQL_INTROSPECTION_REQUIRE_AUTH", "true "),
        ("FRAISEQL_POOL_MAX_SIZE", "seven"),
        ("FRAISEQL_BIND_ADDR", "127.0.0.1"),
        ("FRAISEQL_SHUTDOWN_TIMEOUT_SECS", "-1"),
    ] {
        let result = from_env_with(&[(name, value.into())]);
        assert!(
            result.as_ref().is_err_and(|message| message.contains(name)),
            "{name}={value:?}: {result:?}"
        );
    }
}

/// Every boolean spelling clap reads, `from_env` reads the same (#874: the reader it replaced
/// read clap-valid `y`/`t`/`on`, and any typo, as false).
#[test]
fn a_boolean_is_read_as_clap_reads_it() {
    for (value, expected) in [
        ("true", true),
        ("1", true),
        ("yes", true),
        ("on", true),
        ("t", true),
        ("y", true),
        ("T", true),
        ("false", false),
        ("0", false),
        ("no", false),
        ("off", false),
        ("f", false),
        ("n", false),
    ] {
        let result = from_env_with(&[("FRAISEQL_METRICS_ENABLED", value.into())]).unwrap();
        assert!(
            result.contains(&format!("metrics_enabled: Some({expected})")),
            "{value}: {result}"
        );
    }
}

/// A boolean spelling clap refuses is refused, never read as false (#874).
#[test]
fn an_unrecognised_boolean_is_refused() {
    for value in ["ture", "true ", "2", "enabled", "TRUEISH"] {
        let result = from_env_with(&[("FRAISEQL_SUBSCRIPTION_REQUIRE_AUTH", value.into())]);
        assert!(
            result.as_ref().is_err_and(|m| m.contains("FRAISEQL_SUBSCRIPTION_REQUIRE_AUTH")),
            "{value:?}: {result:?}"
        );
    }
}

/// An Arrow Flight address missing its port refuses to start, rather than falling back to a
/// default (#874).
#[cfg(feature = "arrow")]
#[test]
fn a_flight_address_missing_its_port_is_refused() {
    let result = from_env_with(&[("FRAISEQL_FLIGHT_BIND_ADDR", "127.0.0.1".into())]);
    assert!(
        result.as_ref().is_err_and(|m| m.contains("FRAISEQL_FLIGHT_BIND_ADDR")),
        "{result:?}"
    );
}

/// With no variable set, nothing is overridden.
#[test]
fn an_empty_environment_overrides_nothing() {
    let result = from_env_with(&[]).unwrap();
    assert_eq!(result, format!("{:?}", ServerArgs::default()));
}

// ── what each variable overrides ──────────────────────────────────────────────────

/// The variables that set no key of the server's file: the file's own path, the log format,
/// and the rate-limit overrides, which apply over the compiled schema's limits.
const NO_FILE_KEY: [&str; 7] = [
    "FRAISEQL_CONFIG",
    "FRAISEQL_LOG_FORMAT",
    "FRAISEQL_RATE_LIMITING_ENABLED",
    "FRAISEQL_RATE_LIMIT_RPS_PER_IP",
    "FRAISEQL_RATE_LIMIT_RPS_PER_USER",
    "FRAISEQL_RATE_LIMIT_BURST_SIZE",
    "FRAISEQL_RATE_LIMIT_MAX_BUCKETS",
];

/// The schema of the dotted `key`, through every `$ref` and `Option` on the way.
fn schema_at<'a>(schema: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    use serde_json::Value;
    let object = |mut node: &'a Value| {
        for _ in 0..64 {
            if let Some(name) = node
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
            {
                node = &schema["$defs"][name];
            } else if let Some(arm) = node.get("anyOf").and_then(Value::as_array).and_then(|a| {
                a.iter().find(|a| a.get("type").and_then(Value::as_str) != Some("null"))
            }) {
                node = arm;
            } else {
                break;
            }
        }
        node
    };
    let mut node = schema;
    for part in key.split('.') {
        node = object(node).get("properties")?.get(part)?;
    }
    Some(node)
}

/// Each variable, set alone in `fraiseql run`'s environment, changes exactly the file keys
/// the argument declarations say it does, and the schema names it on each of them. A
/// variable that changes no file key is one of [`NO_FILE_KEY`].
#[test]
fn each_variable_overrides_the_keys_the_schema_names_it_on() {
    const VALUES: [(&str, &str); 4] = [
        ("true", "false"),
        ("2", "3"),
        ("127.0.0.1:1", "127.0.0.1:2"),
        ("x", "y"),
    ];
    let schema = crate::server_config::json_schema::server_config_json_schema();
    let derived = ServerArgs::overridden_keys();
    assert_eq!(derived.len(), declared().len(), "{derived:?}");
    for (env, keys) in &derived {
        let changed = VALUES
            .iter()
            .find_map(|(a, b)| {
                Some((applied_with(&[(env, (*a).into())])?, applied_with(&[(env, (*b).into())])?))
            })
            .map_or_default(|(a, b)| super::differing_keys(&a, &b));
        assert_eq!(&changed, keys, "{env}: the keys set through the environment");
        assert_eq!(keys.is_empty(), NO_FILE_KEY.contains(&env.as_str()), "{env}: {keys:?}");
        for key in keys {
            let node = schema_at(&schema, key);
            assert_eq!(
                node.and_then(|n| n.get(crate::server_config::json_schema::ENV_ANNOTATION)),
                Some(&serde_json::Value::String(env.clone())),
                "{env} overrides `{key}`, whose schema is {node:?}"
            );
        }
    }
}
