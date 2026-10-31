//! Where a Deno function's console output goes: into the function's own log
//! entries, under the same limit as `fraiseql_log`, and never to the server
//! process's stdout or stderr.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use chrono::Utc;

use super::{DenoConfig, DenoRuntime};
use crate::{
    EventPayload, FunctionModule, FunctionRuntime, LogLevel, ResourceLimits, RuntimeType,
    host::NoopHostContext,
};

async fn invoke(source: &str, limits: ResourceLimits) -> crate::FunctionResult {
    let module =
        FunctionModule::from_source("console".to_string(), source.to_string(), RuntimeType::Deno);
    let config = DenoConfig {
        enable_typescript: false,
        v8_flags:          vec![],
    };
    let runtime = DenoRuntime::new(&config).expect("Failed to create DenoRuntime");
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Console".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    Utc::now(),
    };
    runtime
        .invoke(&module, event.clone(), &NoopHostContext::new(event), limits)
        .await
        .unwrap()
}

fn entries(result: &crate::FunctionResult) -> Vec<(LogLevel, &str)> {
    result.logs.iter().map(|l| (l.level, l.message.as_str())).collect()
}

#[tokio::test]
async fn console_output_is_a_log_entry_of_the_function() {
    let result = invoke(
        "export default async () => { console.log(\"a\", 1); console.error(\"b\"); return 0; };\n",
        ResourceLimits::default(),
    )
    .await;
    assert_eq!(entries(&result), [(LogLevel::Info, "a 1"), (LogLevel::Info, "b")]);
}

#[tokio::test]
async fn a_direct_print_is_a_log_entry_too() {
    let result = invoke(
        "export default async () => { Deno.core.print(\"out\\n\", false); \
         Deno.core.ops.op_print(\"err\\n\", true); return 0; };\n",
        ResourceLimits::default(),
    )
    .await;
    assert_eq!(entries(&result), [(LogLevel::Info, "out"), (LogLevel::Error, "err")]);
}

/// A line break inside one call stays inside its one entry: a caller-supplied value
/// cannot start a line of its own.
#[tokio::test]
async fn a_line_break_stays_inside_its_entry() {
    let result = invoke(
        "export default async () => { console.log(\"one\\ntwo\"); return 0; };\n",
        ResourceLimits::default(),
    )
    .await;
    assert_eq!(entries(&result), [(LogLevel::Info, "one\ntwo")]);
}

#[tokio::test]
async fn console_output_counts_against_the_log_limit() {
    let limits = ResourceLimits {
        max_log_entries: 3,
        ..ResourceLimits::default()
    };
    let result = invoke(
        "export default async () => { for (let i = 0; i < 50; i++) console.log(i); \
         Deno.core.ops.fraiseql_log(1, \"late\"); return 0; };\n",
        limits,
    )
    .await;
    assert_eq!(
        entries(&result),
        [
            (LogLevel::Info, "0"),
            (LogLevel::Info, "1"),
            (LogLevel::Info, "2")
        ]
    );
}

const MARKER: &str = "fraiseql-console-marker-7f3a";

/// Run by [`console_output_never_reaches_the_process_streams`] in a child process,
/// whose streams that test reads.
#[tokio::test]
#[ignore = "run in a child process by console_output_never_reaches_the_process_streams"]
async fn child_writes_the_marker_every_way() {
    invoke(
        &format!(
            "export default async () => {{ console.log(\"{MARKER}\"); console.error(\"{MARKER}\"); \
             Deno.core.print(\"{MARKER}\\n\", false); Deno.core.print(\"{MARKER}\\n\", true); \
             return 0; }};\n"
        ),
        ResourceLimits::default(),
    )
    .await;
}

/// The test harness captures `print!`, not writes to the process's file descriptors,
/// so this reads the streams of a child process instead.
#[test]
fn console_output_never_reaches_the_process_streams() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime::deno::console_tests::child_writes_the_marker_every_way",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "child failed:\n{stdout}\n{stderr}");
    assert!(stdout.contains("1 passed"), "the child ran no test:\n{stdout}");
    assert!(!stdout.contains(MARKER), "stdout:\n{stdout}");
    assert!(!stderr.contains(MARKER), "stderr:\n{stderr}");
}
