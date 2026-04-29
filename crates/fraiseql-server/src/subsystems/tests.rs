//! Tests for `ServerSubsystems` builder.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fraiseql_functions::{FunctionDefinition, FunctionObserver, RuntimeType};
use fraiseql_functions::triggers::TriggerRegistry;
use fraiseql_storage::{
    StorageMetadataRepo, StorageRlsEvaluator, StorageState,
    backend::{LocalBackend, StorageBackend},
    config::{BucketAccess, BucketConfig},
};
use sqlx::PgPool;
use tempfile::tempdir;

use crate::realtime::{
    observer::RealtimeBroadcastObserver,
    routes::RealtimeSchemaConfig,
    server::{RealtimeConfig, RealtimeServer},
};
use crate::schema::loader::{FunctionsConfig, SchemaStorageConfig, SchemaBucketDef};

use super::{
    FunctionsSubsystem, RealtimeSubsystem, ServerSubsystems, StorageSubsystem,
    builder::{ServerSubsystemsBuilder, SubsystemBuildError},
    validator::{SubsystemConfigWarning, validate_subsystems_config},
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap()
}

async fn local_storage_state(bucket_name: &str) -> StorageState {
    let tmp = tempdir().unwrap();
    let backend = StorageBackend::Local(LocalBackend::new(tmp.path().to_str().unwrap()));
    let mut buckets = HashMap::new();
    buckets.insert(
        bucket_name.to_string(),
        BucketConfig {
            name: bucket_name.to_string(),
            max_object_bytes: None,
            allowed_mime_types: None,
            access: BucketAccess::Private,
            transform_presets: None,
        },
    );
    StorageState {
        backend: Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(lazy_pool())),
        rls: StorageRlsEvaluator::new(),
        buckets: Arc::new(buckets),
    }
}

fn minimal_schema_storage_config() -> SchemaStorageConfig {
    SchemaStorageConfig {
        buckets: vec![SchemaBucketDef {
            name: "avatars".to_string(),
            access: "private".to_string(),
            max_object_bytes: None,
            allowed_mime_types: None,
        }],
    }
}

fn minimal_functions_config() -> FunctionsConfig {
    FunctionsConfig {
        module_dir: "/functions".into(),
        definitions: vec![FunctionDefinition::new(
            "on_create_user",
            "after:mutation:createUser",
            RuntimeType::Wasm,
        )],
    }
}

fn minimal_realtime_config() -> RealtimeSchemaConfig {
    serde_json::from_value(serde_json::json!({
        "enabled": true,
        "entities": ["User"]
    }))
    .unwrap()
}

// ── Storage ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_server_state_with_storage() {
    let state = local_storage_state("avatars").await;
    let subsystem = StorageSubsystem {
        state,
        schema_config: minimal_schema_storage_config(),
    };

    let subsystems = ServerSubsystemsBuilder::new()
        .with_storage(subsystem)
        .build()
        .unwrap();

    assert!(subsystems.storage.is_some());
    assert!(subsystems.functions.is_none());
    assert!(subsystems.realtime.is_none());
}

#[test]
fn test_server_state_without_storage() {
    let subsystems = ServerSubsystemsBuilder::new().build().unwrap();
    assert!(subsystems.storage.is_none());
}

// ── Functions ─────────────────────────────────────────────────────────────────

#[test]
fn test_server_state_with_functions() {
    let observer = Arc::new(FunctionObserver::new());
    let config = minimal_functions_config();
    let trigger_registry = TriggerRegistry::load_from_definitions(&config.definitions).unwrap();

    let subsystem = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
    };

    let subsystems = ServerSubsystemsBuilder::new()
        .with_functions(subsystem)
        .build()
        .unwrap();

    assert!(subsystems.functions.is_some());
    assert!(subsystems.storage.is_none());
    assert!(subsystems.realtime.is_none());
    assert_eq!(subsystems.functions.as_ref().unwrap().trigger_registry.function_count, 1);
}

#[test]
fn test_server_state_without_functions() {
    let subsystems = ServerSubsystemsBuilder::new().build().unwrap();
    assert!(subsystems.functions.is_none());
}

// ── Realtime ──────────────────────────────────────────────────────────────────

#[test]
fn test_server_state_with_realtime() {
    let entities: HashSet<String> = ["User".to_string()].into();
    let server = Arc::new(RealtimeServer::with_entities(RealtimeConfig::default(), entities));
    let (observer, _rx) = RealtimeBroadcastObserver::new(256);

    let subsystem = RealtimeSubsystem {
        server,
        observer,
        schema_config: minimal_realtime_config(),
    };

    let subsystems = ServerSubsystemsBuilder::new()
        .with_realtime(subsystem)
        .build()
        .unwrap();

    assert!(subsystems.realtime.is_some());
    assert!(subsystems.storage.is_none());
    assert!(subsystems.functions.is_none());
}

#[test]
fn test_server_state_without_realtime() {
    let subsystems = ServerSubsystemsBuilder::new().build().unwrap();
    assert!(subsystems.realtime.is_none());
}

// ── All features ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_server_state_all_features() {
    let state = local_storage_state("avatars").await;
    let storage = StorageSubsystem {
        state,
        schema_config: minimal_schema_storage_config(),
    };

    let observer = Arc::new(FunctionObserver::new());
    let config = minimal_functions_config();
    let trigger_registry = TriggerRegistry::load_from_definitions(&config.definitions).unwrap();
    let functions = FunctionsSubsystem { observer, trigger_registry, config };

    let entities: HashSet<String> = ["User".to_string()].into();
    let rt_server = Arc::new(RealtimeServer::with_entities(RealtimeConfig::default(), entities));
    let (rt_observer, _rx) = RealtimeBroadcastObserver::new(256);
    let realtime = RealtimeSubsystem {
        server: rt_server,
        observer: rt_observer,
        schema_config: minimal_realtime_config(),
    };

    let subsystems = ServerSubsystemsBuilder::new()
        .with_storage(storage)
        .with_functions(functions)
        .with_realtime(realtime)
        .build()
        .unwrap();

    assert!(subsystems.storage.is_some());
    assert!(subsystems.functions.is_some());
    assert!(subsystems.realtime.is_some());
}

// ── Cross-subsystem validation ────────────────────────────────────────────────

#[test]
fn test_server_state_validates_deps_functions_need_storage() {
    // Functions with after:storage triggers but no storage subsystem → error.
    let observer = Arc::new(FunctionObserver::new());
    let config = FunctionsConfig {
        module_dir: "/functions".into(),
        definitions: vec![FunctionDefinition::new(
            "on_upload",
            "after:storage:avatars:upload",
            RuntimeType::Wasm,
        )],
    };
    // Use an empty registry: load_from_definitions rejects after:storage triggers as
    // not yet implemented. The builder's validation reads config.definitions directly,
    // so the registry contents don't affect the cross-subsystem dependency check.
    let trigger_registry = TriggerRegistry::new();
    let functions = FunctionsSubsystem { observer, trigger_registry, config };

    let result = ServerSubsystemsBuilder::new()
        .with_functions(functions)
        .build();

    assert!(
        matches!(result, Err(SubsystemBuildError::MissingDependency { .. })),
        "expected MissingDependency error for after:storage trigger without storage subsystem"
    );
}

#[tokio::test]
async fn test_server_state_validates_deps_storage_triggers_ok_with_storage() {
    // Functions with after:storage triggers + storage subsystem → ok.
    let state = local_storage_state("avatars").await;
    let storage = StorageSubsystem {
        state,
        schema_config: minimal_schema_storage_config(),
    };

    let observer = Arc::new(FunctionObserver::new());
    let config = FunctionsConfig {
        module_dir: "/functions".into(),
        definitions: vec![FunctionDefinition::new(
            "on_upload",
            "after:storage:avatars:upload",
            RuntimeType::Wasm,
        )],
    };
    // Use an empty registry: load_from_definitions rejects after:storage triggers as
    // not yet implemented. The builder's validation reads config.definitions directly,
    // so the registry contents don't affect the cross-subsystem dependency check.
    let trigger_registry = TriggerRegistry::new();
    let functions = FunctionsSubsystem { observer, trigger_registry, config };

    let result = ServerSubsystemsBuilder::new()
        .with_storage(storage)
        .with_functions(functions)
        .build();

    assert!(result.is_ok(), "expected Ok, storage+functions should satisfy deps");
}

// ── ServerSubsystems accessors ────────────────────────────────────────────────

#[tokio::test]
async fn test_subsystems_is_storage_enabled() {
    let state = local_storage_state("avatars").await;
    let subsystem = StorageSubsystem {
        state,
        schema_config: minimal_schema_storage_config(),
    };
    let subsystems = ServerSubsystemsBuilder::new().with_storage(subsystem).build().unwrap();
    assert!(subsystems.is_storage_enabled());
}

#[test]
fn test_subsystems_is_functions_enabled() {
    let observer = Arc::new(FunctionObserver::new());
    let config = minimal_functions_config();
    let trigger_registry = TriggerRegistry::load_from_definitions(&config.definitions).unwrap();
    let subsystem = FunctionsSubsystem { observer, trigger_registry, config };
    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();
    assert!(subsystems.is_functions_enabled());
}

#[test]
fn test_subsystems_is_realtime_enabled() {
    let entities: HashSet<String> = ["Post".to_string()].into();
    let server = Arc::new(RealtimeServer::with_entities(RealtimeConfig::default(), entities));
    let (observer, _rx) = RealtimeBroadcastObserver::new(64);
    let subsystem = RealtimeSubsystem {
        server,
        observer,
        schema_config: minimal_realtime_config(),
    };
    let subsystems = ServerSubsystemsBuilder::new().with_realtime(subsystem).build().unwrap();
    assert!(subsystems.is_realtime_enabled());
}

#[test]
fn test_empty_subsystems_all_disabled() {
    let subsystems = ServerSubsystems::none();
    assert!(!subsystems.is_storage_enabled());
    assert!(!subsystems.is_functions_enabled());
    assert!(!subsystems.is_realtime_enabled());
}

// ── Config validation ─────────────────────────────────────────────────────────

/// No subsystems enabled → no warnings.
#[test]
fn test_validate_no_subsystems_no_warnings() {
    let subsystems = ServerSubsystems::none();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(warnings.is_empty(), "expected no warnings for empty subsystems");
}

/// Local filesystem storage → `LocalStorageInProduction` warning.
#[tokio::test]
async fn test_validate_local_storage_warns() {
    let state = local_storage_state("avatars").await;
    let subsystem = StorageSubsystem {
        state,
        schema_config: minimal_schema_storage_config(),
    };
    let subsystems = ServerSubsystemsBuilder::new().with_storage(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        warnings.contains(&SubsystemConfigWarning::LocalStorageInProduction),
        "expected LocalStorageInProduction warning for local backend"
    );
}

/// A bucket declared in schema config but absent from runtime state →
/// `UnknownBucket` warning.
#[tokio::test]
async fn test_validate_unknown_bucket_warns() {
    // Runtime state has "avatars"; schema config declares "avatars" + "docs"
    let state = local_storage_state("avatars").await;
    let schema_config = SchemaStorageConfig {
        buckets: vec![
            SchemaBucketDef {
                name: "avatars".to_string(),
                access: "private".to_string(),
                max_object_bytes: None,
                allowed_mime_types: None,
            },
            SchemaBucketDef {
                name: "docs".to_string(), // present in schema but not in runtime
                access: "private".to_string(),
                max_object_bytes: None,
                allowed_mime_types: None,
            },
        ],
    };
    let subsystem = StorageSubsystem { state, schema_config };
    let subsystems = ServerSubsystemsBuilder::new().with_storage(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        warnings.iter().any(|w| matches!(w, SubsystemConfigWarning::UnknownBucket { name } if name == "docs")),
        "expected UnknownBucket warning for 'docs'"
    );
}

/// Functions subsystem with no definitions → `EmptyFunctionsRegistry` warning.
#[test]
fn test_validate_empty_functions_registry_warns() {
    let observer = Arc::new(FunctionObserver::new());
    let config = FunctionsConfig {
        module_dir: "/functions".into(),
        definitions: vec![], // no definitions
    };
    let trigger_registry = TriggerRegistry::new();
    let subsystem = FunctionsSubsystem { observer, trigger_registry, config };
    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        warnings.contains(&SubsystemConfigWarning::EmptyFunctionsRegistry),
        "expected EmptyFunctionsRegistry warning"
    );
}

/// Realtime with no entities → `RealtimeWithNoEntities` warning.
#[test]
fn test_validate_realtime_no_entities_warns() {
    let server = Arc::new(RealtimeServer::with_entities(
        RealtimeConfig::default(),
        HashSet::new(), // empty entity set
    ));
    let (observer, _rx) = RealtimeBroadcastObserver::new(64);
    let schema_config: RealtimeSchemaConfig = serde_json::from_value(serde_json::json!({
        "enabled": true,
        "entities": []
    }))
    .unwrap();
    let subsystem = RealtimeSubsystem { server, observer, schema_config };
    let subsystems = ServerSubsystemsBuilder::new().with_realtime(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        warnings.contains(&SubsystemConfigWarning::RealtimeWithNoEntities),
        "expected RealtimeWithNoEntities warning"
    );
}

/// Functions with definitions → no `EmptyFunctionsRegistry` warning.
#[test]
fn test_validate_functions_with_definitions_no_warning() {
    let observer = Arc::new(FunctionObserver::new());
    let config = minimal_functions_config();
    let trigger_registry = TriggerRegistry::new();
    let subsystem = FunctionsSubsystem { observer, trigger_registry, config };
    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        !warnings.contains(&SubsystemConfigWarning::EmptyFunctionsRegistry),
        "should not warn about empty registry when definitions exist"
    );
}
