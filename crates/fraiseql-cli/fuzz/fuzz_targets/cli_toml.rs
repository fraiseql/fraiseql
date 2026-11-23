#![no_main]
//! A `fraiseql.toml` compiled as the input itself, the TOML workflow of `fraiseql compile`:
//! the project settings, inline types and operations, the merger's strategies and every
//! check the settings are held to. Whatever compiles must load as a server loads it (see
//! `round_trip`).
//!
//! Three settings name other files: the `[includes]` globs, the `[domain_discovery]` root,
//! and the `text_template`/`html_template` of `[auth.local.email.*]`. What they do is read
//! those files, as the operator told them to, from wherever the fuzzer runs; none of it is
//! this input, and a value like `/dev/zero` or `**` finds only a fuzzer's own choice. So an
//! input giving one of them a string is skipped. The rest of the document, those sections'
//! other keys included, is compiled.

#[path = "../src/round_trip.rs"]
mod round_trip;

use fraiseql_cli::toml::Value;
use libfuzzer_sys::fuzz_target;

/// The keys whose values the compile reads as file paths or patterns.
const NAMES_FILES: &[&str] = &[
    "includes",
    "domain_discovery",
    "text_template",
    "html_template",
];

/// Every string beneath `value`.
fn any_string(value: &Value, hit: &dyn Fn(&str) -> bool) -> bool {
    match value {
        Value::String(s) => hit(s),
        Value::Array(items) => items.iter().any(|v| any_string(v, hit)),
        Value::Table(table) => table.values().any(|v| any_string(v, hit)),
        _ => false,
    }
}

/// Whether one of [`NAMES_FILES`] is given a string anywhere in the document.
fn names_a_file(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(names_a_file),
        Value::Table(table) => table.iter().any(|(key, v)| {
            if NAMES_FILES.contains(&key.as_str()) {
                any_string(v, &|_| true)
            } else {
                names_a_file(v)
            }
        }),
        _ => false,
    }
}

fuzz_target!(|data: &str| {
    // A document that does not parse is still compiled: the refusal is the loader's to make.
    if fraiseql_cli::toml::from_str::<Value>(data).is_ok_and(|doc| names_a_file(&doc)) {
        return;
    }
    round_trip::check("fraiseql.toml", data);
});
