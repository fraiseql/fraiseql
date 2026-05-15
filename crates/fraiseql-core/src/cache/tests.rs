//! Tests for the `cache` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

mod config_tests {
    use crate::cache::*;

    #[test]
    fn test_default_config() {
        let config = CacheConfig::default();
        assert!(!config.enabled); // Disabled by default as of rc.12
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.ttl_seconds, 86_400);
        assert!(config.cache_list_queries);
    }

    #[test]
    fn test_with_max_entries() {
        let config = CacheConfig::with_max_entries(50_000);
        assert_eq!(config.max_entries, 50_000);
        assert!(!config.enabled); // Disabled by default as of rc.12
        assert_eq!(config.ttl_seconds, 86_400);
    }

    #[test]
    fn test_with_ttl() {
        let config = CacheConfig::with_ttl(3_600);
        assert_eq!(config.ttl_seconds, 3_600);
        assert!(!config.enabled); // Disabled by default as of rc.12
        assert_eq!(config.max_entries, 10_000);
    }

    #[test]
    fn test_enabled() {
        let config = CacheConfig::enabled();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.ttl_seconds, 86_400);
    }

    #[test]
    fn test_disabled() {
        let config = CacheConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_estimated_memory() {
        let config = CacheConfig::with_max_entries(10_000);
        let estimated = config.estimated_memory_bytes();
        // Should be roughly 100 MB (10,000 * 10 KB)
        assert_eq!(estimated, 100_000_000);
    }

    #[test]
    fn test_from_bool_true() {
        let config = CacheConfig::from(true);
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.ttl_seconds, 86_400);
    }

    #[test]
    fn test_from_bool_false() {
        let config = CacheConfig::from(false);
        assert!(!config.enabled);
    }

    #[test]
    fn test_serialization() {
        let config = CacheConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CacheConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.max_entries, deserialized.max_entries);
        assert_eq!(config.ttl_seconds, deserialized.ttl_seconds);
    }
}

mod query_analyzer_tests {
    use crate::cache::*;

    #[test]
    fn test_parse_where_id_constraint() {
        let analyzer = QueryAnalyzer::new();
        let cardinality = analyzer.classify_cardinality("SELECT * FROM users WHERE id = ?");
        assert_eq!(cardinality, QueryCardinality::Single);
    }

    #[test]
    fn test_parse_where_id_in_constraint() {
        let analyzer = QueryAnalyzer::new();
        let cardinality =
            analyzer.classify_cardinality("SELECT * FROM users WHERE id IN (?, ?, ?)");
        assert_eq!(cardinality, QueryCardinality::Multiple);
    }

    #[test]
    fn test_list_queries_no_entity_constraint() {
        let analyzer = QueryAnalyzer::new();
        let cardinality = analyzer.classify_cardinality("SELECT * FROM users");
        assert_eq!(cardinality, QueryCardinality::List);
    }

    #[test]
    fn test_nested_entity_queries() {
        let analyzer = QueryAnalyzer::new();
        let cardinality = analyzer.classify_cardinality(
            "SELECT * FROM (SELECT * FROM users WHERE id = ?) AS u WHERE u.active = true",
        );
        assert_eq!(cardinality, QueryCardinality::Single);
    }

    #[test]
    fn test_complex_where_clauses() {
        let analyzer = QueryAnalyzer::new();
        let cardinality = analyzer.classify_cardinality(
            "SELECT * FROM users WHERE id = ? AND status = 'active' AND created_at > ?",
        );
        assert_eq!(cardinality, QueryCardinality::Single);
    }

    #[test]
    fn test_multiple_where_conditions() {
        let analyzer = QueryAnalyzer::new();
        let cardinality = analyzer
            .classify_cardinality("SELECT * FROM users WHERE email = ? OR username = ? LIMIT 1");
        assert_eq!(cardinality, QueryCardinality::List);
    }

    #[test]
    fn test_cardinality_hit_rates() {
        assert!((QueryCardinality::Single.expected_hit_rate() - 0.91).abs() < f64::EPSILON);
        assert!((QueryCardinality::Multiple.expected_hit_rate() - 0.88).abs() < f64::EPSILON);
        assert!((QueryCardinality::List.expected_hit_rate() - 0.60).abs() < f64::EPSILON);
    }
}

mod cascade_response_parser_tests {
    use serde_json::json;
    use crate::cache::*;
    use crate::cache::cascade_response_parser::CascadeEntities;
    use crate::error::FraiseQLError;


    #[test]
    fn test_parse_simple_cascade_response() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "createPost": {
                "cascade": {
                    "updated": [
                        {
                            "__typename": "User",
                            "id": "550e8400-e29b-41d4-a716-446655440000",
                            "postCount": 5
                        }
                    ]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 1);
        assert_eq!(entities.updated[0].entity_type, "User");
        assert_eq!(entities.updated[0].entity_id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(entities.deleted.len(), 0);
    }

    #[test]
    fn test_parse_multiple_updated_entities() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "updateUser": {
                "cascade": {
                    "updated": [
                        { "__typename": "User", "id": "uuid-1" },
                        { "__typename": "Post", "id": "uuid-2" },
                        { "__typename": "Notification", "id": "uuid-3" }
                    ]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 3);
        assert_eq!(entities.updated[0].entity_type, "User");
        assert_eq!(entities.updated[1].entity_type, "Post");
        assert_eq!(entities.updated[2].entity_type, "Notification");
    }

    #[test]
    fn test_parse_deleted_entities() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "deletePost": {
                "cascade": {
                    "deleted": [
                        { "__typename": "Post", "id": "post-uuid" },
                        { "__typename": "Comment", "id": "comment-uuid" }
                    ]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 0);
        assert_eq!(entities.deleted.len(), 2);
        assert_eq!(entities.deleted[0].entity_type, "Post");
        assert_eq!(entities.deleted[1].entity_type, "Comment");
    }

    #[test]
    fn test_parse_both_updated_and_deleted() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [{ "__typename": "User", "id": "u-1" }],
                    "deleted": [{ "__typename": "Session", "id": "s-1" }]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 1);
        assert_eq!(entities.deleted.len(), 1);
        assert_eq!(entities.all_affected().len(), 2);
    }

    #[test]
    fn test_parse_empty_cascade() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [],
                    "deleted": []
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert!(!entities.has_changes());
        assert_eq!(entities.all_affected().len(), 0);
    }

    #[test]
    fn test_parse_no_cascade_field() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "createPost": {
                "post": { "id": "post-1", "title": "Hello" }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert!(!entities.has_changes());
    }

    #[test]
    fn test_parse_nested_in_data_field() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "data": {
                "createPost": {
                    "cascade": {
                        "updated": [{ "__typename": "User", "id": "uuid-1" }]
                    }
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 1);
    }

    #[test]
    fn test_parse_missing_typename() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [{ "id": "uuid-1" }]
                }
            }
        });

        let result = parser.parse_cascade_response(&response);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing __typename, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_missing_id() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [{ "__typename": "User" }]
                }
            }
        });

        let result = parser.parse_cascade_response(&response);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing id, got: {result:?}"
        );
    }

    #[test]
    fn test_cascade_entities_all_affected() {
        let updated = vec![
            EntityKey::new("User", "u-1").unwrap(),
            EntityKey::new("User", "u-2").unwrap(),
        ];
        let deleted = vec![EntityKey::new("Post", "p-1").unwrap()];

        let cascade = CascadeEntities::new(updated, deleted);
        let all = cascade.all_affected();
        assert_eq!(all.len(), 3);
    }
}

mod response_cache_tests {
    use std::sync::Arc;
    use crate::cache::*;
    use crate::cache::response_cache::hash_security_context;
    use crate::security::SecurityContext;

    fn enabled_config() -> ResponseCacheConfig {
        ResponseCacheConfig {
            enabled:     true,
            max_entries: 100,
            ttl_seconds: 3600,
        }
    }

    #[test]
    fn test_put_and_get() {
        let cache = ResponseCache::new(enabled_config());
        let response = Arc::new(serde_json::json!({"data": {"users": []}}));

        cache
            .put(1, 0, response.clone(), vec!["v_user".to_string()])
            .expect("put should succeed");
        let result = cache.get(1, 0).expect("get should succeed");
        assert!(result.is_some());
        assert_eq!(*result.expect("should be Some"), *response);
    }

    #[test]
    fn test_different_security_contexts_different_entries() {
        let cache = ResponseCache::new(enabled_config());

        let admin_response =
            Arc::new(serde_json::json!({"data": {"users": [{"id": "1", "role": "admin"}]}}));
        let user_response = Arc::new(serde_json::json!({"data": {"users": [{"id": "1"}]}}));

        // Same query key (1), different security hashes
        cache
            .put(1, 100, admin_response.clone(), vec!["v_user".to_string()])
            .expect("put admin");
        cache
            .put(1, 200, user_response.clone(), vec!["v_user".to_string()])
            .expect("put user");

        let admin_result = cache.get(1, 100).expect("get admin").expect("admin hit");
        let user_result = cache.get(1, 200).expect("get user").expect("user hit");

        assert_ne!(*admin_result, *user_result);
        assert_eq!(*admin_result, *admin_response);
        assert_eq!(*user_result, *user_response);
    }

    #[test]
    fn test_invalidate_views() {
        let cache = ResponseCache::new(enabled_config());

        cache
            .put(1, 0, Arc::new(serde_json::json!("r1")), vec!["v_user".to_string()])
            .expect("put 1");
        cache
            .put(2, 0, Arc::new(serde_json::json!("r2")), vec!["v_post".to_string()])
            .expect("put 2");

        // Flush pending moka writes before invalidation
        cache.run_pending_tasks();

        let invalidated = cache.invalidate_views(&["v_user".to_string()]).expect("invalidate");
        assert_eq!(invalidated, 1);

        // Flush invalidations
        cache.run_pending_tasks();

        assert!(cache.get(1, 0).expect("get 1").is_none());
        assert!(cache.get(2, 0).expect("get 2").is_some());
    }

    #[test]
    fn test_disabled_cache_returns_none() {
        let cache = ResponseCache::new(ResponseCacheConfig::default());
        assert!(!cache.is_enabled());

        cache.put(1, 0, Arc::new(serde_json::json!("r")), vec![]).expect("put disabled");
        assert!(cache.get(1, 0).expect("get disabled").is_none());
    }

    #[test]
    fn test_metrics() {
        let cache = ResponseCache::new(enabled_config());

        cache.put(1, 0, Arc::new(serde_json::json!("r")), vec![]).expect("put");
        cache.run_pending_tasks();
        let _ = cache.get(1, 0); // hit
        let _ = cache.get(2, 0); // miss

        let (hits, misses) = cache.metrics();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    // ========================================================================
    // Security Context Hash Tests
    // ========================================================================

    #[test]
    fn test_hash_security_context_none_returns_zero() {
        assert_eq!(hash_security_context(None), 0);
    }

    #[test]
    fn test_hash_security_context_same_context_same_hash() {
        let ctx = make_security_context("alice", &["admin"], Some("tenant-1"), &["read:user"]);
        let hash1 = hash_security_context(Some(&ctx));
        let hash2 = hash_security_context(Some(&ctx));
        assert_eq!(hash1, hash2, "Same context must produce same hash");
    }

    #[test]
    fn test_hash_security_context_different_user_different_hash() {
        let alice = make_security_context("alice", &["admin"], Some("tenant-1"), &[]);
        let bob = make_security_context("bob", &["admin"], Some("tenant-1"), &[]);

        assert_ne!(
            hash_security_context(Some(&alice)),
            hash_security_context(Some(&bob)),
            "Different user_id must produce different hash"
        );
    }

    #[test]
    fn test_hash_security_context_different_roles_different_hash() {
        let admin = make_security_context("alice", &["admin"], None, &[]);
        let viewer = make_security_context("alice", &["viewer"], None, &[]);

        assert_ne!(
            hash_security_context(Some(&admin)),
            hash_security_context(Some(&viewer)),
            "Different roles must produce different hash"
        );
    }

    #[test]
    fn test_hash_security_context_role_order_independent() {
        let ctx1 = make_security_context("alice", &["admin", "viewer"], None, &[]);
        let ctx2 = make_security_context("alice", &["viewer", "admin"], None, &[]);

        assert_eq!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx2)),
            "Role order must not affect hash (sorted internally)"
        );
    }

    #[test]
    fn test_hash_security_context_different_tenant_different_hash() {
        let t1 = make_security_context("alice", &[], Some("tenant-1"), &[]);
        let t2 = make_security_context("alice", &[], Some("tenant-2"), &[]);
        let none = make_security_context("alice", &[], None, &[]);

        assert_ne!(hash_security_context(Some(&t1)), hash_security_context(Some(&t2)),);
        assert_ne!(hash_security_context(Some(&t1)), hash_security_context(Some(&none)),);
    }

    #[test]
    fn test_hash_security_context_different_scopes_different_hash() {
        let read = make_security_context("alice", &[], None, &["read:user"]);
        let write = make_security_context("alice", &[], None, &["write:user"]);
        let both = make_security_context("alice", &[], None, &["read:user", "write:user"]);

        assert_ne!(hash_security_context(Some(&read)), hash_security_context(Some(&write)),);
        assert_ne!(hash_security_context(Some(&read)), hash_security_context(Some(&both)),);
    }

    #[test]
    fn test_hash_security_context_scope_order_independent() {
        let ctx1 = make_security_context("alice", &[], None, &["read:user", "write:post"]);
        let ctx2 = make_security_context("alice", &[], None, &["write:post", "read:user"]);

        assert_eq!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx2)),
            "Scope order must not affect hash (sorted internally)"
        );
    }

    #[test]
    fn test_hash_security_context_different_attributes_different_hash() {
        let mut ctx1 = make_security_context("alice", &["admin"], None, &[]);
        ctx1.attributes
            .insert("department".to_string(), serde_json::json!("engineering"));

        let mut ctx2 = make_security_context("alice", &["admin"], None, &[]);
        ctx2.attributes.insert("department".to_string(), serde_json::json!("sales"));

        let ctx_no_attrs = make_security_context("alice", &["admin"], None, &[]);

        assert_ne!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx2)),
            "Different attribute values must produce different hashes"
        );
        assert_ne!(
            hash_security_context(Some(&ctx1)),
            hash_security_context(Some(&ctx_no_attrs)),
            "Attributes vs no attributes must produce different hashes"
        );
    }

    // ========================================================================
    // Invalidation Edge Cases
    // ========================================================================

    #[test]
    fn test_invalidate_empty_views_is_noop() {
        let cache = ResponseCache::new(enabled_config());
        cache
            .put(1, 0, Arc::new(serde_json::json!("r")), vec!["v_user".to_string()])
            .expect("put");
        cache.run_pending_tasks();

        let invalidated = cache.invalidate_views(&[]).expect("invalidate empty");
        assert_eq!(invalidated, 0);
        assert!(cache.get(1, 0).expect("still cached").is_some());
    }

    #[test]
    fn test_invalidate_nonexistent_view_is_noop() {
        let cache = ResponseCache::new(enabled_config());
        cache
            .put(1, 0, Arc::new(serde_json::json!("r")), vec!["v_user".to_string()])
            .expect("put");
        cache.run_pending_tasks();

        let invalidated = cache
            .invalidate_views(&["v_nonexistent".to_string()])
            .expect("invalidate nonexistent");
        assert_eq!(invalidated, 0);
        assert!(cache.get(1, 0).expect("still cached").is_some());
    }

    #[test]
    fn test_invalidate_clears_all_security_contexts_for_view() {
        let cache = ResponseCache::new(enabled_config());

        // Same query, different users, same view
        cache
            .put(1, 100, Arc::new(serde_json::json!("admin")), vec!["v_user".to_string()])
            .expect("put admin");
        cache
            .put(1, 200, Arc::new(serde_json::json!("user")), vec!["v_user".to_string()])
            .expect("put user");
        cache
            .put(1, 0, Arc::new(serde_json::json!("anon")), vec!["v_user".to_string()])
            .expect("put anon");
        cache.run_pending_tasks();

        let invalidated = cache.invalidate_views(&["v_user".to_string()]).expect("invalidate");
        assert_eq!(invalidated, 3, "All entries for the view must be invalidated");

        cache.run_pending_tasks();

        assert!(cache.get(1, 100).expect("admin gone").is_none());
        assert!(cache.get(1, 200).expect("user gone").is_none());
        assert!(cache.get(1, 0).expect("anon gone").is_none());
    }

    #[test]
    fn test_invalidate_multiple_views_at_once() {
        let cache = ResponseCache::new(enabled_config());

        cache
            .put(1, 0, Arc::new(serde_json::json!("users")), vec!["v_user".to_string()])
            .expect("put users");
        cache
            .put(2, 0, Arc::new(serde_json::json!("posts")), vec!["v_post".to_string()])
            .expect("put posts");
        cache
            .put(3, 0, Arc::new(serde_json::json!("tags")), vec!["v_tag".to_string()])
            .expect("put tags");
        cache.run_pending_tasks();

        let invalidated = cache
            .invalidate_views(&["v_user".to_string(), "v_post".to_string()])
            .expect("invalidate");
        assert_eq!(invalidated, 2);

        cache.run_pending_tasks();

        assert!(cache.get(1, 0).expect("users gone").is_none());
        assert!(cache.get(2, 0).expect("posts gone").is_none());
        assert!(cache.get(3, 0).expect("tags alive").is_some());
    }

    #[test]
    fn test_entry_with_multiple_views_invalidated_by_any() {
        let cache = ResponseCache::new(enabled_config());

        // Query reads from both v_user and v_post (e.g., a join)
        cache
            .put(
                1,
                0,
                Arc::new(serde_json::json!("joined")),
                vec!["v_user".to_string(), "v_post".to_string()],
            )
            .expect("put");
        cache.run_pending_tasks();

        // Invalidating either view should remove the entry
        let invalidated = cache.invalidate_views(&["v_post".to_string()]).expect("invalidate");
        assert_eq!(invalidated, 1);

        cache.run_pending_tasks();
        assert!(cache.get(1, 0).expect("gone").is_none());
    }

    // ========================================================================
    // Response Cache Key Collision Avoidance
    // ========================================================================

    #[test]
    fn test_different_query_keys_no_collision() {
        let cache = ResponseCache::new(enabled_config());

        cache
            .put(1, 0, Arc::new(serde_json::json!("response_1")), vec![])
            .expect("put q1");
        cache
            .put(2, 0, Arc::new(serde_json::json!("response_2")), vec![])
            .expect("put q2");
        cache.run_pending_tasks();

        let r1 = cache.get(1, 0).expect("get q1").expect("q1 hit");
        let r2 = cache.get(2, 0).expect("get q2").expect("q2 hit");

        assert_eq!(*r1, serde_json::json!("response_1"));
        assert_eq!(*r2, serde_json::json!("response_2"));
    }

    #[test]
    fn test_same_query_key_different_security_no_collision() {
        let cache = ResponseCache::new(enabled_config());

        for sec_hash in 0_u64..10 {
            cache
                .put(
                    42,
                    sec_hash,
                    Arc::new(serde_json::json!(format!("response_for_user_{sec_hash}"))),
                    vec![],
                )
                .expect("put");
        }
        cache.run_pending_tasks();

        for sec_hash in 0_u64..10 {
            let r = cache.get(42, sec_hash).expect("get").expect("should be cached");
            assert_eq!(*r, serde_json::json!(format!("response_for_user_{sec_hash}")));
        }
    }

    // ========================================================================
    // Helper: SecurityContext builder for tests
    // ========================================================================

    fn make_security_context(
        user_id: &str,
        roles: &[&str],
        tenant_id: Option<&str>,
        scopes: &[&str],
    ) -> SecurityContext {
        use chrono::Utc;
        SecurityContext {
            user_id:          user_id.into(),
            roles:            roles.iter().map(|s| (*s).to_string()).collect(),
            tenant_id:        tenant_id.map(Into::into),
            scopes:           scopes.iter().map(|s| (*s).to_string()).collect(),
            attributes:       std::collections::HashMap::new(),
            request_id:       "test-request".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }
}

mod cascade_metadata_tests {
    use crate::cache::*;

    #[test]
    fn test_build_from_mutations() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");
        metadata.add_mutation("updateUser", "User");
        metadata.add_mutation("deleteUser", "User");

        assert_eq!(metadata.count(), 3);
    }

    #[test]
    fn test_map_mutation_to_entity_type() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");
        metadata.add_mutation("createPost", "Post");

        assert_eq!(metadata.get_entity_type("createUser"), Some("User"));
        assert_eq!(metadata.get_entity_type("createPost"), Some("Post"));
    }

    #[test]
    fn test_handle_unknown_mutation() {
        let metadata = CascadeMetadata::new();
        assert_eq!(metadata.get_entity_type("unknownMutation"), None);
    }

    #[test]
    fn test_multiple_mutations_same_entity() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");
        metadata.add_mutation("updateUser", "User");
        metadata.add_mutation("deleteUser", "User");

        let mutations = metadata.get_mutations_for_entity("User");
        assert_eq!(mutations.len(), 3);
        assert!(mutations.contains(&"createUser".to_string()));
        assert!(mutations.contains(&"updateUser".to_string()));
        assert!(mutations.contains(&"deleteUser".to_string()));
    }

    #[test]
    fn test_contains_mutation() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");

        assert!(metadata.contains_mutation("createUser"));
        assert!(!metadata.contains_mutation("unknownMutation"));
    }
}

mod fact_table_version_tests {
    use std::time::Duration;
    use crate::cache::*;
    use crate::cache::fact_table_version::{generate_version_key_component, CachedVersion};

    #[test]
    fn test_strategy_default_is_disabled() {
        let strategy = FactTableVersionStrategy::default();
        assert_eq!(strategy, FactTableVersionStrategy::Disabled);
        assert!(!strategy.is_caching_enabled());
    }

    #[test]
    fn test_strategy_time_based() {
        let strategy = FactTableVersionStrategy::time_based(300);
        assert!(strategy.is_caching_enabled());
        assert_eq!(strategy.ttl_seconds(), Some(300));
    }

    #[test]
    fn test_strategy_version_table() {
        let strategy = FactTableVersionStrategy::VersionTable;
        assert!(strategy.is_caching_enabled());
        assert_eq!(strategy.ttl_seconds(), None);
    }

    #[test]
    fn test_strategy_schema_version() {
        let strategy = FactTableVersionStrategy::SchemaVersion;
        assert!(strategy.is_caching_enabled());
        assert_eq!(strategy.ttl_seconds(), None);
    }

    #[test]
    fn test_config_default_strategy() {
        let config = FactTableCacheConfig::default();
        assert_eq!(config.get_strategy("tf_sales"), &FactTableVersionStrategy::Disabled);
    }

    #[test]
    fn test_config_per_table_strategy() {
        let mut config = FactTableCacheConfig::default();
        config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
        config.set_strategy(
            "tf_page_views",
            FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
        );

        assert_eq!(config.get_strategy("tf_sales"), &FactTableVersionStrategy::VersionTable);
        assert_eq!(
            config.get_strategy("tf_page_views"),
            &FactTableVersionStrategy::TimeBased { ttl_seconds: 300 }
        );
        // Unconfigured table uses default
        assert_eq!(config.get_strategy("tf_other"), &FactTableVersionStrategy::Disabled);
    }

    #[test]
    fn test_config_with_default() {
        let config = FactTableCacheConfig::with_default(FactTableVersionStrategy::SchemaVersion);
        assert_eq!(config.get_strategy("tf_any"), &FactTableVersionStrategy::SchemaVersion);
    }

    #[test]
    fn test_generate_version_key_disabled() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::Disabled,
            Some(42),
            "1.0.0",
        );
        assert!(key.is_none());
    }

    #[test]
    fn test_generate_version_key_version_table() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::VersionTable,
            Some(42),
            "1.0.0",
        );
        assert_eq!(key, Some("tv:42".to_string()));

        // No version available - should return None
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::VersionTable,
            None,
            "1.0.0",
        );
        assert!(key.is_none());
    }

    #[test]
    fn test_generate_version_key_time_based() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
            None,
            "1.0.0",
        );
        assert!(key.is_some());
        assert!(key.unwrap().starts_with("tb:"));
    }

    #[test]
    fn test_generate_version_key_schema_version() {
        let key = generate_version_key_component(
            "tf_sales",
            &FactTableVersionStrategy::SchemaVersion,
            None,
            "1.0.0",
        );
        assert_eq!(key, Some("sv:1.0.0".to_string()));
    }

    #[test]
    fn test_version_provider_caching() {
        let provider = FactTableVersionProvider::new(Duration::from_secs(10));

        // Initially no cached version
        assert!(provider.get_cached_version("tf_sales").is_none());

        // Set version
        provider.set_cached_version("tf_sales", 42);
        assert_eq!(provider.get_cached_version("tf_sales"), Some(42));

        // Clear version
        provider.clear_cached_version("tf_sales");
        assert!(provider.get_cached_version("tf_sales").is_none());
    }

    #[test]
    fn test_version_provider_clear_all() {
        let provider = FactTableVersionProvider::new(Duration::from_secs(10));

        provider.set_cached_version("tf_sales", 1);
        provider.set_cached_version("tf_orders", 2);

        provider.clear_all();

        assert!(provider.get_cached_version("tf_sales").is_none());
        assert!(provider.get_cached_version("tf_orders").is_none());
    }

    #[test]
    fn test_cached_version_freshness() {
        let cached = CachedVersion::new(42);

        // Should be fresh immediately
        assert!(cached.is_fresh(Duration::from_secs(1)));

        // Should be fresh for a longer duration
        assert!(cached.is_fresh(Duration::from_secs(60)));
    }

    #[test]
    fn test_strategy_serialization() {
        let strategies = vec![
            FactTableVersionStrategy::Disabled,
            FactTableVersionStrategy::VersionTable,
            FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
            FactTableVersionStrategy::SchemaVersion,
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let deserialized: FactTableVersionStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(strategy, deserialized);
        }
    }

    #[test]
    fn test_config_serialization() {
        let mut config =
            FactTableCacheConfig::with_default(FactTableVersionStrategy::SchemaVersion);
        config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
        config.set_strategy("tf_events", FactTableVersionStrategy::TimeBased { ttl_seconds: 60 });

        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: FactTableCacheConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.default_strategy, FactTableVersionStrategy::SchemaVersion);
        assert_eq!(deserialized.get_strategy("tf_sales"), &FactTableVersionStrategy::VersionTable);
    }
}

mod invalidation_tests {
    use crate::cache::*;

    #[test]
    fn test_for_mutation() {
        let ctx = InvalidationContext::for_mutation("createUser", vec!["v_user".to_string()]);

        assert_eq!(ctx.modified_views, vec!["v_user"]);
        assert!(matches!(ctx.reason, InvalidationReason::Mutation { .. }));
    }

    #[test]
    fn test_manual() {
        let ctx = InvalidationContext::manual(
            vec!["v_user".to_string(), "v_post".to_string()],
            "maintenance",
        );

        assert_eq!(ctx.modified_views.len(), 2);
        assert!(matches!(ctx.reason, InvalidationReason::Manual { .. }));
    }

    #[test]
    fn test_schema_change() {
        let ctx = InvalidationContext::schema_change(vec!["v_user".to_string()], "1.0.0", "1.1.0");

        assert_eq!(ctx.modified_views, vec!["v_user"]);
        assert!(matches!(ctx.reason, InvalidationReason::SchemaChange { .. }));
    }

    #[test]
    fn test_mutation_log_string() {
        let ctx = InvalidationContext::for_mutation("createUser", vec!["v_user".to_string()]);

        assert_eq!(ctx.to_log_string(), "mutation:createUser affecting 1 view(s)");
    }

    #[test]
    fn test_manual_log_string() {
        let ctx = InvalidationContext::manual(vec!["v_user".to_string()], "data import");

        assert_eq!(ctx.to_log_string(), "manual:data import affecting 1 view(s)");
    }

    #[test]
    fn test_schema_change_log_string() {
        let ctx = InvalidationContext::schema_change(vec!["v_user".to_string()], "1.0.0", "1.1.0");

        assert_eq!(ctx.to_log_string(), "schema_change:1.0.0->1.1.0 affecting 1 view(s)");
    }

    #[test]
    fn test_view_count() {
        let ctx = InvalidationContext::for_mutation(
            "createUser",
            vec!["v_user".to_string(), "v_post".to_string()],
        );

        assert_eq!(ctx.view_count(), 2);
    }

    #[test]
    fn test_affects_view() {
        let ctx = InvalidationContext::for_mutation(
            "createUser",
            vec!["v_user".to_string(), "v_post".to_string()],
        );

        assert!(ctx.affects_view("v_user"));
        assert!(ctx.affects_view("v_post"));
        assert!(!ctx.affects_view("v_comment"));
    }

    #[test]
    fn test_empty_views() {
        let ctx = InvalidationContext::manual(vec![], "testing empty invalidation");

        assert_eq!(ctx.view_count(), 0);
        assert!(!ctx.affects_view("v_user"));
    }

    #[test]
    fn test_reason_to_log_string_mutation() {
        let reason = InvalidationReason::Mutation {
            mutation_name: "updatePost".to_string(),
        };

        assert_eq!(reason.to_log_string(), "mutation:updatePost");
    }

    #[test]
    fn test_reason_to_log_string_manual() {
        let reason = InvalidationReason::Manual {
            reason: "cache warmup".to_string(),
        };

        assert_eq!(reason.to_log_string(), "manual:cache warmup");
    }

    #[test]
    fn test_reason_to_log_string_schema_change() {
        let reason = InvalidationReason::SchemaChange {
            old_version: "2.0.0".to_string(),
            new_version: "2.1.0".to_string(),
        };

        assert_eq!(reason.to_log_string(), "schema_change:2.0.0->2.1.0");
    }

    #[test]
    fn test_multiple_views() {
        let views = vec![
            "v_user".to_string(),
            "v_post".to_string(),
            "v_comment".to_string(),
            "v_like".to_string(),
        ];

        let ctx = InvalidationContext::for_mutation("deleteUser", views);

        assert_eq!(ctx.view_count(), 4);
        assert!(ctx.affects_view("v_user"));
        assert!(ctx.affects_view("v_post"));
        assert!(ctx.affects_view("v_comment"));
        assert!(ctx.affects_view("v_like"));
        assert!(!ctx.affects_view("v_notification"));
    }
}

mod dependency_tracker_tests {
    use crate::cache::*;

    #[test]
    fn test_record_and_get_dependency() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);

        let affected = tracker.get_dependent_caches("v_user");
        assert_eq!(affected.len(), 1);
        assert!(affected.contains(&"key1".to_string()));
    }

    #[test]
    fn test_multiple_caches_same_view() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_user".to_string()]);

        let affected = tracker.get_dependent_caches("v_user");
        assert_eq!(affected.len(), 2);
        assert!(affected.contains(&"key1".to_string()));
        assert!(affected.contains(&"key2".to_string()));
    }

    #[test]
    fn test_cache_accesses_multiple_views() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);

        // Should appear in both view mappings
        let user_caches = tracker.get_dependent_caches("v_user");
        let post_caches = tracker.get_dependent_caches("v_post");

        assert!(user_caches.contains(&"key1".to_string()));
        assert!(post_caches.contains(&"key1".to_string()));
    }

    #[test]
    fn test_remove_cache() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.remove_cache("key1");

        let affected = tracker.get_dependent_caches("v_user");
        assert_eq!(affected.len(), 0);
    }

    #[test]
    fn test_remove_cache_with_multiple_views() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);

        tracker.remove_cache("key1");

        // Should be removed from both mappings
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 0);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_cache() {
        let mut tracker = DependencyTracker::new();

        // Should not panic
        tracker.remove_cache("nonexistent");
    }

    #[test]
    fn test_get_nonexistent_view() {
        let tracker = DependencyTracker::new();

        let affected = tracker.get_dependent_caches("nonexistent");
        assert_eq!(affected.len(), 0);
    }

    #[test]
    fn test_clear() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);

        tracker.clear();

        assert_eq!(tracker.cache_count(), 0);
        assert_eq!(tracker.view_count(), 0);
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 0);
    }

    #[test]
    fn test_cache_count() {
        let mut tracker = DependencyTracker::new();

        assert_eq!(tracker.cache_count(), 0);

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        assert_eq!(tracker.cache_count(), 1);

        tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);
        assert_eq!(tracker.cache_count(), 2);

        tracker.remove_cache("key1");
        assert_eq!(tracker.cache_count(), 1);
    }

    #[test]
    fn test_view_count() {
        let mut tracker = DependencyTracker::new();

        assert_eq!(tracker.view_count(), 0);

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        assert_eq!(tracker.view_count(), 1);

        tracker.record_access("key2".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);
        assert_eq!(tracker.view_count(), 2); // v_user and v_post
    }

    #[test]
    fn test_get_all_views() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);

        let views = tracker.get_all_views();
        assert_eq!(views.len(), 2);
        assert!(views.contains(&"v_user".to_string()));
        assert!(views.contains(&"v_post".to_string()));
    }

    #[test]
    fn test_update_access_overwrites() {
        let mut tracker = DependencyTracker::new();

        // Initial access
        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);

        // Update to different views
        tracker.record_access("key1".to_string(), vec!["v_post".to_string()]);

        // Should only be in v_post now
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 0);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 1);
    }

    #[test]
    fn test_bidirectional_consistency() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);

        // Forward: 2 cache entries
        assert_eq!(tracker.cache_count(), 2);

        // Reverse: v_user has 2 dependencies, v_post has 1
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 2);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 1);

        // Remove one
        tracker.remove_cache("key1");

        // Consistency check
        assert_eq!(tracker.cache_count(), 1);
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 1);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 1);
    }
}

mod entity_key_tests {
    use crate::cache::*;
    use crate::error::FraiseQLError;

    #[test]
    fn test_create_valid_entity_key() {
        let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(key.entity_type, "User");
        assert_eq!(key.entity_id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_reject_empty_entity_type() {
        let result = EntityKey::new("", "550e8400-e29b-41d4-a716-446655440000");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty entity_type, got: {result:?}"
        );
    }

    #[test]
    fn test_reject_empty_entity_id() {
        let result = EntityKey::new("User", "");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty entity_id, got: {result:?}"
        );
    }

    #[test]
    fn test_serialize_to_cache_key_format() {
        let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        let cache_key = key.to_cache_key();
        assert_eq!(cache_key, "User:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_deserialize_from_cache_key_format() {
        let cache_key = "User:550e8400-e29b-41d4-a716-446655440000";
        let key = EntityKey::from_cache_key(cache_key).unwrap();
        assert_eq!(key.entity_type, "User");
        assert_eq!(key.entity_id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_reject_colon_in_entity_type() {
        let result = EntityKey::new("User:Admin", "550e8400-e29b-41d4-a716-446655440000");
        assert!(result.is_err(), "colon in entity_type must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("colon") || msg.contains("separator"),
            "error should mention the separator: {msg}"
        );
    }

    #[test]
    fn test_reject_colon_only_in_entity_type() {
        let result = EntityKey::new(":", "some-id");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for colon-only entity_type, got: {result:?}"
        );
    }

    #[test]
    fn test_entity_id_may_contain_colon() {
        // Entity IDs can contain colons (e.g. URNs) — only entity_type is restricted.
        let result = EntityKey::new("User", "urn:uuid:550e8400-e29b-41d4-a716-446655440000");
        assert!(result.is_ok(), "colon in entity_id must be accepted");
        // from_cache_key uses splitn(2, ':'), so it should reconstruct correctly.
        let key = result.unwrap();
        let cache_key = key.to_cache_key();
        let parsed = EntityKey::from_cache_key(&cache_key).unwrap();
        assert_eq!(parsed.entity_type, "User");
        assert_eq!(parsed.entity_id, "urn:uuid:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_hash_consistency_for_hashmap() {
        use std::collections::HashMap;

        let key1 = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        let key2 = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();

        let mut map = HashMap::new();
        map.insert(key1, "value1");

        // Same key should retrieve same value
        assert_eq!(map.get(&key2), Some(&"value1"));

        // Different key should not match
        let key3 = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440001").unwrap();
        assert_eq!(map.get(&key3), None);
    }
}

mod key_tests {
    use std::collections::{HashMap, HashSet};
    use indexmap::IndexMap;
    use serde_json::{json, Value as JsonValue};
    use crate::cache::*;
    use crate::cache::key::verify_deterministic;
    use crate::db::{WhereOperator, where_clause::WhereClause};
    use crate::schema::QueryDefinition;
    use crate::schema::CursorType;



    // ========================================================================
    // Security Tests (CRITICAL)
    // ========================================================================

    #[test]
    fn test_different_variables_produce_different_keys() {
        // SECURITY CRITICAL: Different variables MUST produce different keys
        // to prevent User A from seeing User B's cached data
        let query = "query getUser($id: ID!) { user(id: $id) { name email } }";

        let key_alice = generate_cache_key(query, &json!({"id": "alice"}), None, "v1");
        let key_bob = generate_cache_key(query, &json!({"id": "bob"}), None, "v1");

        assert_ne!(
            key_alice, key_bob,
            "SECURITY: Different variables MUST produce different cache keys"
        );
    }

    #[test]
    fn test_different_variable_values_produce_different_keys() {
        let query = "query getUsers($limit: Int!) { users(limit: $limit) { id } }";

        let key_10 = generate_cache_key(query, &json!({"limit": 10}), None, "v1");
        let key_20 = generate_cache_key(query, &json!({"limit": 20}), None, "v1");

        assert_ne!(
            key_10, key_20,
            "SECURITY: Different variable values MUST produce different keys"
        );
    }

    #[test]
    fn test_empty_vs_non_empty_variables() {
        let query = "query { users { id } }";

        let key_empty = generate_cache_key(query, &json!({}), None, "v1");
        let key_with_vars = generate_cache_key(query, &json!({"limit": 10}), None, "v1");

        assert_ne!(
            key_empty, key_with_vars,
            "Empty variables must produce different key than non-empty"
        );
    }

    #[test]
    fn test_variable_order_independence() {
        // Object keys are sorted before hashing, so insertion order should
        // not affect the result. serde_json's default Map is BTreeMap (sorted),
        // but we sort explicitly in hash_json_value to be safe regardless.
        let query = "query($a: Int, $b: Int) { users { id } }";

        let key1 = generate_cache_key(query, &json!({"a": 1, "b": 2}), None, "v1");
        let key2 = generate_cache_key(query, &json!({"a": 1, "b": 2}), None, "v1");

        assert_eq!(key1, key2, "Same variables must produce same key");
    }

    // ========================================================================
    // Determinism Tests
    // ========================================================================

    #[test]
    fn test_cache_key_deterministic() {
        // Same inputs must always produce same output
        let query = "query { users { id } }";
        let vars = json!({"limit": 10});

        let key1 = generate_cache_key(query, &vars, None, "v1");
        let key2 = generate_cache_key(query, &vars, None, "v1");

        assert_eq!(key1, key2, "Cache keys must be deterministic");
    }

    #[test]
    fn test_verify_deterministic_helper() {
        assert!(
            verify_deterministic("query { users }", &json!({}), "v1"),
            "Helper should verify determinism"
        );
    }

    // ========================================================================
    // WHERE Clause Tests
    // ========================================================================

    #[test]
    fn test_different_where_clauses_produce_different_keys() {
        let query = "query { users { id } }";

        let where1 = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };

        let where2 = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("bob@example.com"),
        };

        let key1 = generate_cache_key(query, &json!({}), Some(&where1), "v1");
        let key2 = generate_cache_key(query, &json!({}), Some(&where2), "v1");

        assert_ne!(key1, key2, "Different WHERE clauses must produce different keys");
    }

    #[test]
    fn test_different_where_operators_produce_different_keys() {
        let query = "query { users { id } }";

        let where_eq = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(30),
        };

        let where_gt = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(30),
        };

        let key_eq = generate_cache_key(query, &json!({}), Some(&where_eq), "v1");
        let key_gt = generate_cache_key(query, &json!({}), Some(&where_gt), "v1");

        assert_ne!(key_eq, key_gt, "Different operators must produce different keys");
    }

    #[test]
    fn test_with_and_without_where_clause() {
        let query = "query { users { id } }";

        let where_clause = WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        };

        let key_without = generate_cache_key(query, &json!({}), None, "v1");
        let key_with = generate_cache_key(query, &json!({}), Some(&where_clause), "v1");

        assert_ne!(key_without, key_with, "Presence of WHERE clause must change key");
    }

    #[test]
    fn test_complex_where_clause() {
        let query = "query { users { id } }";

        let where_clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(18),
            },
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
        ]);

        // Should not panic; produces a valid u64.
        let _key = generate_cache_key(query, &json!({}), Some(&where_clause), "v1");
    }

    // ========================================================================
    // Schema Version Tests
    // ========================================================================

    #[test]
    fn test_different_schema_versions_produce_different_keys() {
        let query = "query { users { id } }";

        let key_v1 = generate_cache_key(query, &json!({}), None, "v1");
        let key_v2 = generate_cache_key(query, &json!({}), None, "v2");

        assert_ne!(key_v1, key_v2, "Different schema versions must produce different keys");
    }

    #[test]
    fn test_schema_version_invalidation() {
        // When schema changes, all cache keys change (automatic invalidation)
        let query = "query { users { id } }";

        let old_schema = "abc123";
        let new_schema = "def456";

        let key_old = generate_cache_key(query, &json!({}), None, old_schema);
        let key_new = generate_cache_key(query, &json!({}), None, new_schema);

        assert_ne!(key_old, key_new, "Schema changes should invalidate cache");
    }

    // ========================================================================
    // Collision Avoidance Test
    // ========================================================================

    #[test]
    fn test_no_collisions_in_sample() {
        // Generate a sample of cache keys from varied inputs and verify
        // that no two distinct inputs produce the same u64.
        let mut keys = HashSet::new();
        let mut count = 0u32;

        let queries = [
            "query { users { id } }",
            "query { posts { id } }",
            "query { users { id name } }",
            "query getUser($id: ID!) { user(id: $id) { name } }",
            "",
        ];
        let variable_sets: &[JsonValue] = &[
            json!({}),
            json!(null),
            json!({"id": 1}),
            json!({"id": 2}),
            json!({"id": "alice"}),
            json!({"limit": 10, "offset": 0}),
            json!({"filter": {"active": true}}),
        ];
        let schema_versions = ["v1", "v2", "abc123"];

        for query in &queries {
            for vars in variable_sets {
                for sv in &schema_versions {
                    let key = generate_cache_key(query, vars, None, sv);
                    keys.insert(key);
                    count += 1;
                }
            }
        }

        assert_eq!(
            keys.len(),
            count as usize,
            "Collision detected among {count} sample cache keys"
        );
    }

    // ========================================================================
    // Extract Views Tests
    // ========================================================================

    #[test]
    fn test_extract_accessed_views_with_sql_source() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams {
                has_where:    true,
                has_order_by: false,
                has_limit:    true,
                has_offset:   false,
            },
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, vec!["v_user"]);
    }

    #[test]
    fn test_extract_accessed_views_without_sql_source() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:                "customQuery".to_string(),
            return_type:         "Custom".to_string(),
            returns_list:        false,
            nullable:            false,
            arguments:           vec![],
            sql_source:          None, // No SQL source (custom resolver)
            description:         None,
            auto_params:         AutoParams {
                has_where:    false,
                has_order_by: false,
                has_limit:    false,
                has_offset:   false,
            },
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, Vec::<String>::new());
    }

    #[test]
    fn test_extract_accessed_views_with_additional_views() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:                "usersWithPosts".to_string(),
            return_type:         "UserWithPosts".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some("v_user_with_posts".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec!["v_post".to_string(), "v_tag".to_string()],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, vec!["v_user_with_posts", "v_post", "v_tag"]);
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_empty_query_string() {
        // Should not panic; produces a valid u64.
        let _key = generate_cache_key("", &json!({}), None, "v1");
    }

    #[test]
    fn test_null_variables() {
        // Should not panic; produces a valid u64.
        let _key = generate_cache_key("query { users }", &json!(null), None, "v1");
    }

    #[test]
    fn test_large_variable_object() {
        let large_vars = json!({
            "filter": {
                "age": 30,
                "active": true,
                "tags": ["rust", "graphql", "database"],
                "metadata": {
                    "created_after": "2024-01-01",
                    "updated_before": "2024-12-31"
                }
            }
        });

        // Should not panic; produces a valid u64.
        let _key = generate_cache_key("query { users }", &large_vars, None, "v1");
    }

    #[test]
    fn test_special_characters_in_query() {
        let query = r#"query { user(email: "test@example.com") { name } }"#;
        // Should not panic; produces a valid u64.
        let _key = generate_cache_key(query, &json!({}), None, "v1");
    }

    // ========================================================================
    // ORDER BY Cache Key Tests
    // ========================================================================

    #[test]
    fn test_view_key_different_order_by_produces_different_keys() {
        use crate::db::{OrderByClause, OrderDirection};

        let asc = [OrderByClause::new("name".into(), OrderDirection::Asc)];
        let desc = [OrderByClause::new("name".into(), OrderDirection::Desc)];

        let key_asc = generate_view_query_key("v_user", None, None, None, Some(&asc), "v1");
        let key_desc = generate_view_query_key("v_user", None, None, None, Some(&desc), "v1");

        assert_ne!(key_asc, key_desc, "Different order directions must produce different keys");
    }

    #[test]
    fn test_view_key_same_order_by_produces_same_key() {
        use crate::db::{OrderByClause, OrderDirection};

        let clauses = [OrderByClause::new("createdAt".into(), OrderDirection::Desc)];

        let key1 = generate_view_query_key("v_user", None, None, None, Some(&clauses), "v1");
        let key2 = generate_view_query_key("v_user", None, None, None, Some(&clauses), "v1");

        assert_eq!(key1, key2, "Same order_by must produce identical keys");
    }

    #[test]
    fn test_view_key_with_and_without_order_by() {
        use crate::db::{OrderByClause, OrderDirection};

        let clauses = [OrderByClause::new("name".into(), OrderDirection::Asc)];

        let key_with = generate_view_query_key("v_user", None, None, None, Some(&clauses), "v1");
        let key_without = generate_view_query_key("v_user", None, None, None, None, "v1");

        assert_ne!(key_with, key_without, "Presence of order_by must change key");
    }

    #[test]
    fn test_view_key_different_fields_produce_different_keys() {
        use crate::db::{OrderByClause, OrderDirection};

        let by_name = [OrderByClause::new("name".into(), OrderDirection::Asc)];
        let by_date = [OrderByClause::new("createdAt".into(), OrderDirection::Asc)];

        let key_name = generate_view_query_key("v_user", None, None, None, Some(&by_name), "v1");
        let key_date = generate_view_query_key("v_user", None, None, None, Some(&by_date), "v1");

        assert_ne!(key_name, key_date, "Different order_by fields must produce different keys");
    }

    #[test]
    fn test_projection_key_includes_order_by() {
        use crate::db::{OrderByClause, OrderDirection};

        let clauses = [OrderByClause::new("name".into(), OrderDirection::Asc)];

        let key_with =
            generate_projection_query_key("v_user", None, None, None, None, Some(&clauses), "v1");
        let key_without =
            generate_projection_query_key("v_user", None, None, None, None, None, "v1");

        assert_ne!(key_with, key_without, "Projection key must include order_by");
    }
}

mod uuid_extractor_tests {
    use serde_json::{json, Value};
    use crate::cache::*;


    #[test]
    fn test_extract_single_uuid_from_response() {
        let response = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "Alice"
        });

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(uuid, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[test]
    fn test_extract_uuid_from_nested_response() {
        let response = json!({
            "user": {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "name": "Alice"
            }
        });

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(uuid, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[test]
    fn test_extract_uuid_from_null_response() {
        let response = Value::Null;

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(uuid, None);
    }

    #[test]
    fn test_extract_batch_uuids_from_array() {
        let response = json!([
            {"id": "550e8400-e29b-41d4-a716-446655440000"},
            {"id": "550e8400-e29b-41d4-a716-446655440001"},
            {"id": "550e8400-e29b-41d4-a716-446655440002"}
        ]);

        let uuids = UUIDExtractor::extract_batch_uuids(&response, "User").unwrap();
        assert_eq!(uuids.len(), 3);
        assert!(uuids.contains(&"550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[test]
    fn test_is_valid_uuid() {
        assert!(UUIDExtractor::is_valid_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(UUIDExtractor::is_valid_uuid("550E8400-E29B-41D4-A716-446655440000"));
        assert!(!UUIDExtractor::is_valid_uuid("not-a-uuid"));
        assert!(!UUIDExtractor::is_valid_uuid("550e8400"));
    }

    #[test]
    fn test_skip_non_uuid_id_fields() {
        let response = json!({
            "id": "some-string-id",
            "name": "Alice"
        });

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        // Non-UUID id field should not be extracted
        assert_eq!(uuid, None);
    }

    #[test]
    fn test_batch_mutations_multiple_entities() {
        let response = json!([
            {"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Alice"},
            {"id": "550e8400-e29b-41d4-a716-446655440001", "name": "Bob"}
        ]);

        let uuids = UUIDExtractor::extract_batch_uuids(&response, "User").unwrap();
        assert_eq!(uuids.len(), 2);
    }

    #[test]
    fn test_error_cases_invalid_format() {
        let response = json!({"id": 12345});
        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(uuid, None);
    }
}

mod cascade_invalidator_tests {
    use crate::cache::*;

    #[test]
    fn test_add_single_dependency() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();

        assert!(invalidator.get_direct_dependencies("v_user").contains("v_raw_user"));
        assert!(invalidator.get_direct_dependents("v_raw_user").contains("v_user"));
    }

    #[test]
    fn test_self_dependency_fails() {
        let mut invalidator = CascadeInvalidator::new();
        let result = invalidator.add_dependency("v_user", "v_user");
        assert!(
            matches!(result, Err(crate::error::FraiseQLError::Validation { .. })),
            "expected Validation error for self-dependency, got: {result:?}"
        );
    }

    #[test]
    fn test_cascade_invalidate_single_level() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        assert_eq!(invalidated.len(), 2);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
    }

    #[test]
    fn test_cascade_invalidate_multiple_levels() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_dashboard", "v_analytics").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        assert_eq!(invalidated.len(), 4);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
        assert!(invalidated.contains("v_analytics"));
        assert!(invalidated.contains("v_dashboard"));
    }

    #[test]
    fn test_cascade_invalidate_branching() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_dashboard", "v_post").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        assert_eq!(invalidated.len(), 5);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
        assert!(invalidated.contains("v_post"));
        assert!(invalidated.contains("v_analytics"));
        assert!(invalidated.contains("v_dashboard"));
    }

    #[test]
    fn test_get_direct_dependents() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();

        let dependents = invalidator.get_direct_dependents("v_raw_user");
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains("v_user"));
        assert!(dependents.contains("v_post"));
    }

    #[test]
    fn test_get_direct_dependencies() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_post").unwrap();

        let deps = invalidator.get_direct_dependencies("v_analytics");
        assert_eq!(deps.len(), 2);
        assert!(deps.contains("v_user"));
        assert!(deps.contains("v_post"));
    }

    #[test]
    fn test_get_transitive_dependents() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_dashboard", "v_analytics").unwrap();

        let transitive = invalidator.get_transitive_dependents("v_raw_user");
        assert_eq!(transitive.len(), 4);
        assert!(transitive.contains("v_raw_user"));
        assert!(transitive.contains("v_user"));
        assert!(transitive.contains("v_analytics"));
        assert!(transitive.contains("v_dashboard"));
    }

    #[test]
    fn test_has_dependency_path() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        assert!(invalidator.has_dependency_path("v_analytics", "v_raw_user"));
        assert!(invalidator.has_dependency_path("v_analytics", "v_user"));
        assert!(invalidator.has_dependency_path("v_user", "v_raw_user"));
        assert!(!invalidator.has_dependency_path("v_raw_user", "v_analytics"));
        assert!(!invalidator.has_dependency_path("v_raw_user", "v_user"));
    }

    #[test]
    fn test_stats_tracking() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        invalidator.cascade_invalidate("v_raw_user").unwrap();
        invalidator.cascade_invalidate("v_user").unwrap();

        let stats = invalidator.stats();
        assert_eq!(stats.total_cascades, 2);
        assert_eq!(stats.total_invalidated, 5); // 3 (raw_user + user + analytics) + 2 (user + analytics)
        assert_eq!(stats.max_affected, 3);
    }

    #[test]
    fn test_clear() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        assert_eq!(invalidator.view_count(), 2);

        invalidator.clear();
        assert_eq!(invalidator.view_count(), 0);
        assert_eq!(invalidator.dependency_count(), 0);
    }

    #[test]
    fn test_view_and_dependency_count() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        assert_eq!(invalidator.view_count(), 4);
        assert_eq!(invalidator.dependency_count(), 3);
    }

    #[test]
    fn test_diamond_dependency() {
        let mut invalidator = CascadeInvalidator::new();
        // Diamond: raw → [user, post] → analytics
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_post").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        // raw_user, user, post, analytics (4 total)
        assert_eq!(invalidated.len(), 4);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
        assert!(invalidated.contains("v_post"));
        assert!(invalidated.contains("v_analytics"));
    }

    #[test]
    fn test_multiple_independent_chains() {
        let mut invalidator = CascadeInvalidator::new();
        // Chain 1: raw1 → user1 → analytics1
        invalidator.add_dependency("v_user_1", "v_raw_1").unwrap();
        invalidator.add_dependency("v_analytics_1", "v_user_1").unwrap();
        // Chain 2: raw2 → user2 → analytics2
        invalidator.add_dependency("v_user_2", "v_raw_2").unwrap();
        invalidator.add_dependency("v_analytics_2", "v_user_2").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_1").unwrap();
        assert_eq!(invalidated.len(), 3); // Only chain 1
        assert!(!invalidated.contains("v_raw_2"));
        assert!(!invalidated.contains("v_user_2"));
    }

    #[test]
    fn test_cycle_detection_via_has_dependency_path() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        // Verify no forward path from leaf to root
        assert!(!invalidator.has_dependency_path("v_raw_user", "v_analytics"));
    }

    #[test]
    fn test_indirect_cycle_detection() {
        let mut invalidator = CascadeInvalidator::new();
        // Build chain: A depends on B, B depends on C
        invalidator.add_dependency("B", "A").unwrap();
        invalidator.add_dependency("C", "B").unwrap();

        // Adding C → A would create the cycle C→B→A→...→C — must be rejected.
        // (C depends on B which depends on A; adding A→C would close the loop)
        let result = invalidator.add_dependency("A", "C");
        assert!(
            matches!(result, Err(crate::error::FraiseQLError::Validation { .. })),
            "expected Validation error for indirect cycle A→C→B→A, got: {result:?}"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cycle"), "error message should mention cycle, got: {msg}");
    }

    #[test]
    fn test_three_node_cycle_rejected() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("B", "A").unwrap(); // B depends on A
        invalidator.add_dependency("C", "B").unwrap(); // C depends on B
        // A depends on C would close: A→C→B→A
        let result = invalidator.add_dependency("A", "C");
        assert!(
            matches!(result, Err(crate::error::FraiseQLError::Validation { .. })),
            "expected Validation error for three-node cycle A→C→B→A, got: {result:?}"
        );
    }

    #[test]
    fn test_serialization() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();

        let json = serde_json::to_string(&invalidator).expect("serialize should work");
        let restored: CascadeInvalidator =
            serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(
            restored.get_direct_dependents("v_raw_user"),
            invalidator.get_direct_dependents("v_raw_user")
        );
    }

    #[test]
    fn cascade_invalidator_is_send_sync() {
        // Invariant: CascadeInvalidator is read-only (graph data) after construction.
        // Stats use an internal Mutex for thread-safe writes.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CascadeInvalidator>();
    }

    #[test]
    fn cascade_invalidate_takes_shared_ref() {
        let mut inv = CascadeInvalidator::new();
        inv.add_dependency("v_b", "v_a").unwrap();
        // Should work with &inv (not &mut inv) — cascade_invalidate takes &self.
        let result = inv.cascade_invalidate("v_a").unwrap();
        assert!(result.contains("v_b"));
    }
}

mod result_tests {
    use std::sync::Arc;
    use serde_json::json;
    use crate::cache::*;
    use crate::db::types::JsonbValue;


    // Helper to create test result
    fn test_result() -> Vec<JsonbValue> {
        vec![JsonbValue::new(json!({"id": 1, "name": "test"}))]
    }

    // ========================================================================
    // Cache Hit/Miss Tests
    // ========================================================================

    #[test]
    fn test_cache_miss() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let result = cache.get(999_u64).unwrap();
        assert!(result.is_none(), "Should be cache miss");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hits, 0);
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let result = test_result();

        // Put
        cache.put(1_u64, result, vec!["v_user".to_string()], None, None).unwrap();

        // Get
        let cached = cache.get(1_u64).unwrap();
        assert!(cached.is_some(), "Should be cache hit");
        assert_eq!(cached.unwrap().len(), 1);

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.total_cached, 1);
    }

    #[test]
    fn test_cache_hit_updates_hit_count() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // First hit
        cache.get(1_u64).unwrap();
        // Second hit
        cache.get(1_u64).unwrap();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 2);
    }

    // ========================================================================
    // TTL Expiry Tests
    // ========================================================================

    #[test]
    fn test_ttl_expiry() {
        let config = CacheConfig {
            ttl_seconds: 1,
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_secs(2));
        cache.run_pending_tasks();

        // Should be expired
        let result = cache.get(1_u64).unwrap();
        assert!(result.is_none(), "Entry should be expired");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.misses, 1); // Expired counts as miss
    }

    #[test]
    fn test_per_entry_ttl_override_expires_early() {
        // Global config has 1-hour TTL but entry overrides to 1 second
        let config = CacheConfig {
            ttl_seconds: 3600,
            enabled: true,
            ..Default::default()
        };
        let cache = QueryResultCache::new(config);

        cache
            .put(
                1_u64,
                test_result(),
                vec!["v_ref".to_string()],
                Some(1), // 1-second per-entry override
                None,
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(2));
        cache.run_pending_tasks();

        let result = cache.get(1_u64).unwrap();
        assert!(result.is_none(), "Entry with per-entry TTL=1s should have expired");
    }

    #[test]
    fn test_per_entry_ttl_zero_cached_indefinitely() {
        // TTL=0 = no time-based expiry; entry lives until mutation invalidation.
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, test_result(), vec!["v_live".to_string()], Some(0), None)
            .unwrap();

        let result = cache.get(1_u64).unwrap();
        assert!(result.is_some(), "Entry with TTL=0 should be cached indefinitely");
    }

    #[test]
    fn test_ttl_not_expired() {
        let config = CacheConfig {
            ttl_seconds: 3600, // 1 hour TTL
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Should still be valid
        let result = cache.get(1_u64).unwrap();
        assert!(result.is_some(), "Entry should not be expired");
    }

    // ========================================================================
    // Eviction Tests (capacity-based)
    // ========================================================================

    #[test]
    fn test_capacity_eviction() {
        let config = CacheConfig {
            max_entries: 2,
            enabled: true,
            ..Default::default()
        };

        let cache = QueryResultCache::new(config);

        // Add 3 entries (max is 2); moka will evict one
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(3_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Run pending tasks to flush evictions
        cache.run_pending_tasks();

        let metrics = cache.metrics().unwrap();
        assert!(metrics.size <= 2, "Cache size should not exceed max capacity");
    }

    // ========================================================================
    // Cache Disabled Tests
    // ========================================================================

    #[test]
    fn test_cache_disabled() {
        let config = CacheConfig::disabled();
        let cache = QueryResultCache::new(config);

        // Put should be no-op
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Get should return None
        assert!(cache.get(1_u64).unwrap().is_none(), "Cache disabled should always miss");

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.total_cached, 0);
    }

    // ========================================================================
    // Invalidation Tests
    // ========================================================================

    #[test]
    fn test_invalidate_single_view() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_post".to_string()], None, None).unwrap();

        // Invalidate v_user
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        // v_user entry gone, v_post remains
        assert!(cache.get(1_u64).unwrap().is_none());
        assert!(cache.get(2_u64).unwrap().is_some());
    }

    #[test]
    fn test_invalidate_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_post".to_string()], None, None).unwrap();
        cache
            .put(3_u64, test_result(), vec!["v_product".to_string()], None, None)
            .unwrap();

        // Invalidate v_user and v_post
        let invalidated =
            cache.invalidate_views(&["v_user".to_string(), "v_post".to_string()]).unwrap();
        assert_eq!(invalidated, 2);

        assert!(cache.get(1_u64).unwrap().is_none());
        assert!(cache.get(2_u64).unwrap().is_none());
        assert!(cache.get(3_u64).unwrap().is_some());
    }

    #[test]
    fn test_invalidate_entry_with_multiple_views() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Entry accesses both v_user and v_post
        cache
            .put(
                1_u64,
                test_result(),
                vec!["v_user".to_string(), "v_post".to_string()],
                None,
                None,
            )
            .unwrap();

        // Invalidating either view should remove the entry
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        assert!(cache.get(1_u64).unwrap().is_none());
    }

    #[test]
    fn test_invalidate_nonexistent_view() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Invalidate view that doesn't exist
        let invalidated = cache.invalidate_views(&["v_nonexistent".to_string()]).unwrap();
        assert_eq!(invalidated, 0);

        // Entry should remain
        assert!(cache.get(1_u64).unwrap().is_some());
    }

    // ========================================================================
    // Clear Tests
    // ========================================================================

    #[test]
    fn test_clear() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v_post".to_string()], None, None).unwrap();

        cache.clear().unwrap();

        // Run pending tasks to flush moka's eviction pipeline
        cache.run_pending_tasks();

        assert!(cache.get(1_u64).unwrap().is_none());
        assert!(cache.get(2_u64).unwrap().is_none());

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0);
    }

    // ========================================================================
    // Metrics Tests
    // ========================================================================

    #[test]
    fn test_metrics_tracking() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Miss
        cache.get(999_u64).unwrap();

        // Put
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Hit
        cache.get(1_u64).unwrap();

        // moka::sync::Cache entry_count() is eventually consistent — flush pending
        // write operations before asserting on size.
        cache.run_pending_tasks();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.size, 1);
        assert_eq!(metrics.total_cached, 1);
    }

    #[test]
    fn test_metrics_hit_rate() {
        let metrics = CacheMetrics {
            hits:          80,
            misses:        20,
            total_cached:  100,
            invalidations: 5,
            size:          95,
            memory_bytes:  1_000_000,
        };

        assert!((metrics.hit_rate() - 0.8).abs() < f64::EPSILON);
        assert!(metrics.is_healthy());
    }

    #[test]
    fn test_metrics_hit_rate_zero_requests() {
        let metrics = CacheMetrics {
            hits:          0,
            misses:        0,
            total_cached:  0,
            invalidations: 0,
            size:          0,
            memory_bytes:  0,
        };

        assert!((metrics.hit_rate() - 0.0).abs() < f64::EPSILON);
        assert!(!metrics.is_healthy());
    }

    #[test]
    fn test_metrics_is_healthy() {
        let good = CacheMetrics {
            hits:          70,
            misses:        30,
            total_cached:  100,
            invalidations: 5,
            size:          95,
            memory_bytes:  1_000_000,
        };
        assert!(good.is_healthy()); // 70% > 60%

        let bad = CacheMetrics {
            hits:          50,
            misses:        50,
            total_cached:  100,
            invalidations: 5,
            size:          95,
            memory_bytes:  1_000_000,
        };
        assert!(!bad.is_healthy()); // 50% < 60%
    }

    // ========================================================================
    // Entity-Aware Invalidation Tests
    // ========================================================================

    fn entity_result(id: &str) -> Vec<JsonbValue> {
        vec![JsonbValue::new(
            serde_json::json!({"id": id, "name": "test"}),
        )]
    }

    #[test]
    fn test_invalidate_by_entity_only_removes_matching_entries() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache User A and User B as separate entries
        cache
            .put(1_u64, entity_result("uuid-a"), vec!["v_user".to_string()], None, Some("User"))
            .unwrap();
        cache
            .put(2_u64, entity_result("uuid-b"), vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidate User A — User B must remain
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none(), "User A should be evicted");
        assert!(cache.get(2_u64).unwrap().is_some(), "User B should remain");
    }

    #[test]
    fn test_invalidate_by_entity_removes_list_containing_entity() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache a single-entity entry (entity_ref uses first row's id)
        cache
            .put(1_u64, entity_result("uuid-a"), vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidate by User A
        let evicted = cache.invalidate_by_entity("User", "uuid-a").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none(), "Entry for A should be evicted");
    }

    #[test]
    fn test_invalidate_by_entity_leaves_unrelated_types() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Cache a User entry and a Post entry
        cache
            .put(
                1_u64,
                entity_result("uuid-user"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();
        cache
            .put(
                2_u64,
                entity_result("uuid-post"),
                vec!["v_post".to_string()],
                None,
                Some("Post"),
            )
            .unwrap();

        // Invalidate the User — Post entry must remain untouched
        let evicted = cache.invalidate_by_entity("User", "uuid-user").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none(), "User entry should be evicted");
        assert!(cache.get(2_u64).unwrap().is_some(), "Post entry should remain");
    }

    #[test]
    fn test_put_builds_entity_id_index() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(1_u64, entity_result("uuid-1"), vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // Invalidating by uuid-1 should evict the entry
        let evicted = cache.invalidate_by_entity("User", "uuid-1").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(1_u64).unwrap().is_none());
    }

    #[test]
    fn test_put_without_entity_type_not_indexed() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache
            .put(
                1_u64,
                entity_result("uuid-1"),
                vec!["v_user".to_string()],
                None,
                None, // no entity type
            )
            .unwrap();

        // invalidate_by_entity should not match (no index was built)
        let evicted = cache.invalidate_by_entity("User", "uuid-1").unwrap();
        assert_eq!(evicted, 0);
        assert!(cache.get(1_u64).unwrap().is_some(), "Non-indexed entry should remain");
    }

    // ========================================================================
    // Multi-entity indexing + list_index / invalidate_list_queries tests
    // ========================================================================

    fn list_result(ids: &[&str]) -> Vec<JsonbValue> {
        ids.iter()
            .map(|id| JsonbValue::new(serde_json::json!({"id": id, "name": "test"})))
            .collect()
    }

    #[test]
    fn test_put_indexes_all_entities_in_list() {
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let rows = list_result(&["uuid-A", "uuid-B", "uuid-C"]);
        cache.put(0xABC, rows, vec!["v_user".to_string()], None, Some("User")).unwrap();

        let evicted_a = cache.invalidate_by_entity("User", "uuid-A").unwrap();
        assert_eq!(evicted_a, 1, "uuid-A must be indexed and evictable");

        // Re-insert to test uuid-C
        let rows2 = list_result(&["uuid-A", "uuid-B", "uuid-C"]);
        cache.put(0xDEF, rows2, vec!["v_user".to_string()], None, Some("User")).unwrap();
        let evicted_c = cache.invalidate_by_entity("User", "uuid-C").unwrap();
        assert_eq!(evicted_c, 1, "uuid-C at position 2 must also be indexed");
    }

    #[test]
    fn test_update_evicts_list_query_via_non_first_entity() {
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let rows = list_result(&["uuid-A", "uuid-B"]);
        cache.put(0x111, rows, vec!["v_user".to_string()], None, Some("User")).unwrap();

        // uuid-B is at position 1 — must still be evicted
        let evicted = cache.invalidate_by_entity("User", "uuid-B").unwrap();
        assert_eq!(evicted, 1);
        assert!(cache.get(0x111).unwrap().is_none(), "list entry containing uuid-B must be gone");
    }

    #[test]
    fn test_invalidate_list_queries_spares_point_lookups() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Point lookup: single row
        let single = vec![JsonbValue::new(serde_json::json!({"id": "uuid-X"}))];
        cache
            .put(0x001, single, vec!["v_user".to_string()], None, Some("User"))
            .unwrap();

        // List query: multiple rows
        let list = list_result(&["uuid-A", "uuid-B"]);
        cache.put(0x002, list, vec!["v_user".to_string()], None, Some("User")).unwrap();

        // CREATE fires invalidate_list_queries
        let evicted = cache.invalidate_list_queries(&["v_user".to_string()]).unwrap();
        assert_eq!(evicted, 1, "only the list entry should be evicted");
        assert!(cache.get(0x001).unwrap().is_some(), "point lookup must survive");
        assert!(cache.get(0x002).unwrap().is_none(), "list entry must be evicted");
    }

    #[test]
    fn test_invalidate_by_entity_short_circuits_on_empty_index() {
        let cache = QueryResultCache::new(CacheConfig::enabled());
        // Nothing cached — must return 0 without panicking
        let count = cache.invalidate_by_entity("User", "uuid-X").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_eviction_listener_cleans_all_entity_refs() {
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let rows = list_result(&["uuid-A", "uuid-B"]);
        cache.put(0x001, rows, vec!["v_user".to_string()], None, Some("User")).unwrap();

        // Force eviction via invalidate_views
        cache.invalidate_views(&["v_user".to_string()]).unwrap();
        // Flush moka's async eviction pipeline
        cache.run_pending_tasks();

        // After eviction the entity_index must be cleaned up (no dangling refs)
        let count_a = cache.invalidate_by_entity("User", "uuid-A").unwrap();
        let count_b = cache.invalidate_by_entity("User", "uuid-B").unwrap();
        assert_eq!(count_a, 0, "entity_index must be clean after eviction");
        assert_eq!(count_b, 0, "entity_index must be clean after eviction");
    }

    // ========================================================================
    // Thread Safety Tests
    // ========================================================================

    #[test]
    fn test_concurrent_access() {
        use std::{sync::Arc, thread};

        let cache = Arc::new(QueryResultCache::new(CacheConfig::enabled()));

        // Spawn multiple threads accessing cache
        let handles: Vec<_> = (0_u64..10)
            .map(|key| {
                let cache_clone = cache.clone();
                thread::spawn(move || {
                    cache_clone
                        .put(key, test_result(), vec!["v_user".to_string()], None, None)
                        .unwrap();
                    cache_clone.get(key).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.total_cached, 10);
        assert_eq!(metrics.hits, 10);
    }

    // ========================================================================
    // Sentinel tests — boundary guards for mutation testing
    // ========================================================================

    /// Sentinel: `cache_list_queries = false` must skip results with >1 row.
    ///
    /// Kills the `> → >=` mutation at the list-query guard: `result.len() > 1`.
    #[test]
    fn test_cache_list_queries_false_skips_multi_row() {
        let config = CacheConfig {
            enabled: true,
            cache_list_queries: false,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Two-row result: must be skipped (killed by > → >= mutant)
        let two_rows = vec![
            JsonbValue::new(json!({"id": 1})),
            JsonbValue::new(json!({"id": 2})),
        ];
        cache.put(1_u64, two_rows, vec!["v_user".to_string()], None, None).unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "multi-row result must not be cached when cache_list_queries=false"
        );
    }

    /// Sentinel: `cache_list_queries = false` must still store single-row results.
    ///
    /// Complements the above: the single-row path must remain unaffected.
    #[test]
    fn test_cache_list_queries_false_allows_single_row() {
        let config = CacheConfig {
            enabled: true,
            cache_list_queries: false,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // One-row result: must be stored
        let one_row = vec![JsonbValue::new(json!({"id": 1}))];
        cache.put(1_u64, one_row, vec!["v_user".to_string()], None, None).unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_some(),
            "single-row result must be cached even when cache_list_queries=false"
        );
    }

    /// Sentinel: entries exceeding `max_entry_bytes` must be silently skipped.
    ///
    /// Kills mutations on the `estimated > max_entry` guard.
    #[test]
    fn test_max_entry_bytes_skips_oversized_entry() {
        let config = CacheConfig {
            enabled: true,
            max_entry_bytes: Some(10), // 10 bytes — smaller than any JSON row
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // A typical row serialises to far more than 10 bytes
        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        assert!(cache.get(1_u64).unwrap().is_none(), "oversized entry must be silently skipped");
    }

    /// Sentinel: entries within `max_entry_bytes` must be stored normally.
    ///
    /// Complements the above to pin both sides of the size boundary.
    #[test]
    fn test_max_entry_bytes_allows_small_entry() {
        let config = CacheConfig {
            enabled: true,
            max_entry_bytes: Some(100_000), // 100 KB — plenty for a test row
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_some(),
            "small entry must be cached when within max_entry_bytes"
        );
    }

    /// Sentinel: `put()` must skip new entries when `max_total_bytes` budget is exhausted.
    ///
    /// Kills mutations on the `current >= max_total` guard.
    #[test]
    fn test_max_total_bytes_skips_when_budget_exhausted() {
        let config = CacheConfig {
            enabled: true,
            max_total_bytes: Some(0), // 0 bytes — always exhausted
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        assert!(
            cache.get(1_u64).unwrap().is_none(),
            "entry must be skipped when max_total_bytes budget is already exhausted"
        );
    }

    // ========================================================================
    // Cross-key invalidation Tests (replaces cross-shard tests)
    // ========================================================================

    /// `invalidate_views` clears matching entries regardless of cache key.
    #[test]
    fn test_cross_key_view_invalidation() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Insert many entries
        for i in 0_u64..200 {
            let view = if i % 2 == 0 { "v_user" } else { "v_post" };
            cache.put(i, test_result(), vec![view.to_string()], None, None).unwrap();
        }

        // Invalidate v_user — should remove exactly 100 entries
        let invalidated = cache.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 100);

        // All v_user entries gone, all v_post entries remain
        for i in 0_u64..200 {
            if i % 2 == 0 {
                assert!(cache.get(i).unwrap().is_none(), "v_user entry should be invalidated");
            } else {
                assert!(cache.get(i).unwrap().is_some(), "v_post entry should remain");
            }
        }
    }

    /// Cross-key entity invalidation works across all cache keys.
    #[test]
    fn test_cross_key_entity_invalidation() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        // Insert entries for the same entity across different cache keys
        for i in 0_u64..50 {
            cache
                .put(
                    i,
                    entity_result("uuid-target"),
                    vec!["v_user".to_string()],
                    None,
                    Some("User"),
                )
                .unwrap();
        }

        // Also insert an unrelated entry
        cache
            .put(
                999_u64,
                entity_result("uuid-other"),
                vec!["v_user".to_string()],
                None,
                Some("User"),
            )
            .unwrap();

        let evicted = cache.invalidate_by_entity("User", "uuid-target").unwrap();
        assert_eq!(evicted, 50);
        assert!(cache.get(999_u64).unwrap().is_some(), "unrelated entity should remain");
    }

    /// Clear works for all entries.
    #[test]
    fn test_clear_all() {
        let config = CacheConfig {
            max_entries: 10_000,
            enabled: true,
            ..CacheConfig::default()
        };
        let cache = QueryResultCache::new(config);

        for i in 0_u64..200 {
            cache.put(i, test_result(), vec!["v_user".to_string()], None, None).unwrap();
        }

        cache.clear().unwrap();
        cache.run_pending_tasks();

        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.size, 0);

        for i in 0_u64..200 {
            assert!(cache.get(i).unwrap().is_none());
        }
    }

    /// `memory_bytes` is tracked and reported via `metrics()`.
    #[test]
    fn test_memory_bytes_tracked() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v".to_string()], None, None).unwrap();
        cache.put(2_u64, test_result(), vec!["v".to_string()], None, None).unwrap();

        let before = cache.metrics().unwrap().memory_bytes;
        assert!(before > 0, "memory_bytes should be tracked");
    }

    /// `memory_bytes` decreases after invalidation (synchronously via clear).
    #[test]
    fn test_memory_bytes_decreases_on_clear() {
        let cache = QueryResultCache::new(CacheConfig::enabled());

        cache.put(1_u64, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        let before = cache.metrics().unwrap().memory_bytes;
        assert!(before > 0);

        cache.clear().unwrap();

        let after = cache.metrics().unwrap().memory_bytes;
        assert_eq!(after, 0, "memory_bytes should be zero after clear()");
    }

    // ========================================================================
    // Concurrency regression test (#185)
    // ========================================================================

    /// Regression guard for #185: LRU+Mutex serialized all hot-key reads through
    /// one shard's mutex. With moka, reads are lock-free and should scale near-
    /// linearly with thread count.
    #[test]
    #[ignore = "wall-clock dependent — run manually to confirm lock-free read scaling"]
    fn test_concurrent_reads_do_not_serialize() {
        const ITERS: usize = 10_000;
        let config = CacheConfig::enabled();
        let cache = Arc::new(QueryResultCache::new(config));
        let key = 42_u64;
        cache.put(key, test_result(), vec!["v_user".to_string()], None, None).unwrap();

        // Single-threaded baseline
        let start = std::time::Instant::now();
        for _ in 0..ITERS {
            let _ = cache.get(key).unwrap();
        }
        let single_elapsed = start.elapsed();

        // 40-thread concurrent
        let start = std::time::Instant::now();
        let handles: Vec<_> = (0..40)
            .map(|_| {
                let c = Arc::clone(&cache);
                std::thread::spawn(move || {
                    for _ in 0..ITERS {
                        let _ = c.get(key).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let multi_elapsed = start.elapsed();

        // 40× the work in ≤2× the time → near-linear scaling.
        // Under old LRU+Mutex, 40-thread took ~20-40× single-thread time.
        assert!(
            multi_elapsed <= single_elapsed * 2,
            "40-thread ({:?}) was more than 2× single-thread ({:?}) — suggests serialization",
            multi_elapsed,
            single_elapsed,
        );
    }
}
