//! #1894: a webhook handler that writes a table a TVIEW reads commits, at every isolation
//! level the pipeline offers.
//!
//! `pg_tviews` refuses a write to a table a TVIEW reads, and the flush after it, unless the
//! transaction runs at READ COMMITTED: at REPEATABLE READ or SERIALIZABLE it fails with
//! `0A000`. The pipeline runs the handler inside its own transaction at the level
//! `WebhookPipeline::with_isolation` sets, so every level it offers must be one such a write
//! survives.
//!
//! Needs `pg_tviews`: the `postgres-tviews` service (`docker/pg-tviews`), through
//! `TVIEWS_DATABASE_URL`, bound in the Dagger `integration: server` suite. Without it the
//! suite skips.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:**
//! `TVIEWS_DATABASE_URL` · **Parallelism:** drops and recreates its own `p1894` schema and
//! truncates the shared `webhooks` ledger → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_webhooks::{
    Authenticated, Delivery, Disposition, EventHandler, Handled, InboundRequest,
    PostgresIdempotencyStore, Result, SignatureError, SignatureVerifier, StaticSecretProvider,
    Verified, VerifiedEvent, WebhookIsolation, WebhookPipeline,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};

const SCHEMA: &str = "p1894";

struct AcceptingVerifier;
impl SignatureVerifier for AcceptingVerifier {
    fn name(&self) -> &'static str {
        "accepting"
    }

    fn verify(
        &self,
        _request: &InboundRequest<'_>,
        _secret: &str,
    ) -> std::result::Result<Verified, SignatureError> {
        Ok(Verified::Body)
    }
}

/// Renames the one item to the delivery's `label`: a write to the table `tv_item` reads.
struct RenamingHandler;
impl EventHandler for RenamingHandler {
    async fn handle(
        &self,
        function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Handled> {
        sqlx::query(&format!("UPDATE {SCHEMA}.tb_item SET label = $1"))
            .bind(params["label"].as_str().unwrap())
            .execute(&mut **tx)
            .await?;
        Ok(Handled::Recorded(json!({ "handled": function_name })))
    }
}

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

/// A base table, the TVIEW over it and an empty delivery ledger. `None` (skip) without
/// `TVIEWS_DATABASE_URL`.
async fn setup() -> Option<PgPool> {
    let url = tviews_database_url()?;
    let admin = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::raw_sql(&format!(
        "CREATE EXTENSION IF NOT EXISTS pg_tviews CASCADE;
         DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
         CREATE SCHEMA {SCHEMA};
         CREATE TABLE {SCHEMA}.tb_item (pk_item bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
           id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(), label text NOT NULL);
         INSERT INTO {SCHEMA}.tb_item (label) VALUES ('before');
         SELECT tviews.pg_tviews_create('{SCHEMA}.tv_item', $T$
           SELECT i.pk_item, i.id, jsonb_build_object('id', i.id, 'label', i.label) AS data
           FROM {SCHEMA}.tb_item i $T$);"
    ))
    .execute(&admin)
    .await
    .unwrap();
    PostgresIdempotencyStore::new(admin.clone()).init().await.unwrap();
    sqlx::query("TRUNCATE webhooks.tb_inbound_delivery")
        .execute(&admin)
        .await
        .unwrap();
    Some(admin)
}

static NO_HEADERS: std::sync::LazyLock<std::collections::BTreeMap<String, String>> =
    std::sync::LazyLock::new(std::collections::BTreeMap::new);

fn delivery() -> Delivery<'static> {
    Delivery {
        route:         "stripe",
        function_name: "rename_item",
        request:       InboundRequest::new(&NO_HEADERS, b"{}", None),
    }
}

fn event(id: String, label: String) -> impl FnOnce(Authenticated<'_>) -> Result<VerifiedEvent> {
    move |_authenticated| {
        Ok(VerifiedEvent {
            id,
            event_type: "item.renamed".to_string(),
            params: json!({ "label": label }),
        })
    }
}

async fn tview_label(pool: &PgPool) -> String {
    sqlx::query_scalar::<_, String>(&format!("SELECT data->>'label' FROM {SCHEMA}.tv_item"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_handler_writing_a_tviews_base_table_commits_at_every_offered_isolation_level() {
    let Some(admin) = setup().await else {
        eprintln!("skipping #1894 webhook isolation test: TVIEWS_DATABASE_URL not set");
        return;
    };
    let levels = [WebhookIsolation::default(), WebhookIsolation::ReadCommitted];
    let mut refused = Vec::new();
    for (n, level) in levels.into_iter().enumerate() {
        let label = format!("renamed at {}", level.as_sql());
        let outcome = WebhookPipeline::new(
            admin.clone(),
            StaticSecretProvider::new().with_secret("stripe", "whsec_test"),
            PostgresIdempotencyStore::new(admin.clone()),
            RenamingHandler,
        )
        .with_isolation(level)
        .process(
            &AcceptingVerifier,
            Some("stripe"),
            &delivery(),
            event(format!("evt_1894_{n}"), label.clone()),
        )
        .await;
        match outcome {
            Ok(Disposition::Processed(_)) => assert_eq!(
                tview_label(&admin).await,
                label,
                "the TVIEW row carries the write made at {}",
                level.as_sql()
            ),
            other => refused.push(format!("{}: {other:?}", level.as_sql())),
        }
    }
    assert!(refused.is_empty(), "levels whose write a TVIEW refused: {refused:#?}");
}
