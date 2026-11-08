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
    let events = cache_opt_in_events(&schema_with_users(Some(60)), true);
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn a_disabled_cache_says_nothing_here() {
    let events = cache_opt_in_events(&schema_with_users(None), false);
    assert!(events.is_empty(), "{events:?}");
}

// ── the server's `cache_enabled = false` over a schema whose queries declare cache TTLs ──

/// The JSON log events `warn_on_inert_cache_ttls` emits for `schema`.
fn inert_ttl_events(schema: &CompiledSchema, cache_enabled: bool) -> Vec<serde_json::Value> {
    log_events(|| super::warn_on_inert_cache_ttls(schema, cache_enabled))
}

/// A `[[caching.rules]]` TTL compiled into the schema is inert when the server's file turns
/// the result cache off, and the operator is told which queries lost it.
#[test]
fn compiled_cache_ttls_under_a_disabled_cache_are_reported_at_boot() {
    let events = inert_ttl_events(&schema_with_users(Some(60)), false);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["level"], "WARN");
    assert_eq!(events[0]["fields"]["queries"], "[\"users\"]");
    let message = events[0]["fields"]["message"].as_str().unwrap();
    assert!(message.contains("cache_enabled = false"), "{message}");
}

#[test]
fn compiled_cache_ttls_under_an_enabled_cache_say_nothing() {
    let events = inert_ttl_events(&schema_with_users(Some(60)), true);
    assert!(events.is_empty(), "{events:?}");
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
    let events = per_process_events(&schema_with_users(Some(60)), true);
    assert!(events.is_empty(), "{events:?}");
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
    let events = per_process_events(&schema_with_users(Some(0)), false);
    assert!(events.is_empty(), "{events:?}");
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

    /// A subscription policy fed only by a service account's `static_enriched` passes this
    /// check: the policy reads the field. `[identity.enrichment]` is still required, by
    /// `enrichment_consumer_without_resolver_check`
    /// (`a_policy_read_only_by_a_service_account_still_needs_enrichment`).
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

/// Every reader of the enriched identity needs `[identity.enrichment]` enabled (#2005).
///
/// One schema per reader kind, each reading one field through one reader and nothing
/// else, so dropping a kind from the shared reader list reddens exactly its own case.
#[cfg(feature = "auth")]
mod enrichment_consumer {
    use fraiseql_core::schema::{
        CompiledSchema, InjectedParamSource, LocaleConfig, LocaleSource, MutationDefinition,
        QueryDefinition, SessionVariableMapping, SessionVariableSource, SubscriptionPolicy,
        TypeDefinition,
    };
    use serde_json::json;

    use super::super::enrichment_consumer_without_resolver_check;

    fn enrichment(field: &str) -> InjectedParamSource {
        InjectedParamSource::Enrichment(field.to_string())
    }

    /// `(reader kind, field read, a schema whose only reader is that kind)`.
    fn one_schema_per_reader_kind() -> Vec<(&'static str, &'static str, CompiledSchema)> {
        let mut session_variable = CompiledSchema::new();
        session_variable.session_variables.variables.push(SessionVariableMapping {
            name:   "app.agent_role".to_string(),
            source: SessionVariableSource::Enrichment {
                field: "agent_role".to_string(),
            },
        });

        let mut query_inject = CompiledSchema::new();
        let mut query = QueryDefinition::new("orders", "Order");
        query.inject_params.insert("p_org".to_string(), enrichment("org_id"));
        query_inject.queries.push(query);

        let mut mutation_inject = CompiledSchema::new();
        let mut mutation = MutationDefinition::new("approveOrder", "Order");
        mutation.inject_params.insert("p_actor".to_string(), enrichment("actor_id"));
        mutation_inject.mutations.push(mutation);

        let mut type_inject = CompiledSchema::new();
        let mut order = TypeDefinition::new("Order", "v_order");
        order.inject_params.insert("tenant_id".to_string(), enrichment("tenant_ref"));
        type_inject.types.push(order);

        let mut fact_inject = CompiledSchema::new();
        fact_inject.fact_tables.insert(
            "tf_sales".to_string(),
            serde_json::from_value(json!({
                "table_name": "tf_sales",
                "measures": [{"name": "revenue", "sql_type": "Decimal", "nullable": false}],
                "dimensions": {"name": "data", "paths": []},
                "denormalized_filters": [{"name": "author_id", "sql_type": "Text", "indexed": true}],
                "inject_params": {"author_id": {"source": "enrichment", "claim": "author_ref"}}
            }))
            .unwrap(),
        );

        let locale = CompiledSchema {
            locale: Some(
                LocaleConfig::new(
                    "en-US",
                    vec!["en-US".to_string()],
                    std::collections::BTreeMap::new(),
                    vec![LocaleSource::Enrichment {
                        enrichment: "preferred_locale".to_string(),
                    }],
                )
                .unwrap(),
            ),
            ..CompiledSchema::new()
        };

        let mut policy = CompiledSchema::new();
        policy
            .types
            .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
                SubscriptionPolicy {
                    owner_path:     "$.owner_id".to_string(),
                    identity_field: "user_id".to_string(),
                    bypass_roles:   vec![],
                },
            ));

        vec![
            ("session variable", "agent_role", session_variable),
            ("query inject", "org_id", query_inject),
            ("mutation inject", "actor_id", mutation_inject),
            ("type inject", "tenant_ref", type_inject),
            ("fact-table inject", "author_ref", fact_inject),
            ("locale source", "preferred_locale", locale),
            ("subscription policy", "user_id", policy),
        ]
    }

    fn config(enrichment_enabled: Option<bool>) -> crate::ServerConfig {
        crate::ServerConfig {
            identity: enrichment_enabled.map(|enabled| {
                serde_json::from_value(json!({
                    "enrichment": {
                        "enabled": enabled,
                        "query": "SELECT actor_id FROM app.actor WHERE sub = $sub",
                        "map": {"actor_id": "actor_id"}
                    }
                }))
                .unwrap()
            }),
            ..crate::ServerConfig::default()
        }
    }

    #[test]
    fn every_reader_kind_without_enrichment_is_refused_naming_the_field() {
        for (kind, field, schema) in one_schema_per_reader_kind() {
            for config in [config(None), config(Some(false))] {
                let error = enrichment_consumer_without_resolver_check(&schema, &config)
                    .expect_err(kind)
                    .to_string();
                assert!(
                    error.contains("`[identity.enrichment]` is not enabled"),
                    "{kind}: {error}"
                );
                assert!(error.contains(&format!("`{field}`")), "{kind}: {error}");
            }
        }
    }

    #[test]
    fn every_reader_kind_with_enrichment_boots() {
        for (kind, _, schema) in one_schema_per_reader_kind() {
            enrichment_consumer_without_resolver_check(&schema, &config(Some(true)))
                .map_err(|error| format!("{kind}: {error}"))
                .unwrap();
        }
    }

    #[test]
    fn a_schema_reading_no_enriched_field_boots_without_enrichment() {
        enrichment_consumer_without_resolver_check(&CompiledSchema::new(), &config(None))
            .expect("nothing reads the enriched identity");
    }

    /// The counterpart of `a_subscription_policy_read_only_by_a_service_account_boots`:
    /// `static_enriched` answers a read in lieu of the resolver, never instead of one, so
    /// the same schema is refused when nothing resolves a human principal.
    #[test]
    fn a_policy_read_only_by_a_service_account_still_needs_enrichment() {
        let mut schema = CompiledSchema {
            security: Some(fraiseql_core::schema::SecurityConfig {
                service_accounts: Some(
                    [(
                        "reconciler".to_string(),
                        fraiseql_core::schema::ServiceAccountConfig {
                            secret_env:      "SA_SECRET".to_string(),
                            roles:           Vec::new(),
                            scopes:          Vec::new(),
                            tenant:          None,
                            static_enriched: [("user_id".to_string(), json!("svc"))].into(),
                        },
                    )]
                    .into(),
                ),
                ..fraiseql_core::schema::SecurityConfig::default()
            }),
            ..CompiledSchema::new()
        };
        schema
            .types
            .push(TypeDefinition::new("Order", "v_order").with_subscription_policy(
                SubscriptionPolicy {
                    owner_path:     "$.owner_id".to_string(),
                    identity_field: "user_id".to_string(),
                    bypass_roles:   vec![],
                },
            ));
        enrichment_consumer_without_resolver_check(&schema, &config(None))
            .expect_err("a service account's static_enriched does not stand in for a resolver");
    }
}

fn config_with_scim(enabled: bool) -> crate::ServerConfig {
    crate::ServerConfig {
        scim: Some(crate::server_config::scim::ScimServerConfig {
            enabled,
            ..crate::server_config::scim::ScimServerConfig::default()
        }),
        ..crate::ServerConfig::default()
    }
}

#[test]
fn scim_enabled_on_a_build_without_auth_is_refused_naming_the_feature() {
    for has_db_pool in [true, false] {
        let err = super::refuse_scim_it_cannot_mount(&config_with_scim(true), false, has_db_pool)
            .expect_err("a build without `auth` serves no SCIM route");
        let message = err.to_string();
        assert!(message.contains("[scim]") && message.contains("`auth` feature"), "{message}");
    }
}

#[test]
fn scim_enabled_without_a_database_pool_is_refused_naming_the_pool() {
    let err = super::refuse_scim_it_cannot_mount(&config_with_scim(true), true, false)
        .expect_err("SCIM is not mounted without the pool its RBAC backend is built on");
    let message = err.to_string();
    assert!(message.contains("[scim]") && message.contains("database pool"), "{message}");
}

#[test]
fn scim_left_off_boots_whatever_the_build_lacks() {
    for config in [config_with_scim(false), crate::ServerConfig::default()] {
        for (auth_built, has_db_pool) in [(false, false), (true, false), (false, true)] {
            assert!(
                super::refuse_scim_it_cannot_mount(&config, auth_built, has_db_pool).is_ok(),
                "scim {:?}, auth {auth_built}, pool {has_db_pool}",
                config.scim.as_ref().map(|scim| scim.enabled),
            );
        }
    }
}

#[test]
fn scim_enabled_with_auth_and_a_database_pool_boots() {
    assert!(super::refuse_scim_it_cannot_mount(&config_with_scim(true), true, true).is_ok());
}

/// A failing root of a multi-root mutation is rendered inside a `200` body by the
/// executor's root renderer, with the sanitizer of the deployment the server was built
/// for: a production server, with nothing declaring the deployment, withholds the
/// database's text; a development server shows it.
#[test]
fn a_failed_mutation_root_withholds_database_text_when_nothing_declares_the_deployment() {
    use fraiseql_guard::deployment::Deployment;
    let detail = "relation \"tb_internal_ledger\" does not exist";
    let rendered = |env: Option<&str>, deployment: Deployment| {
        let vars: [(&str, Option<&str>); 3] = [
            ("FRAISEQL_ENV", env),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ];
        temp_env::with_vars(vars, || {
            let rt = super::executor_runtime_config(
                &CompiledSchema::default(),
                &crate::server_config::ServerConfig::default(),
                deployment,
            )
            .unwrap();
            let render = rt.root_error_renderer.expect("the server installs a root renderer");
            render(&fraiseql_core::error::FraiseQLError::Database {
                message:    detail.to_string(),
                sql_state:  None,
                constraint: None,
            })
            .to_string()
        })
    };
    let production = rendered(None, Deployment::Production);
    assert!(
        !production.contains("tb_internal_ledger"),
        "production withholds it: {production}"
    );
    let development = rendered(Some("development"), Deployment::Development);
    assert!(
        development.contains("tb_internal_ledger"),
        "development shows it: {development}"
    );
}

// ── The Redis URL of the Redis-backed sections ─────────────────────────────────

/// A value no log line may show.
#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
const REDIS_PASSWORD: &str = "pw-REDIS-SENTINEL";

/// PKCE, the rate limiter and token revocation, each naming its Redis URL as given.
#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
fn redis_schema(url: Option<&str>, url_env: Option<&str>) -> CompiledSchema {
    use fraiseql_core::schema::{
        PkceSecurityConfig, RateLimitingSecurityConfig, SecurityConfig,
        TokenRevocationSecurityConfig,
    };

    let url = url.map(str::to_owned);
    let url_env = url_env.map(str::to_owned);
    let mut schema = CompiledSchema::new();
    schema.security = Some(SecurityConfig {
        rate_limiting: Some(RateLimitingSecurityConfig {
            enabled: true,
            redis_url: url.clone(),
            redis_url_env: url_env.clone(),
            ..RateLimitingSecurityConfig::default()
        }),
        pkce: Some(PkceSecurityConfig {
            enabled: true,
            redis_url: url.clone(),
            redis_url_env: url_env.clone(),
            ..PkceSecurityConfig::default()
        }),
        token_revocation: Some(TokenRevocationSecurityConfig {
            enabled: true,
            backend: "redis".to_owned(),
            redis_url: url,
            redis_url_env: url_env,
            ..TokenRevocationSecurityConfig::default()
        }),
        ..SecurityConfig::default()
    });
    schema
}

/// Each Redis-backed section's construction in development: whether it chose Redis, or its
/// refusal.
#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
async fn redis_backends(schema: &CompiledSchema) -> Vec<(&'static str, Result<bool, String>)> {
    use fraiseql_guard::deployment::Deployment;

    let dev = Deployment::Development;
    let pkce = super::pkce_store_from_schema_in(schema, None, dev)
        .await
        .map(|store| !store.expect("PKCE is enabled").is_in_memory());
    let limiter = super::resolve_rate_limiter(schema, &crate::ServerConfig::default(), dev)
        .await
        .map(|limiter| limiter.expect("rate limiting is enabled").is_distributed());
    let revocation =
        crate::token_revocation::revocation_manager_from_schema_in(schema, dev, 86_400)
            .await
            .map(|manager| manager.expect("revocation is enabled").is_distributed());
    vec![
        ("pkce", pkce.map_err(|e| e.to_string())),
        ("rate_limiting", limiter.map_err(|e| e.to_string())),
        ("token_revocation", revocation.map_err(|e| e.to_string())),
    ]
}

/// `redis_backends`, with the JSON log lines it wrote.
#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
fn redis_backends_logged(
    schema: &CompiledSchema,
) -> (Vec<(&'static str, Result<bool, String>)>, Vec<serde_json::Value>) {
    let mut outcome = Vec::new();
    let log = log_events(|| {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        outcome = runtime.block_on(redis_backends(schema));
    });
    (outcome, log)
}

#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
#[test]
fn a_redis_url_env_naming_an_unset_variable_refuses_each_backend() {
    let schema = redis_schema(None, Some("FQ_TEST_REDIS_URL_NEVER_SET"));
    let (outcome, _) = redis_backends_logged(&schema);
    for (section, result) in outcome {
        let refusal = result.expect_err(section);
        assert!(
            refusal.contains(&format!(
                "[security.{section}] redis_url_env names `FQ_TEST_REDIS_URL_NEVER_SET`, which is \
                 not set"
            )),
            "{section}: {refusal}"
        );
    }
}

#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
#[test]
fn a_section_naming_its_redis_url_twice_refuses_each_backend() {
    let schema = redis_schema(Some("redis://127.0.0.1:1"), Some("FQ_TEST_REDIS_URL_NEVER_SET"));
    let (outcome, _) = redis_backends_logged(&schema);
    for (section, result) in outcome {
        let refusal = result.expect_err(section);
        assert!(
            refusal.contains(&format!("[security.{section}] sets both")),
            "{section}: {refusal}"
        );
    }
}

/// A Redis URL the client refuses falls back in development; neither that warning nor any
/// other line shows the URL's password.
#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
#[test]
fn a_refused_redis_url_is_reported_without_its_password() {
    // A port no Redis client accepts: the URL is refused without a connection attempt, whose
    // retries would hold the test for half a minute.
    let url = format!("redis://default:{REDIS_PASSWORD}@127.0.0.1:99999/0");
    let (outcome, log) = redis_backends_logged(&redis_schema(Some(&url), None));
    for (section, result) in outcome {
        assert_eq!(result, Ok(false), "{section} falls back to memory in development");
    }
    let fallbacks = log
        .iter()
        .filter(|line| line.to_string().contains("falling back to in-memory"))
        .count();
    assert_eq!(fallbacks, 3, "each backend reports its fallback: {log:?}");
    for line in &log {
        assert!(!line.to_string().contains(REDIS_PASSWORD), "{line}");
    }
}

/// A URL holding a password reaches a real Redis, from `redis_url` and from the variable
/// `redis_url_env` names, and the startup log names its host only.
///
/// Needs `REDIS_URL` naming a Redis without `requirepass` (which accepts any password for
/// the `default` user): the Redis leg binds one.
#[cfg(all(feature = "redis-pkce", feature = "redis-rate-limiting"))]
#[test]
#[ignore = "requires Redis (REDIS_URL)"]
fn a_redis_url_holding_a_password_reaches_redis_and_is_logged_by_host() {
    let redis = std::env::var("REDIS_URL").expect("REDIS_URL names the test Redis");
    let host = fraiseql_core::schema::redis_url::host_of(&redis);
    let url = host.replacen("://", &format!("://default:{REDIS_PASSWORD}@"), 1);

    let written = redis_schema(Some(&url), None);
    let named = redis_schema(None, Some("FQ_TEST_REDIS_URL_WITH_PASSWORD"));
    let runs = [
        redis_backends_logged(&written),
        temp_env::with_var("FQ_TEST_REDIS_URL_WITH_PASSWORD", Some(&url), || {
            redis_backends_logged(&named)
        }),
    ];
    for (outcome, log) in runs {
        for (section, result) in outcome {
            assert_eq!(result, Ok(true), "{section} runs on Redis");
        }
        let hosts: Vec<&serde_json::Value> =
            log.iter().filter_map(|line| line["fields"].get("redis_host")).collect();
        assert_eq!(hosts.len(), 2, "PKCE and the rate limiter name the host: {log:?}");
        for logged in hosts {
            assert_eq!(logged, &serde_json::Value::String(host.clone()));
        }
        for line in &log {
            assert!(!line.to_string().contains(REDIS_PASSWORD), "{line}");
        }
    }
}

// ── A boot-time URL's credentials never reach the boot log ───────────────────
mod url_credentials {
    use std::sync::Arc;

    use crate::log_capture::capture;

    const SECRET: &str = "pw-boot-5d26";

    /// The Redis the redis leg binds, with `default:SECRET@` userinfo. A Redis whose
    /// `default` user is `nopass` accepts any password, so the connection succeeds.
    #[cfg(any(feature = "redis-pkce", feature = "redis-rate-limiting"))]
    fn redis_with_password() -> String {
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        crate::log_capture::with_userinfo(&url, "default", SECRET)
    }

    #[cfg(all(feature = "auth", feature = "redis-pkce"))]
    #[tokio::test]
    #[ignore = "requires Redis (REDIS_URL) — run with --ignored"]
    async fn the_pkce_redis_log_hides_the_password() {
        let pkce = serde_json::from_value(
            serde_json::json!({ "enabled": true, "redis_url": redis_with_password() }),
        )
        .unwrap();
        let schema = fraiseql_core::schema::CompiledSchema {
            security: Some(fraiseql_core::schema::SecurityConfig {
                pkce: Some(pkce),
                ..fraiseql_core::schema::SecurityConfig::default()
            }),
            ..fraiseql_core::schema::CompiledSchema::default()
        };
        let enc = Arc::new(crate::auth::state_encryption::StateEncryptionService::from_raw_key(
            &[7u8; 32],
            crate::auth::state_encryption::EncryptionAlgorithm::Aes256Gcm,
        ));
        let (captured, _guard) = capture();
        let store = super::super::pkce_store_from_schema_in(
            &schema,
            Some(&enc),
            fraiseql_guard::deployment::Deployment::Development,
        )
        .await
        .unwrap();
        assert!(store.is_some_and(|s| !s.is_in_memory()), "the Redis backend was not used");
        let log = captured.text();
        assert!(
            log.contains("PKCE state store: Redis backend"),
            "the backend log was not reached:\n{log}"
        );
        assert!(!log.contains(SECRET), "the backend log wrote the Redis password:\n{log}");
    }

    #[cfg(feature = "redis-rate-limiting")]
    #[tokio::test]
    #[ignore = "requires Redis (REDIS_URL) — run with --ignored"]
    async fn the_rate_limiter_redis_log_hides_the_password() {
        let sec = crate::middleware::RateLimitingSecurityConfig {
            redis_url: Some(redis_with_password()),
            ..crate::middleware::RateLimitingSecurityConfig::default()
        };
        let (captured, _guard) = capture();
        super::super::build_rate_limiter(
            crate::middleware::RateLimitConfig::default(),
            &sec,
            Some(&sec),
            fraiseql_guard::deployment::Deployment::Development,
        )
        .await
        .unwrap();
        let log = captured.text();
        assert!(
            log.contains("using Redis distributed backend"),
            "the backend log was not reached:\n{log}"
        );
        assert!(!log.contains(SECRET), "the backend log wrote the Redis password:\n{log}");
    }

    #[tokio::test]
    async fn the_manifest_address_refusal_log_hides_the_query_token() {
        let mut tasks = tokio::task::JoinSet::new();
        let (captured, _guard) = capture();
        crate::server::Server::spawn_trusted_docs_reload(
            Arc::new(crate::trusted_documents::TrustedDocumentStore::disabled()),
            format!("http://127.0.0.1/manifest.json?token={SECRET}"),
            60,
            &mut tasks,
        );
        let log = captured.text();
        assert!(log.contains("SSRF protection"), "the refusal log was not reached:\n{log}");
        assert!(!log.contains(SECRET), "the refusal log wrote the query token:\n{log}");
    }

    #[tokio::test]
    async fn the_manifest_fetch_failure_log_hides_the_query_token() {
        let mut tasks = tokio::task::JoinSet::new();
        let (captured, _guard) = capture();
        // `.invalid` never resolves (RFC 2606), so the first fetch fails in reqwest, whose
        // error text names the request URL.
        crate::server::Server::spawn_trusted_docs_reload(
            Arc::new(crate::trusted_documents::TrustedDocumentStore::disabled()),
            format!("http://manifest.invalid/manifest.json?token={SECRET}"),
            60,
            &mut tasks,
        );
        let mut log = captured.text();
        for _ in 0..200 {
            if log.contains("Failed to fetch trusted documents manifest") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            log = captured.text();
        }
        assert!(
            log.contains("Failed to fetch trusted documents manifest"),
            "the fetch-failure log was not reached:\n{log}"
        );
        assert!(!log.contains(SECRET), "the fetch-failure log wrote the query token:\n{log}");
    }
}
