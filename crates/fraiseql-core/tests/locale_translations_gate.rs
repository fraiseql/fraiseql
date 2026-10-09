#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1523: a localized field's translations sibling (`<field>Translations`) lists every label
//! of the field, so it is gated by the field's own gate, under either name:
//!
//! - `requires_scope` + `on_deny = mask` → `[]` for a caller without the scope;
//! - `requires_scope` + `on_deny = reject` → the read is refused;
//! - `authorize` → the field authorizer is asked about the **base** field, and its `Mask` is `[]`,
//!   its `Reject` a refusal.
//!
//! A caller the gate admits reads every label, as for an ungated field.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_tr_gate_product` table.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, RuntimeConfig, with_request_locale},
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, LocaleConfig, LocaleSource,
        RoleDefinition, SecurityConfig,
    },
    security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest, SecurityContext},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const TABLE: &str = "tv_tr_gate_product";

/// Every allowed label of the one row, in `allowed`'s order.
fn labels() -> Value {
    json!([{"locale": "en-US", "value": "Apple"}, {"locale": "fr-FR", "value": "Pomme"}])
}

fn localized(name: &str) -> FieldDefinition {
    let mut field = FieldDefinition::nullable(name, FieldType::String);
    field.localized = true;
    field
}

/// `motto` masks without `read:motto`, `slogan` refuses without `read:slogan`, `tagline`
/// is decided by the field authorizer.
fn schema() -> CompiledSchema {
    let mut motto = localized("motto");
    motto.requires_scope = Some("read:motto".to_string());
    motto.on_deny = FieldDenyPolicy::Mask;
    let mut slogan = localized("slogan");
    slogan.requires_scope = Some("read:slogan".to_string());
    slogan.on_deny = FieldDenyPolicy::Reject;
    let mut tagline = localized("tagline");
    tagline.authorize = true;
    tagline.on_deny = FieldDenyPolicy::Mask;
    let product = TestTypeBuilder::new("Product", TABLE)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_field(localized("name"))
        .with_field(motto)
        .with_field(slogan)
        .with_field(tagline)
        .build();
    let products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(TABLE)
        .build();
    let connection = TestQueryBuilder::new("productsConnection", "Product")
        .returns_list(true)
        .with_sql_source(TABLE)
        .relay_cursor_column("pk")
        .build();
    let mut schema = TestSchemaBuilder::new()
        .with_type(product)
        .with_query(products)
        .with_query(connection)
        .build();
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    let mut security = SecurityConfig::new();
    security.add_role(RoleDefinition::new(
        "reader",
        vec!["read:motto".to_string(), "read:slogan".to_string()],
    ));
    schema.security = Some(security);
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".into(), "fr-FR".into()],
            BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema.build_indexes();
    schema
}

/// Records the fields it is asked about; denies `tagline` with `on_deny` when set.
struct Policy {
    asked: Mutex<Vec<String>>,
    deny:  Option<FieldDenyPolicy>,
}

impl FieldAuthorizer for Policy {
    fn authorize_field(
        &self,
        req: &FieldAuthzRequest<'_>,
    ) -> fraiseql_core::error::Result<FieldAuthzDecision> {
        self.asked.lock().unwrap().push(req.field_name.to_string());
        Ok(match self.deny {
            Some(on_deny) if req.field_name == "tagline" => FieldAuthzDecision::Deny {
                code: "no".to_string(),
                on_deny,
            },
            _ => FieldAuthzDecision::Allow,
        })
    }
}

async fn executor(policy: Option<Arc<Policy>>) -> Option<Executor> {
    executor_over(schema(), policy).await
}

async fn executor_over(schema: CompiledSchema, policy: Option<Arc<Policy>>) -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let map = r#"{"en-US": "Apple", "fr-FR": "Pomme"}"#;
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (pk bigint, id text, data jsonb)"),
        format!(
            "INSERT INTO {TABLE} VALUES (1, '1', jsonb_build_object('id', '1', 'pk', 1, 'name', \
             '{map}'::jsonb, 'motto', '{map}'::jsonb, 'slogan', '{map}'::jsonb, 'tagline', \
             '{map}'::jsonb))"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let config = RuntimeConfig {
        field_authorizer: policy.map(|p| p as Arc<dyn FieldAuthorizer>),
        ..RuntimeConfig::default()
    };
    Some(Executor::with_config_and_relay(schema, Arc::new(adapter), config))
}

fn caller(roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          fraiseql_core::prelude::UserId::new("caller"),
        tenant_id:        None,
        roles:            roles.iter().map(|r| (*r).to_string()).collect(),
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-tr-gate".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

async fn run(
    executor: &Executor,
    query: &str,
    who: &SecurityContext,
) -> fraiseql_core::error::Result<Value> {
    with_request_locale("fr-FR", executor.execute_with_security(query, None, who)).await
}

#[tokio::test]
async fn a_masked_fields_translations_are_empty_without_its_scope() {
    let Some(executor) = executor(None).await else {
        eprintln!("skipping #1523: DATABASE_URL not set");
        return;
    };
    let query = "{ products { id mottoTranslations { locale value } } }";
    let denied = run(&executor, query, &caller(&[])).await.unwrap();
    assert_eq!(denied["data"]["products"][0]["mottoTranslations"], json!([]), "{denied}");
    let admitted = run(&executor, query, &caller(&["reader"])).await.unwrap();
    assert_eq!(admitted["data"]["products"][0]["mottoTranslations"], labels(), "{admitted}");
    let anonymous = with_request_locale("fr-FR", executor.execute(query, None)).await.unwrap();
    assert_eq!(anonymous["data"]["products"][0]["mottoTranslations"], json!([]), "{anonymous}");
}

#[tokio::test]
async fn a_refusing_fields_translations_are_refused_without_its_scope() {
    let Some(executor) = executor(None).await else {
        return;
    };
    let query = "{ products { id sloganTranslations { locale value } } }";
    let err = run(&executor, query, &caller(&[])).await.unwrap_err();
    assert!(
        matches!(err, fraiseql_core::error::FraiseQLError::Authorization { .. }),
        "{err}"
    );
    let admitted = run(&executor, query, &caller(&["reader"])).await.unwrap();
    assert_eq!(admitted["data"]["products"][0]["sloganTranslations"], labels(), "{admitted}");
}

#[tokio::test]
async fn an_authorized_fields_translations_follow_the_authorizer_on_the_base_field() {
    let query = "{ products { id taglineTranslations { locale value } } }";

    let masking = Arc::new(Policy {
        asked: Mutex::new(Vec::new()),
        deny:  Some(FieldDenyPolicy::Mask),
    });
    let Some(masking_executor) = executor(Some(Arc::clone(&masking))).await else {
        return;
    };
    let masked = run(&masking_executor, query, &caller(&[])).await.unwrap();
    assert_eq!(masked["data"]["products"][0]["taglineTranslations"], json!([]), "{masked}");
    assert_eq!(*masking.asked.lock().unwrap(), ["tagline"], "asked about the base field");

    let refusing = Arc::new(Policy {
        asked: Mutex::new(Vec::new()),
        deny:  Some(FieldDenyPolicy::Reject),
    });
    let refusing_executor = executor(Some(refusing)).await.unwrap();
    assert!(run(&refusing_executor, query, &caller(&[])).await.is_err(), "a Reject refuses");

    let allowing = Arc::new(Policy {
        asked: Mutex::new(Vec::new()),
        deny:  None,
    });
    let allowing_executor = executor(Some(allowing)).await.unwrap();
    let served = run(&allowing_executor, query, &caller(&[])).await.unwrap();
    assert_eq!(served["data"]["products"][0]["taglineTranslations"], labels(), "{served}");
}

/// The relay path classifies the node's selection the same way. (It refuses any type that
/// declares an `authorize` field, translations or not, so `tagline` is ungated here.)
#[tokio::test]
async fn a_relay_nodes_masked_translations_are_empty() {
    let mut relay_schema = schema();
    for field in &mut relay_schema.types[0].fields {
        field.authorize = false;
    }
    let Some(executor) = executor_over(relay_schema, None).await else {
        return;
    };
    let query =
        "{ productsConnection(first: 5) { edges { node { id mottoTranslations { value } } } } }";
    let denied = run(&executor, query, &caller(&[])).await.unwrap();
    assert_eq!(
        denied["data"]["productsConnection"]["edges"][0]["node"]["mottoTranslations"],
        json!([]),
        "{denied}"
    );
}

/// A read selecting a policy-gated field is projected in Rust from the stored document. It
/// serves a localized field in the locale its `locale:` argument names, as the SQL
/// projection does, not in the request's.
#[tokio::test]
async fn a_gated_read_honours_a_locale_argument() {
    let allowing = Arc::new(Policy {
        asked: Mutex::new(Vec::new()),
        deny:  None,
    });
    let Some(executor) = executor(Some(allowing)).await else {
        return;
    };
    let served = run(
        &executor,
        r#"{ products { id tagline en: tagline(locale: "en-US") nameTranslations { value } } }"#,
        &caller(&[]),
    )
    .await
    .unwrap();
    let row = &served["data"]["products"][0];
    assert_eq!(row["tagline"], json!("Pomme"), "the request locale: {served}");
    assert_eq!(row["en"], json!("Apple"), "the argument's locale: {served}");
    assert_eq!(
        row["nameTranslations"],
        json!([{"value": "Apple"}, {"value": "Pomme"}]),
        "an ungated sibling beside a gated field: {served}"
    );
}

/// A federation `_entities` lookup masks its entities' fields through the same classifier,
/// so a masked field's sibling is `[]` there too.
#[cfg(feature = "federation")]
#[tokio::test]
async fn an_entitys_masked_translations_are_empty() {
    let mut federated = schema();
    for field in &mut federated.types[0].fields {
        field.authorize = false;
    }
    federated.federation = Some(
        serde_json::from_value(json!({
            "enabled": true, "version": "v2", "service_name": "products",
            "entities": [{"name": "Product", "key_fields": ["id"]}]
        }))
        .unwrap(),
    );
    federated.build_indexes();
    let Some(executor) = executor_over(federated, None).await else {
        return;
    };
    let query = "query($representations: [_Any!]!) { _entities(representations: \
                 $representations) { ... on Product { id mottoTranslations { value } } } }";
    let variables = json!({ "representations": [{ "__typename": "Product", "id": "1" }] });
    let denied = with_request_locale(
        "fr-FR",
        executor.execute_with_security(query, Some(&variables), &caller(&[])),
    )
    .await
    .unwrap();
    assert_eq!(denied["data"]["_entities"][0]["mottoTranslations"], json!([]), "{denied}");
}
