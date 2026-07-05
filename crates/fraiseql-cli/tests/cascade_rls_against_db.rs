//! Live 2-tenant RLS conformance test for the shipped cascade builders.
//!
//! Ratified gate decision (2026-07-05): cascade row-visibility is a documented
//! boundary — the runtime enforces field-level authz on each cascade entity but
//! does NOT re-check row visibility; that comes from RLS, because
//! `fraiseql.cascade_entity` reads each entity through its RLS-protected view. This
//! test pins the paved path end-to-end: two tenants, a cascade assembled from the
//! view as tenant A, asserting tenant B's rows never ride in tenant A's cascade.
//!
//! It also locks the load-bearing requirement the boundary depends on: the view
//! MUST be `security_invoker = true`. A *default* view runs with the view owner's
//! privileges and silently bypasses the caller's RLS — this test proves such a view
//! leaks, so a regression that drops `security_invoker` fails here.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free leg.
//!
//! **Execution engine:** PostgreSQL
//! **Infrastructure:** `DATABASE_URL` (a superuser, to `SET ROLE` + create a role)
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::process::Command;

use tokio_postgres::NoTls;

const TENANT_A: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const TENANT_B: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const POST_A: &str = "11111111-1111-1111-1111-111111111111";
const POST_B: &str = "22222222-2222-2222-2222-222222222222";

#[tokio::test]
async fn cascade_rls_conformance_isolates_tenants() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping cascade RLS conformance test: DATABASE_URL not set");
        return;
    };

    // Install the cascade builders via the real installer.
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["setup", "--database", &url])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fraiseql setup must install the cascade builders; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });

    // Fresh 2-tenant fixture: a table with an RLS policy scoped to a session GUC, a
    // `security_invoker` view (the paved path) and a DEFAULT view (the anti-pattern),
    // and a NOBYPASSRLS role so the policy binds.
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS crt CASCADE;
             CREATE SCHEMA crt;
             CREATE TABLE crt.tb_post (id UUID PRIMARY KEY, tenant_id UUID, title TEXT);
             CREATE VIEW crt.v_post WITH (security_invoker = true) AS
                 SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM crt.tb_post;
             CREATE VIEW crt.v_post_leaky AS
                 SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM crt.tb_post;
             ALTER TABLE crt.tb_post ENABLE ROW LEVEL SECURITY;
             CREATE POLICY tenant_isolation ON crt.tb_post
                 USING (tenant_id = current_setting('fraiseql.tenant_id', true)::uuid);
             INSERT INTO crt.tb_post (id, tenant_id, title) VALUES
                 ('{POST_A}', '{TENANT_A}', 'Tenant A post'),
                 ('{POST_B}', '{TENANT_B}', 'Tenant B post');
             DO $$ BEGIN
                 IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='crt_tenant') THEN
                     CREATE ROLE crt_tenant NOSUPERUSER NOBYPASSRLS;
                 END IF;
             END $$;
             GRANT USAGE ON SCHEMA crt, fraiseql TO crt_tenant;
             GRANT SELECT ON crt.tb_post, crt.v_post, crt.v_post_leaky TO crt_tenant;
             GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA fraiseql TO crt_tenant;"
        ))
        .await
        .unwrap();

    // Act as tenant A (NOBYPASSRLS role + tenant GUC), with `crt` on the search path
    // so `cascade_entity`'s unqualified `%I` view name resolves there.
    client
        .batch_execute(&format!(
            "SET ROLE crt_tenant; SET search_path = crt, public; \
             SET fraiseql.tenant_id = '{TENANT_A}';"
        ))
        .await
        .unwrap();

    // Tenant A sees its own post through the security_invoker view.
    let own: serde_json::Value = client
        .query_one(
            &format!(
                "SELECT fraiseql.cascade_entity('Post', '{POST_A}', 'UPDATED', 'v_post') AS c"
            ),
            &[],
        )
        .await
        .unwrap()
        .get("c");
    assert_eq!(own["__typename"], "Post", "tenant A must see its own cascade entity");

    // Tenant B's post is invisible through the security_invoker view → NULL, so it
    // never rides in tenant A's cascade. THE conformance guarantee.
    let cross: Option<serde_json::Value> = client
        .query_one(
            &format!(
                "SELECT fraiseql.cascade_entity('Post', '{POST_B}', 'UPDATED', 'v_post') AS c"
            ),
            &[],
        )
        .await
        .unwrap()
        .get("c");
    assert!(
        cross.is_none(),
        "tenant B's row MUST NOT ride in tenant A's cascade (security_invoker view): {cross:?}"
    );

    // And `build_cascade` drops the invisible entry, so the assembled cascade holds
    // only tenant A's entity.
    let cascade: serde_json::Value = client
        .query_one(
            &format!(
                "SELECT fraiseql.build_cascade(p_updated := jsonb_build_array(
                     fraiseql.cascade_entity('Post', '{POST_A}', 'UPDATED', 'v_post'),
                     fraiseql.cascade_entity('Post', '{POST_B}', 'UPDATED', 'v_post')
                 )) AS c"
            ),
            &[],
        )
        .await
        .unwrap()
        .get("c");
    let updated = cascade["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 1, "only the visible (tenant A) entity survives: {cascade}");
    assert_eq!(updated[0]["id"], POST_A);

    // Anti-pattern lock: a DEFAULT (non-security_invoker) view runs as its owner and
    // LEAKS tenant B — proving `security_invoker` is load-bearing, not optional.
    let leaked: Option<serde_json::Value> = client
        .query_one(
            &format!(
                "SELECT fraiseql.cascade_entity('Post', '{POST_B}', 'UPDATED', 'v_post_leaky') AS c"
            ),
            &[],
        )
        .await
        .unwrap()
        .get("c");
    assert!(
        leaked.is_some(),
        "a DEFAULT view is expected to leak cross-tenant rows — if this is None, the \
         view convention changed and the security_invoker requirement can be relaxed"
    );

    // Cleanup: drop role membership objects then the role + schema.
    client
        .batch_execute(
            "RESET ROLE; RESET search_path;
             DROP OWNED BY crt_tenant; DROP ROLE IF EXISTS crt_tenant;
             DROP SCHEMA IF EXISTS crt CASCADE;",
        )
        .await
        .unwrap();
}
