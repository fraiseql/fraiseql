#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_core::schema::{CompiledSchema, LocaleConfig, LocaleSource};

use super::session_variables_supported_check;

fn with_locale() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".to_string()],
            std::collections::BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

/// #1115: the wire adapter cannot apply session variables, so a schema with `[locale]` is
/// refused when a server is built over it; the same schema boots on an adapter that applies
/// them, and a schema without them boots on the wire adapter. The server binary no longer
/// boots on it (#1566), but the adapter is still a library an embedder can pass to
/// `Server::new_read_only`.
#[test]
fn a_schema_needing_session_variables_is_refused_on_the_wire_adapter() {
    let wire = fraiseql_core::db::FraiseWireAdapter::new("postgres://localhost/unused");
    let err = session_variables_supported_check(&with_locale(), &wire).unwrap_err();
    assert!(err.to_string().contains("[locale]"), "{err}");
    session_variables_supported_check(&CompiledSchema::new(), &wire)
        .expect("nothing to apply: the wire adapter boots");

    let double = fraiseql_test_utils::failing_adapter::FailingAdapter::new();
    session_variables_supported_check(&with_locale(), &double)
        .expect("an adapter that applies them boots");
}

/// The same refusal, through the constructor an embedder serving it would use.
#[tokio::test]
async fn a_server_over_the_wire_adapter_refuses_to_boot_with_locale() {
    crate::server::parity_tests::under_the_env_lock(Box::pin(async {
        let wire = std::sync::Arc::new(fraiseql_core::db::FraiseWireAdapter::new(
            "postgres://localhost/unused",
        ));
        let config = crate::server_config::ServerConfig {
            database_url: "postgres://localhost/unused".to_string(),
            ..crate::server_config::ServerConfig::default()
        };
        let err = Box::pin(crate::Server::new_read_only(config, with_locale(), wire, None))
            .await
            .err()
            .expect("a server over the wire adapter with [locale] must not boot");
        assert!(err.to_string().contains("cannot apply session variables"), "{err}");
    }))
    .await;
}

// ── #1555: the row cache is opt-in, and a deployment that turned it on is told so ──

/// The JSON log events `warn_when_nothing_is_cacheable` emits for `schema`.
fn cache_opt_in_events(schema: &CompiledSchema, cache_enabled: bool) -> Vec<serde_json::Value> {
    log_events(|| super::warn_when_nothing_is_cacheable(schema, cache_enabled))
}

/// The JSON log events `emit` writes.
fn log_events(emit: impl FnOnce()) -> Vec<serde_json::Value> {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt().json().with_writer(move || writer.clone()).finish();
    fraiseql_test_support::tracing_capture::with_default(subscriber, emit);
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    log.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

fn schema_with_users(cache_ttl_seconds: Option<u64>) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut users = fraiseql_core::schema::QueryDefinition::new("users", "User").returning_list();
    users.sql_source = Some("v_user".to_string());
    users.cache_ttl_seconds = cache_ttl_seconds;
    schema.queries.push(users);
    schema
}

#[test]
fn a_cache_no_query_opts_into_is_reported_at_boot() {
    let events = cache_opt_in_events(&schema_with_users(None), true);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    assert_eq!(events[0]["fields"]["cacheable_queries"], 0);
}

#[test]
fn a_cache_a_query_opts_into_says_nothing() {
    assert!(cache_opt_in_events(&schema_with_users(Some(60)), true).is_empty());
}

#[test]
fn a_disabled_cache_says_nothing_here() {
    assert!(cache_opt_in_events(&schema_with_users(None), false).is_empty());
}

// ── #1577: the cache is per process; an entry that never expires is stale on every other ──

/// The JSON log events `warn_on_entries_no_other_server_evicts` emits for `schema`.
fn per_process_events(schema: &CompiledSchema, cache_enabled: bool) -> Vec<serde_json::Value> {
    use crate::server_config::CacheCrossInstance;
    log_events(|| {
        super::warn_on_entries_no_other_server_evicts(
            schema,
            cache_enabled,
            CacheCrossInstance::Off,
        );
    })
}

#[test]
fn a_query_cached_without_expiry_is_named_at_boot() {
    let events = per_process_events(&schema_with_users(Some(0)), true);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    assert_eq!(events[0]["fields"]["queries"], "[\"users\"]");
}

#[test]
fn a_query_cached_with_a_ttl_says_nothing() {
    assert!(per_process_events(&schema_with_users(Some(60)), true).is_empty());
}

#[test]
fn a_query_reading_no_view_is_not_named() {
    let mut schema = schema_with_users(Some(0));
    schema.queries[0].sql_source = None;
    assert!(per_process_events(&schema, true).is_empty(), "the cache never stores it");
}

#[test]
fn a_server_hearing_the_others_names_no_query_cached_without_expiry() {
    use crate::server_config::CacheCrossInstance;
    let events = log_events(|| {
        super::warn_on_entries_no_other_server_evicts(
            &schema_with_users(Some(0)),
            true,
            CacheCrossInstance::Notify,
        );
    });
    assert!(events.is_empty(), "another server's write evicts here too: {events:?}");
}

#[test]
fn a_disabled_cache_names_no_query_cached_without_expiry() {
    assert!(per_process_events(&schema_with_users(Some(0)), false).is_empty());
}

// ── #1717: a service SDL that requires authentication, on a server that authenticates nobody ──

/// The JSON log events `warn_when_no_caller_can_read_the_service_sdl` emits.
fn service_sdl_events(
    federation_enabled: bool,
    requires_auth: bool,
    authenticates: bool,
) -> Vec<serde_json::Value> {
    log_events(|| {
        super::warn_when_no_caller_can_read_the_service_sdl(
            federation_enabled,
            requires_auth,
            authenticates,
        );
    })
}

#[test]
fn a_service_sdl_no_caller_can_read_is_reported_naming_both_ways_out() {
    let events = service_sdl_events(true, true, false);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    let message = events[0]["fields"]["message"].as_str().unwrap();
    assert!(message.contains("service_sdl_require_auth = false"), "{message}");
    assert!(message.contains("fraiseql federation sdl"), "{message}");
}

#[test]
fn a_service_sdl_some_caller_can_read_is_not_reported() {
    assert_eq!(service_sdl_events(false, true, false), Vec::<serde_json::Value>::new());
    assert_eq!(service_sdl_events(true, false, false), Vec::<serde_json::Value>::new());
    assert_eq!(service_sdl_events(true, true, true), Vec::<serde_json::Value>::new());
}

// ── #1801: a federated server whose trusted documents are strict ──

fn store(
    mode: crate::trusted_documents::TrustedDocumentMode,
) -> crate::trusted_documents::TrustedDocumentStore {
    crate::trusted_documents::TrustedDocumentStore::from_documents(
        std::collections::HashMap::new(),
        mode,
    )
}

#[test]
fn a_federated_server_with_strict_trusted_documents_is_refused_naming_both_ways_out() {
    use crate::trusted_documents::TrustedDocumentMode;
    let err = super::refuse_strict_trusted_documents_under_federation(
        true,
        Some(&store(TrustedDocumentMode::Strict)),
    )
    .expect_err("a strict store under federation must refuse the boot");
    let message = err.to_string();
    assert!(message.contains("at the gateway"), "{message}");
    assert!(message.contains("disable federation"), "{message}");
}

#[test]
fn a_federated_server_without_strict_trusted_documents_boots() {
    use crate::trusted_documents::TrustedDocumentMode;
    let permissive = store(TrustedDocumentMode::Permissive);
    let strict = store(TrustedDocumentMode::Strict);
    for (federation, store) in [
        (true, Some(&permissive)),
        (true, None),
        (false, Some(&strict)),
    ] {
        assert!(
            super::refuse_strict_trusted_documents_under_federation(federation, store).is_ok(),
            "federation {federation}, store {:?}",
            store.map(crate::trusted_documents::TrustedDocumentStore::mode),
        );
    }
}

// ── #1717: a production server told to give "Did you mean" hints ──

/// The JSON log events `warn_when_production_gives_suggestions` emits.
fn suggestion_events(
    set: Option<bool>,
    deployment: fraiseql_guard::deployment::Deployment,
) -> Vec<serde_json::Value> {
    log_events(|| super::warn_when_production_gives_suggestions(set, deployment))
}

#[test]
fn a_production_server_told_to_give_suggestions_is_warned() {
    use fraiseql_guard::deployment::Deployment;
    let events = suggestion_events(Some(true), Deployment::Production);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    let message = events[0]["fields"]["message"].as_str().unwrap();
    assert!(message.contains("suggestions_enabled"), "{message}");
}

#[test]
fn a_server_giving_suggestions_by_its_own_choice_is_not_warned() {
    use fraiseql_guard::deployment::Deployment;
    for (set, deployment) in [
        (None, Deployment::Production),
        (Some(false), Deployment::Production),
        (Some(true), Deployment::Development),
        (None, Deployment::Development),
    ] {
        assert_eq!(
            suggestion_events(set, deployment),
            Vec::<serde_json::Value>::new(),
            "{set:?} in {deployment:?}"
        );
    }
}

/// #1989: a `static_enriched` field must be one the compiled schema reads
/// ([`CompiledSchema::enriched_fields_read`]); whether `[identity.enrichment]` is configured
/// does not enter into it.
mod static_enriched_fields {
    use fraiseql_core::schema::{
        CompiledSchema, SecurityConfig, ServiceAccountConfig, SessionVariableMapping,
        SessionVariableSource, SubscriptionPolicy, TypeDefinition,
    };
    use serde_json::json;

    use super::super::static_enriched_fields_check;

    /// What the schema reads: session variables `app.agent_id` / `app.agent_role` from the
    /// enriched fields `agent_id` / `agent_role`. The settings' names deliberately differ
    /// from the fields.
    pub(super) fn reading_agent_fields(mut schema: CompiledSchema) -> CompiledSchema {
        for field in ["agent_id", "agent_role"] {
            schema.session_variables.variables.push(SessionVariableMapping {
                name:   format!("app.{field}"),
                source: SessionVariableSource::Enrichment {
                    field: field.to_string(),
                },
            });
        }
        schema
    }

    fn with_service_account(static_enriched: serde_json::Value) -> CompiledSchema {
        let account = ServiceAccountConfig {
            secret_env:      "SA_SECRET".to_string(),
            roles:           vec!["manager".to_string()],
            scopes:          Vec::new(),
            tenant:          None,
            static_enriched: serde_json::from_value(static_enriched).unwrap(),
        };
        CompiledSchema {
            security: Some(SecurityConfig {
                service_accounts: Some([("sidecar".to_string(), account)].into()),
                ..SecurityConfig::default()
            }),
            ..CompiledSchema::new()
        }
    }

    fn with_source(static_enriched: &serde_json::Value) -> CompiledSchema {
        CompiledSchema {
            sources: vec![
                serde_json::from_value(json!({
                    "name": "poll_orders", "schedule": "*/5 * * * *", "function": "pollOrders",
                    "run_as": { "roles": ["ingest_writer"], "static_enriched": static_enriched }
                }))
                .unwrap(),
            ],
            ..CompiledSchema::new()
        }
    }

    fn refusal(schema: &CompiledSchema) -> String {
        static_enriched_fields_check(schema)
            .expect_err("a field the schema never reads is refused")
            .to_string()
    }

    /// The real shape: the schema reads both declared fields, and an empty value is a value.
    #[test]
    fn a_service_account_whose_fields_the_schema_reads_boots() {
        let schema = reading_agent_fields(with_service_account(
            json!({ "agent_role": "manager", "agent_id": "" }),
        ));
        static_enriched_fields_check(&schema).expect("both fields are read");
    }

    /// A typo'd field is refused, naming the field and the account, and only that field.
    #[test]
    fn a_service_account_field_the_schema_never_reads_is_refused() {
        let schema = reading_agent_fields(with_service_account(
            json!({ "agent_role": "manager", "agent_rol": "manager" }),
        ));
        let error = refusal(&schema);
        assert!(error.contains("service account `sidecar` declares `agent_rol`"), "{error}");
        assert!(!error.contains("declares `agent_role`"), "names only the unread field: {error}");
    }

    /// A session variable's own name is not the field it reads.
    #[test]
    fn a_field_named_after_a_session_variable_is_refused() {
        let schema = reading_agent_fields(with_service_account(json!({ "app.agent_role": "a" })));
        let error = refusal(&schema);
        assert!(error.contains("declares `app.agent_role`"), "{error}");
    }

    #[test]
    fn a_source_run_as_field_the_schema_never_reads_is_refused() {
        let schema = reading_agent_fields(with_source(&json!({ "agent_team": "a" })));
        let error = refusal(&schema);
        assert!(error.contains("source `poll_orders` declares `agent_team`"), "{error}");
    }

    #[test]
    fn a_source_run_as_whose_fields_the_schema_reads_boots() {
        let schema =
            reading_agent_fields(with_source(&json!({ "agent_role": "manager", "agent_id": "" })));
        static_enriched_fields_check(&schema).expect("both fields are read");
    }

    /// A schema that reads no enriched field refuses every declared one.
    #[test]
    fn a_schema_reading_no_enriched_field_refuses_static_enriched() {
        let error = refusal(&with_service_account(json!({ "agent_role": "manager" })));
        assert!(error.contains("declares `agent_role`"), "{error}");
        assert!(error.contains("reads no enriched field"), "{error}");
    }

    /// A subscription policy fed only by a service account's `static_enriched` boots: the
    /// policy reads the field, and no `[identity.enrichment]` is needed for it.
    #[test]
    fn a_subscription_policy_read_only_by_a_service_account_boots() {
        let mut schema = with_service_account(json!({ "user_id": "svc-reconciler" }));
        schema
            .types
            .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
                SubscriptionPolicy {
                    owner_path:     "$.owner_id".to_string(),
                    identity_field: "user_id".to_string(),
                    bypass_roles:   vec![],
                },
            ));
        static_enriched_fields_check(&schema).expect("the policy reads `user_id`");
    }

    /// No declaration, nothing to check.
    #[test]
    fn a_schema_declaring_no_static_enriched_boots() {
        static_enriched_fields_check(&with_service_account(json!({}))).expect("nothing declared");
        static_enriched_fields_check(&with_source(&json!({}))).expect("nothing declared");
    }

    /// #1989: the functions' ceilings, checked where the functions section is read.
    #[cfg(feature = "functions-runtime")]
    mod functions {
        use fraiseql_core::schema::CompiledSchema;
        use serde_json::json;

        use super::reading_agent_fields;
        use crate::{
            schema::loader::FunctionsConfig,
            server::initialization::functions_static_enriched_fields_check,
        };

        fn with_function(static_enriched: &serde_json::Value) -> FunctionsConfig {
            serde_json::from_value(json!({
                "module_dir": "functions",
                "definitions": [{
                    "name": "expireSessions", "trigger": "cron:*/5 * * * *", "runtime": "Deno",
                    "run_as": { "roles": ["maintenance"], "static_enriched": static_enriched }
                }]
            }))
            .unwrap()
        }

        #[test]
        fn a_function_run_as_field_the_schema_never_reads_is_refused() {
            let error = functions_static_enriched_fields_check(
                &with_function(&json!({ "agent_team": "a" })),
                &reading_agent_fields(CompiledSchema::new()),
            )
            .expect_err("refused")
            .to_string();
            assert!(error.contains("function `expireSessions` declares `agent_team`"), "{error}");
        }

        #[test]
        fn a_function_run_as_whose_fields_the_schema_reads_boots() {
            functions_static_enriched_fields_check(
                &with_function(&json!({ "agent_role": "manager", "agent_id": "" })),
                &reading_agent_fields(CompiledSchema::new()),
            )
            .expect("both fields are read");
        }
    }
}
