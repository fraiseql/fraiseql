//! FraiseQL serverless functions runtime.
//!
//! This crate provides the core infrastructure for executing serverless functions
//! in FraiseQL, with support for multiple runtimes (WASM, Deno, etc.).
//!
//! # Architecture
//!
//! - `FunctionRuntime`: Trait for implementing function execution backends
//! - `WasmRuntime`: WASM component model executor (feature: `runtime-wasm`)
//! - `DenoRuntime`: JavaScript/TypeScript executor via V8 (feature: `runtime-deno`)
//! - `FunctionObserver`: Integrates with fraiseql-observers for trigger execution

pub mod host;
pub mod migrations;
pub mod observer;
pub mod runtime;
pub mod triggers;
pub mod types;

pub use host::{HostContext, NoopHostContext};
pub use observer::FunctionObserver;
pub use runtime::{FunctionRuntime, SendFunctionRuntime};
pub use triggers::mutation::{
    AfterMutationTrigger, BeforeMutationTrigger, EntityEvent, EventKind, TriggerMatcher,
};
pub use types::{
    EventPayload, FunctionDefinition, FunctionModule, FunctionResult, LogEntry, LogLevel,
    ResourceLimits, RuntimeType,
};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_function_result_captures_output() {
        let logs = vec![LogEntry {
            level: LogLevel::Info,
            message: "test".to_string(),
            timestamp: chrono::Utc::now(),
        }];
        let duration = Duration::from_millis(100);
        let result = FunctionResult {
            value: Some(serde_json::json!({"key": "value"})),
            logs,
            duration,
            memory_peak_bytes: 1024,
        };

        assert_eq!(result.value, Some(serde_json::json!({"key": "value"})));
        assert_eq!(result.duration, duration);
        assert_eq!(result.memory_peak_bytes, 1024);
        assert_eq!(result.logs.len(), 1);
    }

    #[test]
    fn test_event_payload_serialization() {
        let payload = EventPayload {
            trigger_type: "mutation".to_string(),
            entity: "User".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"id": 123}),
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: EventPayload = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.trigger_type, payload.trigger_type);
        assert_eq!(restored.entity, payload.entity);
        assert_eq!(restored.event_kind, payload.event_kind);
        assert_eq!(restored.data, payload.data);
    }

    #[test]
    fn test_resource_limits_defaults() {
        let limits = ResourceLimits::default();

        assert_eq!(limits.max_memory_bytes, 128 * 1024 * 1024); // 128MB
        assert_eq!(limits.max_duration, Duration::from_secs(5)); // 5s
        assert_eq!(limits.max_log_entries, 10_000);
    }

    #[test]
    fn test_function_runtime_trait_is_object_safe() {
        // This test verifies that we can create a Box<dyn SendFunctionRuntime>
        // for dynamic dispatch of runtimes
        let _: Option<Box<dyn SendFunctionRuntime>> = None;
    }
}
