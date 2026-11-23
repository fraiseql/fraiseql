//! MCP declares each tool argument's variable at the type the argument publishes (§ 5.8.5),
//! so a required argument is called, and a `null` for one is refused under the argument's
//! own name before the document is built, not under the variable MCP carries it in.

use fraiseql_core::{db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _};
use serde_json::{Value, json};

use super::{SCHEMA, call, provisioned, schema};

async fn users_with_email(url: &str, email: &str) -> usize {
    let adapter = PostgresAdapter::new(url).await.unwrap();
    let rows = adapter
        .execute_raw_query(&format!(
            "SELECT id FROM {SCHEMA}.tb_user WHERE email IS NOT DISTINCT FROM {email}"
        ))
        .await
        .unwrap();
    rows.len()
}

#[tokio::test]
async fn a_required_argument_is_called_and_a_null_for_it_is_refused_by_its_name() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1617: DATABASE_URL not set");
        return;
    };

    let (failed, created) =
        call(schema(), &url, "createUser", &json!({ "email": "lin@example.com" })).await;
    let (refused, message) = call(schema(), &url, "createUser", &json!({ "email": null })).await;

    assert!(!failed, "{created}");
    assert_eq!(created["data"]["createUser"]["email"], json!("lin@example.com"), "{created}");
    assert!(refused, "{message}");
    assert_eq!(
        message,
        Value::String(
            "Validation error: Argument `email` on Mutation.createUser has type `String!`, and \
             may not be null"
                .to_string()
        )
    );
    assert_eq!(users_with_email(&url, "NULL").await, 0, "a null email was written");
}

/// [`schema`] with `usersByEmails(emails: [String!])`, a list argument whose items are
/// non-null.
fn schema_with_a_list_argument() -> fraiseql_core::schema::CompiledSchema {
    use fraiseql_core::schema::{ArgumentDefinition, FieldType, QueryDefinition};

    let mut schema = schema();
    let mut query = QueryDefinition::new("usersByEmails", "User")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_user"));
    query.arguments = vec![ArgumentDefinition::optional(
        "emails",
        FieldType::list(FieldType::String),
    )];
    schema.queries.push(query);
    schema.build_indexes();
    schema
}

/// The variable for a `[String!]` argument is declared `[String!]`, so the call is executed;
/// a null item is refused naming the argument's item.
#[tokio::test]
async fn a_list_argument_with_non_null_items_is_called_and_a_null_item_refused_by_its_name() {
    let Some(url) = provisioned().await else {
        eprintln!("skipping #1617: DATABASE_URL not set");
        return;
    };

    let (failed, served) = call(
        schema_with_a_list_argument(),
        &url,
        "usersByEmails",
        &json!({ "emails": ["ada@example.com"] }),
    )
    .await;
    let (refused, message) = call(
        schema_with_a_list_argument(),
        &url,
        "usersByEmails",
        &json!({ "emails": ["ada@example.com", null] }),
    )
    .await;

    assert!(!failed, "{served}");
    assert!(served["data"]["usersByEmails"].is_array(), "{served}");
    assert!(served.get("errors").is_none(), "{served}");
    assert!(refused, "{message}");
    let message = message.as_str().unwrap_or_default();
    assert!(
        message.contains("emails[1]") && !message.contains('$'),
        "the null item is named by the argument, not a variable: {message}"
    );
}
