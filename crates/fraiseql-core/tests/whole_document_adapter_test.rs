//! A read is shaped by its selection, whatever the adapter returns.
//!
//! The adapter here ignores the projection hint and answers every read with the whole
//! stored document: unselected keys at the root and inside nested objects, and fields
//! a scope the caller lacks would mask. The response must still hold exactly what the
//! query selected, each level in selection order, and a masked field must be `null`.
//! Nothing below the root is taken on the adapter's word.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::{CompiledSchema, FieldDenyPolicy, FieldType, SecurityConfig, SqlProjectionHint},
    security::SecurityContext,
};
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

/// Every key the stored document holds, most of which no query below selects.
fn stored_document() -> JsonbValue {
    JsonbValue::new(json!({
        "id": "aaaa0000-0000-0000-0000-000000000001",
        "name": "Alice",
        "hidden": "ROOT-UNSELECTED",
        "profile": {
            "tier": "gold",
            "hidden": "DEPTH-1-UNSELECTED",
            "note": "DEPTH-1-MASKED",
            "address": {
                "city": "Paris",
                "hidden": "DEPTH-2-UNSELECTED",
                "code": "DEPTH-2-MASKED",
            },
        },
    }))
}

/// Answers every read with [`stored_document`], whatever projection it was handed.
struct WholeDocumentAdapter;

// Reason: DatabaseAdapter is defined with #[async_trait]; implementations must match it.
#[async_trait]
impl DatabaseAdapter for WholeDocumentAdapter {
    fn applies_session_variables(&self) -> bool {
        true
    }

    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, offset, order_by).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![stored_document()])
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(&self, _sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }
}

/// A field a scope nobody holds masks.
fn masked(name: &str, scope: &str) -> fraiseql_core::schema::FieldDefinition {
    TestFieldBuilder::new(name, FieldType::String)
        .requires_scope(scope)
        .on_deny(FieldDenyPolicy::Mask)
        .build()
}

fn executor() -> Executor {
    let user = TestTypeBuilder::new("User", "v_user")
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("hidden", FieldType::String)
        .with_simple_field("profile", FieldType::Object("Profile".to_string()))
        .build();
    let profile = TestTypeBuilder::new("Profile", "v_profile")
        .with_simple_field("tier", FieldType::String)
        .with_simple_field("hidden", FieldType::String)
        .with_field(masked("note", "read:Profile.note"))
        .with_simple_field("address", FieldType::Object("Address".to_string()))
        .build();
    let address = TestTypeBuilder::new("Address", "v_address")
        .with_simple_field("city", FieldType::String)
        .with_simple_field("hidden", FieldType::String)
        .with_field(masked("code", "read:Address.code"))
        .build();
    let users = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_sql_source("v_user")
        .build();
    let schema: CompiledSchema = TestSchemaBuilder::new()
        .with_type(user)
        .with_type(profile)
        .with_type(address)
        .with_query(users)
        .with_security(SecurityConfig::default())
        .build();
    Executor::read_only(schema, Arc::new(WholeDocumentAdapter))
}

/// An authenticated caller holding no scope.
fn scopeless() -> SecurityContext {
    SecurityContext {
        user_id:          fraiseql_core::types::UserId::new("whole-document"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-whole-document".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

async fn first_user(query: &str) -> Value {
    let response = executor()
        .execute_operation_with_security(query, None, &scopeless(), None)
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    assert!(response.get("errors").is_none(), "{query}: {response}");
    response["data"]["users"][0].clone()
}

#[tokio::test]
async fn a_key_the_adapter_returns_inside_a_nested_object_unselected_never_reaches_the_response() {
    let user = first_user("{ users { name profile { tier address { city } } } }").await;
    assert_eq!(
        user,
        json!({"name": "Alice", "profile": {"tier": "gold", "address": {"city": "Paris"}}}),
        "only the selected keys, at every depth"
    );
}

#[tokio::test]
async fn a_masked_field_inside_a_nested_object_is_null_at_every_depth() {
    let user = first_user("{ users { profile { note tier address { code city } } } }").await;
    assert_eq!(
        user,
        json!({"profile": {"note": null, "tier": "gold", "address": {"code": null, "city": "Paris"}}}),
        "masked positions are null, kept in selection order"
    );
}

#[tokio::test]
async fn a_nested_object_follows_the_selection_order_not_the_documents() {
    let user = first_user("{ users { profile { address { city } tier } name } }").await;
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&user), ["profile", "name"]);
    assert_eq!(keys(&user["profile"]), ["address", "tier"]);
}

/// A direct read (REST) over the same adapter answers what the GraphQL read answers: the
/// selected keys only, masked positions `null`, every level in selection order.
#[tokio::test]
async fn a_direct_read_is_shaped_by_its_selection_as_the_graphql_read_is() {
    let exec = executor();
    let matcher = fraiseql_core::runtime::QueryMatcher::new(exec.schema().clone());
    for query in [
        "{ users { name profile { tier address { city } } } }",
        "{ users { profile { note tier address { code city } } } }",
        "{ users { profile { address { city } tier } name } }",
    ] {
        let query_match = matcher.match_query(query, None).unwrap();
        let direct = exec
            .execute_query_direct(&query_match, None, Some(&scopeless()), None)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let user = &direct["data"]["users"][0];
        assert_eq!(user, &first_user(query).await, "{query}");
        assert_eq!(
            serde_json::to_string(user).unwrap(),
            serde_json::to_string(&first_user(query).await).unwrap(),
            "{query}: the same key order"
        );
    }
}
