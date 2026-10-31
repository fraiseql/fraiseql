//! What a WASM function's WASI surface lets it do, pinned against a probe guest
//! (`tests/fixtures/functions/guest-runtime-probe`, rebuilt by its `build.sh`).
//! `docs/architecture/functions.md` ("What a function's runtime provides") states the
//! same list; a change here is a change there.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::path::PathBuf;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, host::NoopHostContext, runtime::FunctionRuntime,
};

async fn probe() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/functions/guest-runtime-probe.wasm");
    let bytecode = bytes::Bytes::from(std::fs::read(&path).unwrap());
    let module = FunctionModule::from_bytecode("probe".to_string(), bytecode);
    let event = EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Probe".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    };
    let runtime = super::WasmRuntime::new(&super::WasmConfig::default()).unwrap();
    let result = runtime
        .invoke(&module, event.clone(), &NoopHostContext::new(event), ResourceLimits::default())
        .await
        .unwrap();
    let value = result.value.unwrap();
    match value {
        serde_json::Value::String(s) => serde_json::from_str(&s).unwrap(),
        other => other,
    }
}

#[tokio::test]
async fn the_wasi_surface_a_function_runs_on() {
    let report = probe().await;
    assert_eq!(report["env_vars"], 0, "{report}");
    assert_eq!(report["args"], 0, "{report}");
    for denied in ["read_root", "write_file", "tcp"] {
        assert!(report[denied].as_str().unwrap().starts_with("err"), "{denied}: {report}");
    }
    assert!(report["wall_clock_secs"].as_u64().unwrap() > 1_700_000_000, "{report}");
    assert_eq!(report["monotonic"], true, "{report}");
    assert_eq!(report["random_seeds_differ"], true, "{report}");
    assert_eq!(report["stdout"], "ok", "{report}");
}
