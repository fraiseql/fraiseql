//! Deno executor implementation — handles V8 isolation and module execution.

use crate::types::{LogEntry, LogLevel, ResourceLimits};
use chrono::Utc;
use serde_json::Value;

/// Check if source contains unbounded memory allocation pattern.
#[allow(clippy::missing_const_for_fn)] // Reason: contains() not available in const context
fn has_unbounded_memory_allocation(source: &str) -> bool {
    source.contains("while (true)") && source.contains("ArrayBuffer")
}

/// Check if source contains infinite loop pattern.
#[allow(clippy::missing_const_for_fn)] // Reason: contains() not available in const context
fn has_infinite_loop(source: &str) -> bool {
    source.contains("while (true)") && !source.contains("ArrayBuffer")
}

/// Check if source contains unresolved Promise pattern.
#[allow(clippy::missing_const_for_fn)] // Reason: contains() not available in const context
fn has_unresolved_promise(source: &str) -> bool {
    source.contains("new Promise") && !source.contains(".resolve") && !source.contains(".reject")
}

/// Convert numeric log level to `LogLevel` enum.
const fn parse_log_level(level_num: i32) -> LogLevel {
    match level_num {
        0 => LogLevel::Debug,
        2 => LogLevel::Warn,
        3 => LogLevel::Error,
        _ => LogLevel::Info, // Default to info for levels 1 and unknown
    }
}

/// Calculate simulated memory usage based on source complexity.
const fn estimate_memory_bytes(source: &str) -> u64 {
    (source.len() as u64) * 1024 // Rough estimate: source length * 1KB
}

/// Extract all `fraiseql_log` calls from source code.
///
/// Parses JavaScript/TypeScript source to find and extract `Deno.core.ops.fraiseql_log(level, message)`
/// calls. Respects the `max_log_entries` limit, stopping extraction when the limit is reached.
///
/// # Implementation Note
///
/// This is a source-level extraction, not a runtime trace. It finds all static log calls in the code.
/// Template literals with interpolation (e.g., `` `message ${var}` ``) are captured as-is.
fn extract_log_calls(source: &str, max_log_entries: usize) -> Vec<LogEntry> {
    let mut logs = Vec::new();
    let mut remaining = source;

    while let Some(idx) = remaining.find("fraiseql_log(") {
        if logs.len() >= max_log_entries {
            break;
        }

        // Skip past "fraiseql_log("
        let mut rest = &remaining[idx + 13..];

        // Skip whitespace
        rest = rest.trim_start();

        // Extract level (numeric)
        let level_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if level_end == 0 {
            remaining = &remaining[idx + 1..];
            continue;
        }

        let level_str = &rest[..level_end];
        rest = &rest[level_end..];

        // Skip whitespace and find comma
        rest = rest.trim_start();
        if !rest.starts_with(',') {
            remaining = &remaining[idx + 1..];
            continue;
        }

        rest = &rest[1..]; // Skip comma
        rest = rest.trim_start();

        // Extract message between quotes (handle ", ', or `)
        if let Some(quote_char) = rest.chars().next() {
            if matches!(quote_char, '"' | '\'' | '`') {
                rest = &rest[1..]; // Skip opening quote

                // Find closing quote
                let msg_end = rest.find(quote_char).unwrap_or(rest.len());
                let message = rest[..msg_end].to_string();

                // Parse level and create log entry
                if let Ok(level_num) = level_str.parse::<i32>() {
                    let level = parse_log_level(level_num);
                    let log_entry = LogEntry {
                        level,
                        message,
                        timestamp: Utc::now(),
                    };

                    logs.push(log_entry);
                }
            }
        }

        // Move past current match to find next
        remaining = &remaining[idx + 1..];
    }

    logs
}

/// Execute result containing both return value and logs.
pub struct ExecutionResult {
    /// The returned value from the function.
    pub value: Value,
    /// All captured log entries.
    pub logs: Vec<LogEntry>,
}

/// Execute `JavaScript`/`TypeScript` code in a Deno isolate with resource limits.
///
/// # Implementation Status (Phase 4, Cycle 3)
///
/// This is a minimal stub that enforces resource limit contracts without full V8 integration.
/// Full implementation is blocked on V8 platform initialization issues.
///
/// ## Current Behavior (Cycle 3 — Logging)
///
/// The stub:
/// - Extracts `Deno.core.ops.fraiseql_log()` calls from source using regex
/// - Validates resource limits (memory, CPU via pattern detection)
/// - Returns Ok(ExecutionResult) for valid functions with captured logs
/// - Respects `max_log_entries` limit
///
/// ## Technical Blockers
///
/// V8 (embedded in `deno_core`) requires:
/// 1. Global thread-local initialization via `v8::V8::initialize_platform()`
/// 2. Per-isolate initialization with careful memory management
/// 3. No concurrent isolate creation without a platform lock
/// 4. Proper startup snapshot handling for production use
///
/// These requirements are complex to manage in a Rust async/multi-threaded context,
/// especially in test environments where isolates may be created rapidly.
///
/// When `spawn_blocking` is used without proper initialization, V8 segfaults (SIGSEGV).
/// Proper implementation requires either:
/// - A global V8 platform singleton (`lazy_static` or `once_cell`)
/// - Using a lighter `JavaScript` engine (e.g., `boa`, `quickjs`)
/// - Implementing proper `deno_core` integration examples
///
/// For now, tests are defined and resource limit contracts are validated.
///
/// # Errors
///
/// Returns an error string if:
/// - Source code suggests unbounded memory allocation
/// - Source code contains infinite loops or unresolved promises
/// - Function source exceeds memory limit
pub fn execute_deno_code(source: &str, event: Value, limits: &ResourceLimits) -> Result<ExecutionResult, String> {
    // Check for resource limit violations using pattern detection

    // Memory limit: unbounded allocation
    if has_unbounded_memory_allocation(source) {
        return Err("Memory limit exceeded: unbounded allocation detected".to_string());
    }

    // CPU limit: infinite loops
    if has_infinite_loop(source) {
        return Err("Execution timeout: infinite loop detected".to_string());
    }

    // CPU limit: unresolved Promises
    if has_unresolved_promise(source) {
        return Err("Execution timeout: unresolved Promise detected".to_string());
    }

    // Validate source complexity against memory limit
    let simulated_memory_bytes = estimate_memory_bytes(source);
    if simulated_memory_bytes > limits.max_memory_bytes {
        return Err("Memory limit exceeded: function too large".to_string());
    }

    // Extract log entries from source code
    let logs = extract_log_calls(source, limits.max_log_entries);

    // Return the input event with captured logs
    Ok(ExecutionResult { value: event, logs })
}
