//! Tests for `ServerSubsystems` builder.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_functions::{
    FunctionDefinition, FunctionObserver, RuntimeType, triggers::TriggerRegistry,
};
use fraiseql_storage::{
    StorageMetadataRepo, StorageRlsEvaluator, StorageState,
    backend::{LocalBackend, StorageBackend},
    config::{BucketAccess, BucketConfig},
};
use sqlx::PgPool;
use tempfile::tempdir;

#[cfg(feature = "functions-runtime")]
use super::BeforeMutationHooks;
use super::{
    FunctionsSubsystem, ServerSubsystems, StorageSubsystem,
    builder::{ServerSubsystemsBuilder, SubsystemBuildError},
    validator::{SubsystemConfigWarning, validate_subsystems_config},
};
use crate::schema::loader::{FunctionsConfig, SchemaBucketDef, SchemaStorageConfig};

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
            name:               bucket_name.to_string(),
            max_object_bytes:   None,
            allowed_mime_types: None,
            access:             BucketAccess::Private,
            transform_presets:  None,
            serve_inline:       false,
        },
    );
    StorageState {
        backend:  Arc::new(backend),
        metadata: Arc::new(StorageMetadataRepo::new(lazy_pool())),
        rls:      StorageRlsEvaluator::new(),
        buckets:  Arc::new(buckets),
    }
}

fn minimal_schema_storage_config() -> SchemaStorageConfig {
    SchemaStorageConfig {
        buckets: vec![SchemaBucketDef {
            name:               "avatars".to_string(),
            access:             "private".to_string(),
            max_object_bytes:   None,
            allowed_mime_types: None,
        }],
    }
}

fn minimal_functions_config() -> FunctionsConfig {
    FunctionsConfig {
        module_dir:  "/functions".into(),
        dlq_store:   None,
        definitions: vec![FunctionDefinition::new(
            "on_create_user",
            "after:mutation:createUser",
            RuntimeType::Wasm,
        )],
    }
}

// ── Storage ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_server_state_with_storage() {
    let state = local_storage_state("avatars").await;
    let subsystem = StorageSubsystem {
        state,
        schema_config: minimal_schema_storage_config(),
    };

    let subsystems = ServerSubsystemsBuilder::new().with_storage(subsystem).build().unwrap();

    assert!(subsystems.storage.is_some());
    assert!(subsystems.functions.is_none());
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
        module_registry: std::collections::HashMap::new(),
    };

    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();

    assert!(subsystems.functions.is_some());
    assert!(subsystems.storage.is_none());
    assert_eq!(subsystems.functions.as_ref().unwrap().trigger_registry.function_count, 1);
}

#[test]
fn test_server_state_without_functions() {
    let subsystems = ServerSubsystemsBuilder::new().build().unwrap();
    assert!(subsystems.functions.is_none());
}

#[cfg(feature = "functions-runtime")]
#[test]
fn into_before_mutation_hooks_resolves_dispatch_settings() {
    // The subsystem → hooks seam resolves per-function durable-dispatch settings
    // from the compiled schema and stands up the shared dead-letter queue.
    let config = FunctionsConfig {
        module_dir:  "/functions".into(),
        dlq_store:   None,
        definitions: vec![
            FunctionDefinition::new("chargeCard", "after:mutation:Order:insert", RuntimeType::Wasm),
            FunctionDefinition::new("scoreDeal", "after:mutation:Deal:insert", RuntimeType::Wasm)
                .re_runnable(),
        ],
    };
    let trigger_registry = TriggerRegistry::load_from_definitions(&config.definitions).unwrap();
    let subsystem = FunctionsSubsystem {
        observer: Arc::new(FunctionObserver::new()),
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };

    let hooks = subsystem.into_before_mutation_hooks();

    assert_eq!(hooks.dispatch_settings.len(), 2, "one setting per function definition");
    assert!(
        hooks.dispatch_settings["scoreDeal"].re_runnable,
        "re_runnable resolved from schema"
    );
    assert!(!hooks.dispatch_settings["chargeCard"].re_runnable);
}

#[cfg(feature = "functions-runtime")]
#[test]
fn with_email_attaches_sender_resolver_and_transport() {
    use std::{future::Future, pin::Pin};

    use fraiseql_functions::{
        EmailTransport, LoginEmailSender, SendContext, SendEmailRequest, SendEmailResponse,
        SenderIdentity,
    };

    // A transport stub so the seam test needs no SMTP / `inbound-email`.
    struct NoopTransport;
    impl EmailTransport for NoopTransport {
        fn send<'a>(
            &'a self,
            _sender: &'a SenderIdentity,
            _request: &'a SendEmailRequest,
            _context: SendContext<'a>,
        ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<SendEmailResponse>> + Send + 'a>>
        {
            Box::pin(async {
                Ok(SendEmailResponse {
                    message_id: None,
                    accepted:   true,
                })
            })
        }
    }

    let trigger_registry = TriggerRegistry::load_from_definitions(&[]).unwrap();
    let hooks = BeforeMutationHooks::new(
        trigger_registry,
        HashMap::new(),
        Arc::new(FunctionObserver::new()),
    );
    // Unconfigured by default → send_email fails loud.
    assert!(hooks.sender_resolver.is_none());
    assert!(hooks.email_transport.is_none());

    // Attaching both enables the op for every dispatched function's fresh host.
    let hooks = hooks.with_email(Arc::new(LoginEmailSender), Arc::new(NoopTransport));
    assert!(hooks.sender_resolver.is_some(), "resolver attached");
    assert!(hooks.email_transport.is_some(), "transport attached");
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
    let functions = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };

    let subsystems = ServerSubsystemsBuilder::new()
        .with_storage(storage)
        .with_functions(functions)
        .build()
        .unwrap();

    assert!(subsystems.storage.is_some());
    assert!(subsystems.functions.is_some());
}

// ── Cross-subsystem validation ────────────────────────────────────────────────

#[test]
fn test_server_state_validates_deps_functions_need_storage() {
    // Functions with after:storage triggers but no storage subsystem → error.
    let observer = Arc::new(FunctionObserver::new());
    let config = FunctionsConfig {
        module_dir:  "/functions".into(),
        dlq_store:   None,
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
    let functions = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };

    let result = ServerSubsystemsBuilder::new().with_functions(functions).build();

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
        module_dir:  "/functions".into(),
        dlq_store:   None,
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
    let functions = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };

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
    let subsystem = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };
    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();
    assert!(subsystems.is_functions_enabled());
}

#[test]
fn test_empty_subsystems_all_disabled() {
    let subsystems = ServerSubsystems::none();
    assert!(!subsystems.is_storage_enabled());
    assert!(!subsystems.is_functions_enabled());
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
                name:               "avatars".to_string(),
                access:             "private".to_string(),
                max_object_bytes:   None,
                allowed_mime_types: None,
            },
            SchemaBucketDef {
                name:               "docs".to_string(), // present in schema but not in runtime
                access:             "private".to_string(),
                max_object_bytes:   None,
                allowed_mime_types: None,
            },
        ],
    };
    let subsystem = StorageSubsystem {
        state,
        schema_config,
    };
    let subsystems = ServerSubsystemsBuilder::new().with_storage(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, SubsystemConfigWarning::UnknownBucket { name } if name == "docs")),
        "expected UnknownBucket warning for 'docs'"
    );
}

/// Functions subsystem with no definitions → `EmptyFunctionsRegistry` warning.
#[test]
fn test_validate_empty_functions_registry_warns() {
    let observer = Arc::new(FunctionObserver::new());
    let config = FunctionsConfig {
        module_dir:  "/functions".into(),
        dlq_store:   None,
        definitions: vec![], // no definitions
    };
    let trigger_registry = TriggerRegistry::new();
    let subsystem = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };
    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        warnings.contains(&SubsystemConfigWarning::EmptyFunctionsRegistry),
        "expected EmptyFunctionsRegistry warning"
    );
}

/// Functions with definitions → no `EmptyFunctionsRegistry` warning.
#[test]
fn test_validate_functions_with_definitions_no_warning() {
    let observer = Arc::new(FunctionObserver::new());
    let config = minimal_functions_config();
    let trigger_registry = TriggerRegistry::new();
    let subsystem = FunctionsSubsystem {
        observer,
        trigger_registry,
        config,
        module_registry: std::collections::HashMap::new(),
    };
    let subsystems = ServerSubsystemsBuilder::new().with_functions(subsystem).build().unwrap();
    let warnings = validate_subsystems_config(&subsystems);
    assert!(
        !warnings.contains(&SubsystemConfigWarning::EmptyFunctionsRegistry),
        "should not warn about empty registry when definitions exist"
    );
}
