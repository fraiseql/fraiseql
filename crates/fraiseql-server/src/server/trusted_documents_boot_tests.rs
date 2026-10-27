//! A server told to enforce trusted documents enforces them, or does not boot.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use fraiseql_core::schema::{
    CompiledSchema, SecurityConfig, TrustedDocumentMode, TrustedDocumentsConfig,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::{Server, ServerError, server_config::ServerConfig};

async fn boot(security: SecurityConfig) -> Result<Server, ServerError> {
    let mut schema = CompiledSchema::new();
    schema.security = Some(security);
    Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        },
        schema,
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
}

fn trusted_documents(
    mode: TrustedDocumentMode,
    manifest_path: Option<String>,
    manifest_url: Option<&str>,
) -> SecurityConfig {
    SecurityConfig {
        trusted_documents: Some(TrustedDocumentsConfig {
            enabled: true,
            mode,
            manifest_path,
            manifest_url: manifest_url.map(str::to_string),
            ..TrustedDocumentsConfig::default()
        }),
        ..SecurityConfig::default()
    }
}

fn manifest(contents: &str) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), contents).unwrap();
    file
}

/// The configuration error a refused boot carries.
async fn refusal(security: SecurityConfig) -> String {
    match boot(security).await {
        Err(ServerError::ConfigError(message)) => message,
        Err(other) => panic!("refused for another reason: {other:?}"),
        Ok(_) => panic!("booted with an allowlist it does not enforce"),
    }
}

#[tokio::test]
async fn a_manifest_that_cannot_be_loaded_refuses_the_boot() {
    let missing = "/nonexistent/trusted-documents.json".to_string();
    let unparseable = manifest(r#"{"documents": {}}"#);
    for mode in [TrustedDocumentMode::Strict, TrustedDocumentMode::Permissive] {
        for path in [missing.clone(), unparseable.path().display().to_string()] {
            let message = refusal(trusted_documents(mode, Some(path.clone()), None)).await;
            assert!(message.contains(&path), "{mode:?}: {message}");
        }
    }
}

#[tokio::test]
async fn trusted_documents_with_no_manifest_path_refuse_the_boot() {
    for url in [None, Some("https://manifests.example.com/trusted.json")] {
        let message = refusal(trusted_documents(TrustedDocumentMode::Strict, None, url)).await;
        assert!(message.contains("manifest_path"), "{url:?}: {message}");
    }
}

#[tokio::test]
async fn persisted_queries_only_without_enabled_trusted_documents_refuses_the_boot() {
    let disabled = SecurityConfig {
        trusted_documents: Some(TrustedDocumentsConfig::default()),
        persisted_queries_only: true,
        ..SecurityConfig::default()
    };
    let absent = SecurityConfig {
        persisted_queries_only: true,
        ..SecurityConfig::default()
    };
    for security in [disabled, absent] {
        let message = refusal(security).await;
        assert!(message.contains("persisted_queries_only"), "{message}");
    }
}

#[tokio::test]
async fn a_loadable_manifest_or_trusted_documents_not_asked_for_boots() {
    let valid = manifest(r#"{"version": 1, "documents": {}}"#);
    let path = valid.path().display().to_string();
    boot(trusted_documents(TrustedDocumentMode::Strict, Some(path), None))
        .await
        .unwrap();
    boot(SecurityConfig::default()).await.unwrap();
    boot(SecurityConfig {
        trusted_documents: Some(TrustedDocumentsConfig::default()),
        ..SecurityConfig::default()
    })
    .await
    .unwrap();
}
