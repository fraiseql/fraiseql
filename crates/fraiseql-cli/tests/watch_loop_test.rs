//! #383 — the `fraiseql watch` loop against a real filesystem.
//!
//! `watch::run` shipped with only pure URL-formatting unit tests; nothing
//! executed the loop. This suite drives it end to end with real file events:
//!
//! - the initial compile writes the artifact;
//! - a BROKEN save reports the failure and leaves the previous good artifact in place (the
//!   acceptance criterion watch exists for — a typo must not destroy the served schema);
//! - a subsequent fixed save recompiles, so one bad save never wedges the loop.
//!
//! No database and no server: `reload_url` is `None`, which skips the live
//! reload leg (covered by its own unit tests + the admin-route tests in
//! fraiseql-server).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — panics are acceptable

use std::{path::Path, time::Duration};

use serde_json::json;

/// A minimal valid authoring schema whose one type is named `type_name`.
fn valid_schema(type_name: &str) -> String {
    json!({
        "types": [{
            "name": type_name,
            "sql_source": "v_thing",
            "fields": [{"name": "id", "type": "ID", "nullable": false}]
        }],
        "queries": [{
            "name": "things",
            "return_type": type_name,
            "returns_list": true,
            "sql_source": "v_thing"
        }]
    })
    .to_string()
}

/// Poll until `predicate` holds on the output file, up to ~10s.
async fn wait_for(out: &Path, predicate: impl Fn(&str) -> bool) -> String {
    for _ in 0..200 {
        if let Ok(content) = std::fs::read_to_string(out) {
            if predicate(&content) {
                return content;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("output file never reached the expected state: {}", out.display());
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_recompiles_and_a_broken_save_preserves_the_artifact() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("schema.json");
    let output = dir.path().join("schema.compiled.json");
    std::fs::write(&input, valid_schema("Thing")).unwrap();

    let input_s = input.to_str().unwrap().to_string();
    let output_s = output.to_str().unwrap().to_string();
    let watcher = tokio::spawn(async move {
        // No reload URL and no database: pure compile-on-change loop.
        fraiseql_cli::commands::watch::run(&input_s, &output_s, None, None, None).await
    });

    // 1. Initial compile writes the artifact.
    let first = wait_for(&output, |c| c.contains("\"Thing\"")).await;

    // 2. A broken save must NOT clobber the good artifact. Wait past the debounce + compile window
    //    before asserting the file is unchanged.
    std::fs::write(&input, "{ this is not json").unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let after_broken = std::fs::read_to_string(&output).unwrap();
    assert_eq!(
        after_broken, first,
        "a failed compile must leave the previous good artifact untouched"
    );

    // 3. A fixed save recompiles — one bad save never wedges the loop.
    std::fs::write(&input, valid_schema("Widget")).unwrap();
    wait_for(&output, |c| c.contains("\"Widget\"")).await;

    watcher.abort();
}
