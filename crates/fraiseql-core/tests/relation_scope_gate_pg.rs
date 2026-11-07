//! The relation-scope check: a fact table declared `row_scope = "relation"` must be
//! filtered by its relation, on a session variable the server sets from the caller's
//! token or enriched identity.
//!
//! The server adds no tenant predicate to such a table's aggregate and window roots: the
//! declaration says the relation filters its own rows. Each test below builds one shape
//! of relation and asserts what
//! [`refuse_unfiltered_relation_scopes`](fraiseql_core::schema::refuse_unfiltered_relation_scopes)
//! answers for it, connected as the role that reads.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** safe — every fixture relation is unique to one test, and role setup
//! takes the role-setup lock the RLS suites share.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_core::{
    compiler::fact_table::FactTableMetadata,
    db::postgres::PostgresAdapter,
    error::FraiseQLError,
    schema::{
        CompiledSchema, SessionVariableMapping, SessionVariableSource, SessionVariablesConfig,
        refuse_unfiltered_relation_scopes,
    },
};
use fraiseql_test_support::try_database_url;
use serde_json::json;

/// The setting the server sets from the caller's token.
const TOKEN_SETTING: &str = "app.rsg_tenant";
/// The setting the server sets from an enriched identity field.
const ENRICHED_SETTING: &str = "app.rsg_enriched";
/// The setting the server sets from a request header: client input.
const HEADER_SETTING: &str = "app.rsg_header";
/// A setting the schema does not declare.
const UNDECLARED_SETTING: &str = "app.rsg_other";

/// The unprivileged role most tests read as: neither superuser nor `BYPASSRLS`, owner
/// of nothing.
const READER: &str = "p_rsg_reader";

/// The transaction-scoped lock every RLS suite takes for role setup: roles are cluster-wide, and
/// two concurrent `ALTER ROLE` or `GRANT` statements on one catalog row fail.
const ROLE_LOCK: i64 = 762_004;

/// A schema declaring `table` relation-scoped, with one session variable of each source.
fn schema_scoping(table: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let metadata: FactTableMetadata = serde_json::from_value(json!({
        "table_name": table,
        "row_scope": "relation",
        "measures": [],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [],
    }))
    .unwrap();
    schema.add_fact_table(table.to_string(), metadata);
    schema.session_variables = SessionVariablesConfig {
        variables: vec![
            SessionVariableMapping {
                name:   TOKEN_SETTING.to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "tid".to_string(),
                },
            },
            SessionVariableMapping {
                name:   ENRICHED_SETTING.to_string(),
                source: SessionVariableSource::Enrichment {
                    field: "org".to_string(),
                },
            },
            SessionVariableMapping {
                name:   HEADER_SETTING.to_string(),
                source: SessionVariableSource::Header {
                    header: "x-tenant".to_string(),
                },
            },
        ],
        ..SessionVariablesConfig::default()
    };
    schema
}

async fn setup_connection(db_url: &str) -> tokio_postgres::Client {
    let (client, connection) =
        tokio_postgres::connect(db_url, tokio_postgres::NoTls).await.expect("connect");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("setup connection error: {e}");
        }
    });
    client
}

/// (Re)create the `LOGIN` role `name`, whose password is its name, never superuser, with
/// exactly `attributes`.
async fn login_role(setup: &tokio_postgres::Client, name: &str, attributes: &str) {
    setup
        .batch_execute(&format!(
            "BEGIN;
             SELECT pg_advisory_xact_lock({ROLE_LOCK});
             DO $$ BEGIN
                 CREATE ROLE {name} LOGIN PASSWORD '{name}';
             EXCEPTION WHEN duplicate_object THEN NULL;
             END $$;
             ALTER ROLE {name} LOGIN PASSWORD '{name}' NOSUPERUSER {attributes};
             GRANT USAGE ON SCHEMA public TO {name};
             COMMIT;"
        ))
        .await
        .expect("login role");
}

/// `db_url` with `role`'s credentials (its password is its name).
fn url_as(db_url: &str, role: &str) -> String {
    let cfg: tokio_postgres::Config = db_url.parse().expect("parse DATABASE_URL");
    let host = match cfg.get_hosts().first().expect("a host") {
        tokio_postgres::config::Host::Tcp(h) => h.clone(),
        #[cfg(unix)]
        tokio_postgres::config::Host::Unix(p) => p.display().to_string(),
    };
    let port = cfg.get_ports().first().copied().unwrap_or(5432);
    let dbname = cfg.get_dbname().expect("a database name");
    format!("postgres://{role}:{role}@{host}:{port}/{dbname}")
}

/// Run `fixture` as the harness role, then the check for `table` as `role` (`None`: the
/// harness role itself).
async fn check(
    fixture: &str,
    table: &str,
    role: Option<(&str, &str)>,
) -> Option<fraiseql_core::Result<()>> {
    let Some(db_url) = try_database_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return None;
    };
    let setup = setup_connection(&db_url).await;
    if let Some((name, attributes)) = role {
        login_role(&setup, name, attributes).await;
    }
    setup.batch_execute(fixture).await.expect("fixture");
    let url = role.map_or_else(|| db_url.clone(), |(name, _)| url_as(&db_url, name));
    let adapter = PostgresAdapter::new(&url).await.expect("PostgresAdapter");
    Some(refuse_unfiltered_relation_scopes(&adapter, &schema_scoping(table)).await)
}

/// The configuration message of a refusal, or a panic naming what came back instead.
fn refusal(result: fraiseql_core::Result<()>) -> String {
    match result {
        Err(FraiseQLError::Configuration { message }) => message,
        other => panic!("the check must refuse this shape: {other:?}"),
    }
}

fn assert_names(message: &str, parts: &[&str]) {
    for part in parts {
        assert!(message.contains(part), "the refusal must name {part:?}: {message}");
    }
}

/// A table holding two tenants' rows, owned by the harness role.
fn table(name: &str) -> String {
    format!(
        "DROP TABLE IF EXISTS {name} CASCADE;
         CREATE TABLE {name} (amount numeric, data jsonb, tenant_id text);
         INSERT INTO {name} VALUES (1, '{{}}', 'a'), (2, '{{}}', 'b');"
    )
}

/// [`table`] with row level security on and one permissive policy filtering reads with
/// `using`.
fn policy_table(name: &str, using: &str) -> String {
    format!(
        "{}
         ALTER TABLE {name} ENABLE ROW LEVEL SECURITY;
         CREATE POLICY p_read ON {name} USING ({using});",
        table(name)
    )
}

fn on(setting: &str) -> String {
    format!("tenant_id = NULLIF(current_setting('{setting}', true), '')")
}

const fn reader() -> Option<(&'static str, &'static str)> {
    Some((READER, "NOBYPASSRLS"))
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_plain_table_is_refused() {
    let Some(result) = check(&table("tf_rsg_plain"), "tf_rsg_plain", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_plain", "row level security is not enabled"]);
}

#[tokio::test]
async fn row_level_security_without_a_policy_is_refused() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_no_policy ENABLE ROW LEVEL SECURITY;",
        table("tf_rsg_no_policy")
    );
    let Some(result) = check(&fixture, "tf_rsg_no_policy", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_no_policy", "no row level security policy"]);
}

#[tokio::test]
async fn a_policy_on_a_token_sourced_setting_passes() {
    let fixture = policy_table("tf_rsg_token", &on(TOKEN_SETTING));
    let Some(result) = check(&fixture, "tf_rsg_token", reader()).await else {
        return;
    };
    result.expect("a policy on a token-sourced setting filters the relation");
}

#[tokio::test]
async fn a_policy_on_an_enrichment_sourced_setting_passes() {
    let fixture = policy_table("tf_rsg_enriched", &on(ENRICHED_SETTING));
    let Some(result) = check(&fixture, "tf_rsg_enriched", reader()).await else {
        return;
    };
    result.expect("a policy on an enrichment-sourced setting filters the relation");
}

#[tokio::test]
async fn a_policy_naming_the_setting_in_another_case_passes() {
    let fixture = policy_table("tf_rsg_case", &on(&TOKEN_SETTING.to_uppercase()));
    let Some(result) = check(&fixture, "tf_rsg_case", reader()).await else {
        return;
    };
    result.expect("setting names are case-insensitive");
}

#[tokio::test]
async fn a_policy_on_a_header_sourced_setting_is_refused() {
    let fixture = policy_table("tf_rsg_header", &on(HEADER_SETTING));
    let Some(result) = check(&fixture, "tf_rsg_header", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_header", "does not filter on", TOKEN_SETTING]);
}

#[tokio::test]
async fn a_policy_on_an_undeclared_setting_is_refused() {
    let fixture = policy_table("tf_rsg_undeclared", &on(UNDECLARED_SETTING));
    let Some(result) = check(&fixture, "tf_rsg_undeclared", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_undeclared", "does not filter on"]);
}

#[tokio::test]
async fn a_policy_on_a_setting_the_declared_name_only_prefixes_is_refused() {
    let fixture = policy_table("tf_rsg_prefixed", &on(&format!("{TOKEN_SETTING}_other")));
    let Some(result) = check(&fixture, "tf_rsg_prefixed", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_prefixed", "does not filter on"]);
}

#[tokio::test]
async fn a_permissive_policy_that_does_not_filter_beside_one_that_does_is_refused() {
    let fixture = format!(
        "{} CREATE POLICY p_open ON tf_rsg_open USING (true);",
        policy_table("tf_rsg_open", &on(TOKEN_SETTING))
    );
    let Some(result) = check(&fixture, "tf_rsg_open", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_open", "does not filter on"]);
}

#[tokio::test]
async fn a_restrictive_policy_on_the_setting_passes_beside_a_permissive_one() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_restrictive ENABLE ROW LEVEL SECURITY;
         CREATE POLICY p_open ON tf_rsg_restrictive USING (true);
         CREATE POLICY p_tenant ON tf_rsg_restrictive AS RESTRICTIVE USING ({});",
        table("tf_rsg_restrictive"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_restrictive", reader()).await else {
        return;
    };
    result.expect("a restrictive policy on the setting filters every read");
}

#[tokio::test]
async fn a_policy_to_public_passes() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_public ENABLE ROW LEVEL SECURITY;
         CREATE POLICY p_public ON tf_rsg_public TO PUBLIC USING ({});",
        table("tf_rsg_public"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_public", reader()).await else {
        return;
    };
    result.expect("a policy TO PUBLIC applies to every reader");
}

/// A restrictive policy narrows what permissive policies grant; with none, PostgreSQL
/// returns no rows at all. The relation then serves nothing, which is refused: it is a
/// misconfiguration, and the refusal says which policy is missing.
#[tokio::test]
async fn a_restrictive_policy_without_a_permissive_one_is_refused() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_restrictive_only ENABLE ROW LEVEL SECURITY;
         CREATE POLICY p_tenant ON tf_rsg_restrictive_only AS RESTRICTIVE USING ({});",
        table("tf_rsg_restrictive_only"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_restrictive_only", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_restrictive_only", "no row level security policy"]);
}

#[tokio::test]
async fn a_policy_that_does_not_apply_to_reads_is_refused() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_insert_only ENABLE ROW LEVEL SECURITY;
         CREATE POLICY p_insert ON tf_rsg_insert_only FOR INSERT WITH CHECK ({});",
        table("tf_rsg_insert_only"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_insert_only", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_insert_only", "no row level security policy"]);
}

#[tokio::test]
async fn a_policy_for_another_role_is_refused() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_other_role ENABLE ROW LEVEL SECURITY;
         DO $$ BEGIN CREATE ROLE p_rsg_someone_else NOLOGIN;
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;
         CREATE POLICY p_other ON tf_rsg_other_role TO p_rsg_someone_else USING ({});",
        table("tf_rsg_other_role"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_other_role", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_other_role", "no row level security policy"]);
}

#[tokio::test]
async fn a_table_its_reader_owns_without_force_is_refused() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_owned OWNER TO p_rsg_owner;",
        policy_table("tf_rsg_owned", &on(TOKEN_SETTING))
    );
    let Some(result) = check(&fixture, "tf_rsg_owned", Some(("p_rsg_owner", "NOBYPASSRLS"))).await
    else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_owned", "without FORCE ROW LEVEL SECURITY"]);
}

#[tokio::test]
async fn a_forced_table_its_reader_owns_passes() {
    let fixture = format!(
        "{} ALTER TABLE tf_rsg_owned_forced FORCE ROW LEVEL SECURITY;
         ALTER TABLE tf_rsg_owned_forced OWNER TO p_rsg_owner_forced;",
        policy_table("tf_rsg_owned_forced", &on(TOKEN_SETTING))
    );
    let Some(result) =
        check(&fixture, "tf_rsg_owned_forced", Some(("p_rsg_owner_forced", "NOBYPASSRLS"))).await
    else {
        return;
    };
    result.expect("a FORCEd table binds its owner");
}

#[tokio::test]
async fn a_bypassrls_reader_is_refused() {
    let fixture = policy_table("tf_rsg_unbound", &on(TOKEN_SETTING));
    let Some(result) = check(&fixture, "tf_rsg_unbound", Some(("p_rsg_bypass", "BYPASSRLS"))).await
    else {
        return;
    };
    assert_names(
        &refusal(result),
        &[
            "tf_rsg_unbound",
            "which has BYPASSRLS: row level security never applies",
        ],
    );
}

#[tokio::test]
async fn a_superuser_reader_of_a_policy_table_is_refused() {
    let fixture = policy_table("tf_rsg_superuser", &on(TOKEN_SETTING));
    let Some(result) = check(&fixture, "tf_rsg_superuser", None).await else {
        return;
    };
    assert_names(
        &refusal(result),
        &[
            "tf_rsg_superuser",
            "a superuser: row level security never applies",
        ],
    );
}

#[tokio::test]
async fn a_materialized_view_is_refused() {
    let fixture = format!(
        "{} DROP MATERIALIZED VIEW IF EXISTS tf_rsg_matview;
         CREATE MATERIALIZED VIEW tf_rsg_matview AS
             SELECT * FROM tb_rsg_matview_base WHERE {};",
        table("tb_rsg_matview_base"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_matview", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_matview", "neither a table nor a view"]);
}

#[tokio::test]
async fn a_missing_relation_is_refused() {
    let Some(result) = check("SELECT 1;", "tf_rsg_missing", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_missing", "does not exist"]);
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_invoker_view_over_a_filtered_table_passes() {
    let fixture = format!(
        "{} DROP VIEW IF EXISTS tf_rsg_invoker;
         CREATE VIEW tf_rsg_invoker WITH (security_invoker = true)
             AS SELECT * FROM tb_rsg_invoker_base;",
        policy_table("tb_rsg_invoker_base", &on(TOKEN_SETTING))
    );
    let Some(result) = check(&fixture, "tf_rsg_invoker", reader()).await else {
        return;
    };
    result.expect("an invoker view reads its table under the table's policies");
}

#[tokio::test]
async fn an_invoker_view_over_an_unfiltered_table_is_refused() {
    let fixture = format!(
        "{} DROP VIEW IF EXISTS tf_rsg_invoker_plain;
         CREATE VIEW tf_rsg_invoker_plain WITH (security_invoker = true)
             AS SELECT * FROM tb_rsg_invoker_plain_base;",
        table("tb_rsg_invoker_plain_base")
    );
    let Some(result) = check(&fixture, "tf_rsg_invoker_plain", reader()).await else {
        return;
    };
    assert_names(
        &refusal(result),
        &[
            "tf_rsg_invoker_plain",
            "tb_rsg_invoker_plain_base",
            "row level security is not enabled",
        ],
    );
}

#[tokio::test]
async fn a_definer_view_filtering_on_the_setting_passes() {
    let fixture = format!(
        "{} DROP VIEW IF EXISTS tf_rsg_definer;
         CREATE VIEW tf_rsg_definer AS SELECT * FROM tb_rsg_definer_base WHERE {};",
        table("tb_rsg_definer_base"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_definer", reader()).await else {
        return;
    };
    result.expect("a view filtering on the setting in its own WHERE filters every reader");
}

#[tokio::test]
async fn a_definer_view_filtering_on_the_setting_passes_for_a_superuser_reader() {
    let fixture = format!(
        "{} DROP VIEW IF EXISTS tf_rsg_definer_su;
         CREATE VIEW tf_rsg_definer_su AS SELECT * FROM tb_rsg_definer_su_base WHERE {};",
        table("tb_rsg_definer_su_base"),
        on(TOKEN_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_definer_su", None).await else {
        return;
    };
    result.expect("a WHERE clause binds every role, a superuser too");
}

#[tokio::test]
async fn a_definer_view_filtering_on_a_header_sourced_setting_is_refused() {
    let fixture = format!(
        "{} DROP VIEW IF EXISTS tf_rsg_definer_header;
         CREATE VIEW tf_rsg_definer_header AS SELECT * FROM tb_rsg_definer_header_base WHERE {};",
        table("tb_rsg_definer_header_base"),
        on(HEADER_SETTING)
    );
    let Some(result) = check(&fixture, "tf_rsg_definer_header", reader()).await else {
        return;
    };
    assert_names(&refusal(result), &["tf_rsg_definer_header", "does not filter on"]);
}

#[tokio::test]
async fn a_definer_view_without_a_filter_of_its_own_is_refused() {
    let fixture = format!(
        "{} DROP VIEW IF EXISTS tf_rsg_definer_bare;
         CREATE VIEW tf_rsg_definer_bare AS SELECT * FROM tb_rsg_definer_bare_base;",
        policy_table("tb_rsg_definer_bare_base", &on(TOKEN_SETTING))
    );
    let Some(result) = check(&fixture, "tf_rsg_definer_bare", reader()).await else {
        return;
    };
    assert_names(
        &refusal(result),
        &[
            "tf_rsg_definer_bare",
            "view is not `security_invoker` and does not filter on",
        ],
    );
}
