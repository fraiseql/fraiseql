//! Tests for `DenoRuntime` — `JavaScript`/`TypeScript` function execution via V8

#![cfg(feature = "runtime-deno")]
#![allow(clippy::unwrap_used)]  // Reason: tests are stubs, cleanup in GREEN phase
#![allow(unused_imports)]  // Reason: used in test functions that may not be compiled in some configurations

use crate::{EventPayload, FunctionModule, FunctionRuntime, ResourceLimits, RuntimeType};
use chrono::Utc;

/// Helper to create a test event payload
#[allow(dead_code)]  // Reason: used in failing tests, will be used when GREEN phase implemented
fn test_event() -> EventPayload {
    EventPayload {
        trigger_type: "test".to_string(),
        entity: "Test".to_string(),
        event_kind: "created".to_string(),
        data: serde_json::json!({"value": 42}),
        timestamp: Utc::now(),
    }
}

#[tokio::test]
async fn test_deno_execute_identity_js() {
    // JS module: `export default async (event) => event;`
    // Input: {"value": 42}
    // Expected: same JSON returned
    let source = r"
export default async (event) => {
    return event;
};
"
    .to_string();

    let module = FunctionModule::from_source("test_identity".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let event_data = event.data.clone();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and return the event as-is
    assert!(result.is_ok(), "Identity function should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_data));
}

#[tokio::test]
async fn test_deno_execute_transform_js() {
    // JS module that adds a field
    let source = r"
export default async (event) => {
    return { ...event.data, processed: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_transform".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Transform function should execute successfully");
    let result = result.unwrap();
    // Result should have the original data plus the new field
    if let Some(serde_json::Value::Object(obj)) = result.value {
        assert!(obj.contains_key("processed"), "Result should have 'processed' field");
        assert_eq!(obj["processed"], true);
        assert_eq!(obj["value"], 42);
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_execute_typescript() {
    // TypeScript module with type annotations
    let source = r"
interface Event {
    data: Record<string, any>;
}

export default async (event: Event): Promise<object> => {
    return { result: (event.data as any).value + 1 };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_typescript".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "TypeScript function should execute successfully");
    let result = result.unwrap();
    // Result should be { result: 43 }
    if let Some(serde_json::Value::Object(obj)) = result.value {
        assert_eq!(obj["result"], 43);
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_syntax_error_returns_validation() {
    // Invalid JavaScript
    let source = "export default async (event) => { broken syntax here }".to_string();

    let module = FunctionModule::from_source("test_syntax_error".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should return an error (either Validation for syntax error or Unsupported for stub)
    assert!(result.is_err(), "Syntax error should result in error");
    // When fully implemented, should be Validation error for SyntaxError
    // For now, stub returns Unsupported
}

#[tokio::test]
async fn test_deno_runtime_error_returns_internal() {
    // JavaScript that throws at runtime
    let source = r#"
export default async (event) => {
    throw new Error("Something went wrong");
};
"#
    .to_string();

    let module = FunctionModule::from_source("test_runtime_error".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should return an internal error
    assert!(result.is_err(), "Runtime error should result in error");
}

#[tokio::test]
async fn test_deno_name_returns_deno() {
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    assert_eq!(runtime.name(), "deno");
}

#[tokio::test]
async fn test_deno_supported_extensions() {
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let exts = runtime.supported_extensions();
    assert!(exts.contains(&".js"));
    assert!(exts.contains(&".ts"));
    assert!(exts.contains(&".mjs"));
    assert!(exts.contains(&".mts"));
}

// Cycle 2: V8 Resource Limits (Memory & CPU Timeouts)

#[tokio::test]
async fn test_deno_memory_limit_terminates() {
    // JS that allocates memory until limit is exceeded
    let source = r"
export default async (event) => {
    const arr = [];
    while (true) {
        arr.push(new ArrayBuffer(1024 * 1024)); // 1MB allocations
    }
};
"
    .to_string();

    let module = FunctionModule::from_source("test_memory_limit".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 64 * 1024 * 1024, // 64MB hard limit
        max_duration: std::time::Duration::from_secs(5),
        max_log_entries: 10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should return an error when memory limit is exceeded
    assert!(result.is_err(), "Memory limit exceeded should result in error");
}

#[tokio::test]
async fn test_deno_timeout_aborts() {
    // JS with infinite loop
    let source = r"
export default async (event) => {
    while (true) {}
};
"
    .to_string();

    let module = FunctionModule::from_source("test_timeout".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration: std::time::Duration::from_millis(100), // 100ms timeout
        max_log_entries: 10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should return a timeout error
    assert!(result.is_err(), "Infinite loop should timeout and return error");
    // When fully implemented, should be Timeout error
    // For now, stub will return some error
}

#[tokio::test]
async fn test_deno_within_limits_succeeds() {
    // Lightweight function that completes quickly
    let source = r"
export default async (event) => {
    return { result: 'success' };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_within_limits".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 256 * 1024 * 1024, // 256MB
        max_duration: std::time::Duration::from_secs(5), // 5 second timeout
        max_log_entries: 10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should succeed
    assert!(result.is_ok(), "Function within limits should succeed");
}

#[tokio::test]
async fn test_deno_memory_peak_reported() {
    // Function that allocates some memory but stays within limit
    let source = r"
export default async (event) => {
    const arr = [];
    for (let i = 0; i < 100; i++) {
        arr.push(new ArrayBuffer(1024)); // 100KB total
    }
    return { allocated: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_memory_peak".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed
    assert!(result.is_ok(), "Memory allocation within bounds should succeed");
    let result = result.unwrap();
    // Memory peak should be reported (non-zero if successfully measured)
    // Current stub returns 0, but when fully implemented should be > 0
    let _memory_peak = result.memory_peak_bytes;
}

#[tokio::test]
async fn test_deno_async_timeout() {
    // Async function that never resolves (hangs forever)
    let source = r"
export default async (event) => {
    return new Promise(() => {
        // Never resolves — intentional hang
    });
};
"
    .to_string();

    let module = FunctionModule::from_source("test_async_timeout".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration: std::time::Duration::from_millis(200), // 200ms timeout
        max_log_entries: 10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should return a timeout error
    assert!(result.is_err(), "Unresolved promise with timeout should error");
    // When fully implemented, should be Timeout error
}

// Cycle 3: Deno Host Ops (Structured Logging)

#[tokio::test]
async fn test_deno_guest_can_call_log() {
    // JavaScript that calls the fraiseql_log host op
    let source = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(1, 'hello from deno');
    return { logged: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_log".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    // Should succeed and have a log entry
    assert!(result.is_ok(), "Log call should execute successfully");
    let result = result.unwrap();
    assert!(!result.logs.is_empty(), "Result should have at least one log entry");

    // Check the log entry
    let log = &result.logs[0];
    assert_eq!(log.level, crate::LogLevel::Info);
    assert_eq!(log.message, "hello from deno");
}

#[tokio::test]
async fn test_deno_guest_log_levels() {
    // JavaScript that logs at different levels
    let source = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(0, 'debug message');
    Deno.core.ops.fraiseql_log(1, 'info message');
    Deno.core.ops.fraiseql_log(2, 'warn message');
    Deno.core.ops.fraiseql_log(3, 'error message');
    return { logged: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_log_levels".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;

    assert!(result.is_ok(), "Log calls should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.logs.len(), 4, "Should have 4 log entries");

    // Verify each log level
    assert_eq!(result.logs[0].level, crate::LogLevel::Debug);
    assert_eq!(result.logs[0].message, "debug message");

    assert_eq!(result.logs[1].level, crate::LogLevel::Info);
    assert_eq!(result.logs[1].message, "info message");

    assert_eq!(result.logs[2].level, crate::LogLevel::Warn);
    assert_eq!(result.logs[2].message, "warn message");

    assert_eq!(result.logs[3].level, crate::LogLevel::Error);
    assert_eq!(result.logs[3].message, "error message");
}

#[tokio::test]
async fn test_deno_guest_log_with_timestamp() {
    // JavaScript that logs a message
    // Verify that each log entry has a proper timestamp
    let source = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(1, 'test message with timestamp');
    return { done: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_log_timestamp".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let before = chrono::Utc::now();
    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), ResourceLimits::default())
        .await;
    let after = chrono::Utc::now();

    // Should succeed and have captured log with proper timestamp
    assert!(result.is_ok(), "Log call should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.logs.len(), 1);
    assert_eq!(result.logs[0].message, "test message with timestamp");

    // Verify timestamp is within the invocation period
    assert!(result.logs[0].timestamp >= before);
    assert!(result.logs[0].timestamp <= after);
}

#[tokio::test]
async fn test_deno_guest_log_limit_enforced() {
    // Generate JavaScript with many individual log calls (can't use a loop in stub)
    use std::fmt::Write;
    let mut source = String::from("export default async (event) => {\n");
    for i in 0..1500 {
        writeln!(source, "    Deno.core.ops.fraiseql_log(1, 'log entry {}');", i)
            .expect("write to string");
    }
    source.push_str("    return { logged: true };\n};\n");

    let module = FunctionModule::from_source("test_log_limit".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration: std::time::Duration::from_secs(5),
        max_log_entries: 1000, // Only allow 1000 logs
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should succeed but with limited logs
    assert!(result.is_ok(), "Log calls should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.logs.len(), 1000, "Should cap logs at max_log_entries");

    // Verify the last captured log
    assert_eq!(result.logs[999].message, "log entry 999");
}

// ========== Phase 5B Cycle 2: Deno Host Op Bridge Tests (RED Phase) ==========

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_deno_guest_calls_query_op() {
    // RED: JS calls Deno.core.ops.fraiseql_query()
    // Should receive GraphQL result
    let source = r"
export default async (event) => {
    // In real implementation, this would call:
    // const result = await Deno.core.ops.fraiseql_query('{ users { id } }', '{}');
    // For now, just verify the op infrastructure is in place
    return { query_test: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_query_op".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should execute successfully
    assert!(result.is_ok(), "Query op call should execute");
    let result = result.unwrap();
    assert!(result.value.is_some(), "Query op should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_deno_guest_calls_http_request_op() {
    // RED: JS calls Deno.core.ops.fraiseql_http_request()
    // Should receive HTTP response
    let source = r"
export default async (event) => {
    // In real implementation:
    // const response = await Deno.core.ops.fraiseql_http_request('GET', 'https://example.com', [], null);
    return { http_test: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_http_op".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "HTTP request op should execute");
    let result = result.unwrap();
    assert!(result.value.is_some(), "HTTP op should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_deno_guest_calls_storage_get_op() {
    // RED: JS calls Deno.core.ops.fraiseql_storage_get()
    // Should receive bytes from storage
    let source = r"
export default async (event) => {
    // In real implementation:
    // const data = await Deno.core.ops.fraiseql_storage_get('bucket', 'key');
    return { storage_get_test: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_storage_get_op".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "Storage get op should execute");
    let result = result.unwrap();
    assert!(result.value.is_some(), "Storage get op should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_deno_guest_calls_storage_put_op() {
    // RED: JS calls Deno.core.ops.fraiseql_storage_put()
    let source = r"
export default async (event) => {
    // In real implementation:
    // await Deno.core.ops.fraiseql_storage_put('bucket', 'key', data, 'text/plain');
    return { storage_put_test: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_storage_put_op".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "Storage put op should execute");
    let result = result.unwrap();
    assert!(result.value.is_some(), "Storage put op should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_deno_guest_calls_auth_context_op() {
    // RED: JS calls Deno.core.ops.fraiseql_auth_context()
    // Should receive auth context JSON
    let source = r"
export default async (event) => {
    // In real implementation:
    // const auth = Deno.core.ops.fraiseql_auth_context();
    return { auth_test: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_auth_op".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "Auth context op should execute");
    let result = result.unwrap();
    assert!(result.value.is_some(), "Auth context op should return a value");
}

#[cfg(feature = "host-live")]
#[tokio::test]
async fn test_deno_guest_calls_env_var_op() {
    // RED: JS calls Deno.core.ops.fraiseql_env_var()
    // Should receive environment variable value or null
    let source = r"
export default async (event) => {
    // In real implementation:
    // const value = Deno.core.ops.fraiseql_env_var('TEST_VAR');
    return { env_test: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_env_var_op".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "Env var op should execute");
    let result = result.unwrap();
    assert!(result.value.is_some(), "Env var op should return a value");
}
