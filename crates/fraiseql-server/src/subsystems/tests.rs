//! Tests for the functions subsystem hot-path types.
//!
//! (The `ServerSubsystems` bundle / builder / `validate_subsystems_config`
//! tests went with their subjects in #874 — the bundle had no production
//! constructor, so those tests exercised an API nothing served.)

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![cfg(feature = "functions-runtime")]

use std::{collections::HashMap, sync::Arc};

use fraiseql_functions::{
    FunctionDefinition, FunctionObserver, RuntimeType, triggers::TriggerRegistry,
};

use super::{BeforeMutationHooks, FunctionsSubsystem};
use crate::schema::loader::FunctionsConfig;

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
