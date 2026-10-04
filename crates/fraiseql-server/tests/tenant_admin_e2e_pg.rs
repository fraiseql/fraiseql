//! Tenant administrators over HTTP (#1089).
//!
//! The deployment `admin_token` manages every tenant. A tenant admin token, minted by the
//! platform at `/api/admin-tokens`, manages one: on every tenant-aware admin router a request
//! naming another tenant is `403`, another tenant's row is `404` exactly like a missing one,
//! and naming no tenant means its own, never all. Every other admin router stays
//! platform-only.
//!
//! Each surface is driven on a booted server, against PostgreSQL, as the platform and as two
//! tenants.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test owns a scratch database.
// Every surface under test is mounted only with these features (the e2e leg enables both).
#![cfg(all(feature = "auth-saml", feature = "observers"))]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine
#![allow(clippy::doc_markdown)] // Reason: technical terms (IdP, SCIM, RBAC) throughout

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_server::{
    Server,
    server_config::{
        ServerConfig, hs256::Hs256Config, saml::SamlServerConfig, scim::ScimServerConfig,
    },
};
use fraiseql_test_support::try_database_url;
use reqwest::{Method, StatusCode};
use samael::idp::{CertificateParams, IdentityProvider, KeyType, Rsa};
use serde_json::{Value, json};
use sqlx::PgPool;

const ADMIN_TOKEN: &str = "tenant-admin-e2e-platform-token-32-chars";
const HS256_SECRET: &str = "tenant-admin-e2e-hs256-secret-32b";
const SECRET_ENV: &str = "FRAISEQL_TEST_TENANT_ADMIN_HS256_SECRET";
const TENANT_A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const TENANT_B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const IDP_ENTITY: &str = "https://idp.example.com";

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

async fn scratch_pool(admin_url: &str, db: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {db}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    PgPool::connect(&with_database(admin_url, db))
        .await
        .expect("connect to scratch database")
}

async fn drop_scratch(admin_url: &str, db: &str) {
    let Ok(admin) = PgPool::connect(admin_url).await else {
        return;
    };
    let _ = sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;
}

fn empty_schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION,
        "types": [],
        "queries": [],
        "mutations": [],
    }))
    .expect("compiled schema")
}

fn idp_metadata_xml() -> String {
    let idp = IdentityProvider::generate_new(KeyType::Rsa(Rsa::Rsa2048)).unwrap();
    let cert = idp
        .create_certificate(&CertificateParams {
            common_name:           IDP_ENTITY,
            issuer_name:           IDP_ENTITY,
            days_until_expiration: 3650,
        })
        .unwrap();
    let cert_b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(cert.der_data())
    };
    format!(
        r#"<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{IDP_ENTITY}">
  <IDPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <KeyDescriptor use="signing">
      <KeyInfo xmlns="http://www.w3.org/2000/09/xmldsig#">
        <X509Data><X509Certificate>{cert_b64}</X509Certificate></X509Data>
      </KeyInfo>
    </KeyDescriptor>
    <SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="https://idp.example.com/sso"/>
  </IDPSSODescriptor>
</EntityDescriptor>"#
    )
}

/// A booted server with every tenant-aware admin router mounted, plus one tenant admin
/// token per tenant, minted the way an operator would.
struct Rig {
    base:   String,
    client: reqwest::Client,
    url:    String,
    db:     String,
    a:      String,
    b:      String,
    stop:   tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<Result<(), fraiseql_server::ServerError>>,
}

impl Rig {
    async fn boot(test: &str, db: &str) -> Option<Self> {
        let Some(url) = try_database_url() else {
            eprintln!("SKIP {test}: DATABASE_URL not set");
            return None;
        };
        std::env::set_var(SECRET_ENV, HS256_SECRET);
        let pool = scratch_pool(&url, db).await;
        let scratch_url = with_database(&url, db);
        let config = ServerConfig {
            cors_enabled: false,
            admin_token: Some(ADMIN_TOKEN.to_string()),
            database_url: scratch_url.clone(),
            saml: Some(SamlServerConfig {
                idps: std::collections::HashMap::new(),
                store_enabled: true,
                refresh_interval_secs: 30,
                certificate_expiry_warning_days: 30,
                sp: None,
            }),
            scim: Some(ScimServerConfig {
                enabled:  true,
                base_url: "/scim/v2".to_string(),
            }),
            auth_hs256: Some(Hs256Config {
                secret_env: SECRET_ENV.to_string(),
                issuer:     Some("https://sp.example.com".to_string()),
                audience:   Some("fraiseql".to_string()),
            }),
            ..ServerConfig::default()
        };
        let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("adapter"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let server = Box::pin(Server::new(config, empty_schema(), adapter, Some(pool)))
            .await
            .expect("server boots");
        let (stop, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            server
                .serve_on_listener(listener, async {
                    let _ = rx.await;
                })
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let mut rig = Self {
            base: format!("http://127.0.0.1:{port}"),
            client: reqwest::Client::new(),
            url,
            db: db.to_string(),
            a: String::new(),
            b: String::new(),
            stop,
            handle,
        };
        rig.a = rig.mint(TENANT_A).await;
        rig.b = rig.mint(TENANT_B).await;
        Some(rig)
    }

    async fn mint(&self, tenant: &str) -> String {
        let (status, body) = self
            .call(
                ADMIN_TOKEN,
                Method::POST,
                "/api/admin-tokens",
                Some(json!({ "tenant_id": tenant })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "the platform mints a tenant admin token: {body}");
        assert_eq!(body["tenant_id"], tenant, "the minted token carries the tenant asked for");
        body["token"].as_str().unwrap().to_string()
    }

    async fn call(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req =
            self.client.request(method, format!("{}{path}", self.base)).bearer_auth(token);
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await.expect("request");
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    async fn shutdown(self) {
        let _ = self.stop.send(());
        let _ = self.handle.await;
        drop_scratch(&self.url, &self.db).await;
    }
}

/// Minting tenant administrators is the platform's job: a tenant admin token cannot mint,
/// list or revoke them, and a revoked one stops authenticating.
#[tokio::test]
async fn only_the_platform_manages_tenant_admin_tokens() {
    let Some(rig) = Rig::boot("admin_tokens", "fraiseql_tenant_admin_tokens").await else {
        return;
    };
    for (method, body) in [
        (Method::POST, Some(json!({ "tenant_id": TENANT_A }))),
        (Method::GET, None),
    ] {
        let (status, _) = rig.call(&rig.a, method.clone(), "/api/admin-tokens", body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} /api/admin-tokens is platform-only");
    }
    let (status, body) = rig
        .call(
            ADMIN_TOKEN,
            Method::POST,
            "/api/admin-tokens",
            Some(json!({ "description": "x" })),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a tenant admin token without a tenant would be a second platform credential: {body}"
    );

    let (_, listed) = rig.call(ADMIN_TOKEN, Method::GET, "/api/admin-tokens", None).await;
    let a_id = listed["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tenant_id"] == TENANT_A)
        .map(|t| t["id"].as_str().unwrap().to_string())
        .unwrap();
    let (status, _) = rig
        .call(ADMIN_TOKEN, Method::DELETE, &format!("/api/admin-tokens/{a_id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "the platform revokes a tenant admin token");
    let (status, _) = rig.call(&rig.a, Method::GET, "/api/roles", None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a revoked tenant admin token stops authenticating"
    );

    rig.shutdown().await;
}

/// The routers that do not take tenant administrators answer a tenant admin token as the
/// wrong token it is for them.
#[tokio::test]
async fn platform_only_routers_refuse_a_tenant_admin_token() {
    let Some(rig) = Rig::boot("platform_only", "fraiseql_tenant_admin_platform_only").await else {
        return;
    };
    let (status, _) =
        rig.call(&rig.a, Method::POST, "/api/identity/flush-all", Some(json!({}))).await;
    // Mounted only with an enrichment resolver; when absent the route does not exist.
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
        "identity flush must not accept a tenant admin token: {status}"
    );
    let (status, _) = rig.call(&rig.a, Method::GET, "/api/admin-tokens", None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant admin token is a wrong token on a platform-only route"
    );
    rig.shutdown().await;
}

#[tokio::test]
async fn a_tenant_administrator_manages_only_its_own_saml_idps() {
    let Some(rig) = Rig::boot("saml_idps", "fraiseql_tenant_admin_saml").await else {
        return;
    };
    let idp = |name: &str, tenant: Option<&str>| {
        let mut body = json!({
            "idp_name":     name,
            "sp_entity_id": "https://sp.example.com/metadata",
            "acs_url":      "https://sp.example.com/auth/saml/acs",
            "metadata_xml": idp_metadata_xml(),
        });
        if let Some(t) = tenant {
            body["tenant_id"] = json!(t);
        }
        body
    };

    // Naming no tenant creates in its own.
    let (status, created) = rig
        .call(&rig.a, Method::POST, "/api/saml/idps", Some(idp("a-okta", None)))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["tenant_id"], TENANT_A, "naming no tenant means its own, never global");
    // Naming another is refused.
    let (status, _) = rig
        .call(&rig.a, Method::POST, "/api/saml/idps", Some(idp("x-okta", Some(TENANT_B))))
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator cannot create an IdP in another tenant"
    );
    // The other tenant's IdP and a platform IdP exist…
    let (status, _) = rig
        .call(&rig.b, Method::POST, "/api/saml/idps", Some(idp("b-okta", None)))
        .await;
    assert_eq!(status, StatusCode::CREATED, "tenant B creates its own IdP");
    let (status, _) = rig
        .call(ADMIN_TOKEN, Method::POST, "/api/saml/idps", Some(idp("global-okta", None)))
        .await;
    assert_eq!(status, StatusCode::CREATED, "the platform creates a global IdP");

    // …and are invisible to tenant A.
    let (_, listed) = rig.call(&rig.a, Method::GET, "/api/saml/idps", None).await;
    let names: Vec<&str> = listed["idps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["idp_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a-okta"], "a tenant lists only its own IdPs");
    let (status, _) = rig
        .call(&rig.a, Method::GET, &format!("/api/saml/idps?tenant_id={TENANT_B}"), None)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator cannot list another tenant's IdPs"
    );
    for name in ["b-okta", "global-okta"] {
        let path = format!("/api/saml/idps/{name}");
        let (status, _) = rig.call(&rig.a, Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET {name}");
        let update = json!({
            "sp_entity_id": "https://sp.example.com/metadata",
            "acs_url":      "https://sp.example.com/auth/saml/acs",
            "metadata_xml": idp_metadata_xml(),
        });
        let (status, _) = rig.call(&rig.a, Method::PUT, &path, Some(update)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "PUT {name}");
        let (status, _) = rig.call(&rig.a, Method::DELETE, &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "DELETE {name}");
    }
    // Untouched, as the platform sees them.
    let (_, all) = rig.call(ADMIN_TOKEN, Method::GET, "/api/saml/idps", None).await;
    assert_eq!(all["total"], 3, "the platform still sees every IdP: {all}");

    rig.shutdown().await;
}

#[tokio::test]
async fn a_tenant_administrator_manages_only_its_own_scim_tokens() {
    let Some(rig) = Rig::boot("scim_tokens", "fraiseql_tenant_admin_scim").await else {
        return;
    };
    let (status, mine) = rig
        .call(&rig.a, Method::POST, "/api/scim/tokens", Some(json!({ "idp_name": "a-okta" })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{mine}");
    assert_eq!(mine["tenant_id"], TENANT_A, "a tenant's SCIM token is confined to that tenant");
    let (status, _) = rig
        .call(
            &rig.a,
            Method::POST,
            "/api/scim/tokens",
            Some(json!({ "idp_name": "x", "tenant_id": TENANT_B })),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator cannot mint a SCIM token for another tenant"
    );

    let (_, theirs) = rig
        .call(&rig.b, Method::POST, "/api/scim/tokens", Some(json!({ "idp_name": "b-okta" })))
        .await;
    let (_, platform) = rig
        .call(
            ADMIN_TOKEN,
            Method::POST,
            "/api/scim/tokens",
            Some(json!({ "idp_name": "global" })),
        )
        .await;

    let (_, listed) = rig.call(&rig.a, Method::GET, "/api/scim/tokens", None).await;
    let ids: Vec<&str> = listed["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![mine["id"].as_str().unwrap()], "a tenant lists only its own: {listed}");
    for other in [&theirs, &platform] {
        let path = format!("/api/scim/tokens/{}", other["id"].as_str().unwrap());
        let (status, _) = rig.call(&rig.a, Method::DELETE, &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "another tenant's token is not revocable");
    }
    let (_, all) = rig.call(ADMIN_TOKEN, Method::GET, "/api/scim/tokens", None).await;
    assert_eq!(all["total"], 3, "nothing was revoked: {all}");

    rig.shutdown().await;
}

#[tokio::test]
async fn a_tenant_administrator_manages_only_its_own_roles_and_assignments() {
    let Some(rig) = Rig::boot("rbac", "fraiseql_tenant_admin_rbac").await else {
        return;
    };
    let role = |name: &str| json!({ "name": name, "permissions": [] });

    let (status, mine) = rig.call(&rig.a, Method::POST, "/api/roles", Some(role("a-editor"))).await;
    assert_eq!(status, StatusCode::CREATED, "{mine}");
    assert_eq!(mine["tenant_id"], TENANT_A, "naming no tenant means its own");
    let mut foreign = role("x");
    foreign["tenant_id"] = json!(TENANT_B);
    let (status, _) = rig.call(&rig.a, Method::POST, "/api/roles", Some(foreign)).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator cannot create a role in another tenant"
    );
    let (_, theirs) = rig.call(&rig.b, Method::POST, "/api/roles", Some(role("b-editor"))).await;
    let (_, global) = rig.call(ADMIN_TOKEN, Method::POST, "/api/roles", Some(role("global"))).await;

    let (_, listed) = rig.call(&rig.a, Method::GET, "/api/roles", None).await;
    let names: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a-editor"], "a tenant lists only its own roles: {listed}");
    for other in [&theirs, &global] {
        let path = format!("/api/roles/{}", other["id"].as_str().unwrap());
        let (status, _) = rig.call(&rig.a, Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET another tenant's role");
        let (status, _) = rig.call(&rig.a, Method::PUT, &path, Some(role("hijacked"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "PUT another tenant's role");
        let (status, _) = rig.call(&rig.a, Method::DELETE, &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "DELETE another tenant's role");
        let assign = json!({ "user_id": "u-1", "role_id": other["id"] });
        let (status, _) = rig.call(&rig.a, Method::POST, "/api/user-roles", Some(assign)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "assigning another tenant's role");
    }

    // Assignments: its own role, in its own tenant.
    let assign = json!({ "user_id": "u-1", "role_id": mine["id"] });
    let (status, assigned) = rig.call(&rig.a, Method::POST, "/api/user-roles", Some(assign)).await;
    assert_eq!(status, StatusCode::CREATED, "{assigned}");
    assert_eq!(
        assigned["tenant_id"], TENANT_A,
        "the assignment lands in the administrator's tenant"
    );
    // Tenant B's assignment of its own role to the same user is invisible to A.
    let assign = json!({ "user_id": "u-1", "role_id": theirs["id"] });
    let (status, _) = rig.call(&rig.b, Method::POST, "/api/user-roles", Some(assign)).await;
    assert_eq!(status, StatusCode::CREATED, "tenant B assigns its own role");
    let (_, listed) = rig.call(&rig.a, Method::GET, "/api/user-roles?user_id=u-1", None).await;
    assert_eq!(listed["total"], 1, "a tenant lists only its own assignments: {listed}");
    let path = format!("/api/user-roles/u-1/{}", theirs["id"].as_str().unwrap());
    let (status, _) = rig.call(&rig.a, Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "revoking another tenant's assignment");

    // The permission catalogue is shared: readable, not writable, by a tenant.
    let (status, _) = rig.call(&rig.a, Method::GET, "/api/permissions", None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a tenant administrator may read the permission catalogue"
    );
    let perm = json!({ "resource": "doc", "action": "read" });
    let (status, _) = rig.call(&rig.a, Method::POST, "/api/permissions", Some(perm.clone())).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator may not add to the permission catalogue"
    );
    let (status, created) =
        rig.call(ADMIN_TOKEN, Method::POST, "/api/permissions", Some(perm)).await;
    assert_eq!(status, StatusCode::CREATED, "the platform adds to the permission catalogue");
    let path = format!("/api/permissions/{}", created["id"].as_str().unwrap());
    let (status, _) = rig.call(&rig.a, Method::DELETE, &path, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator may not delete from the permission catalogue"
    );

    // Audit is scoped the same way.
    let (status, _) = rig
        .call(
            &rig.a,
            Method::GET,
            &format!("/api/audit/permissions?tenant_id={TENANT_B}"),
            None,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tenant administrator cannot read another tenant's audit"
    );

    rig.shutdown().await;
}

/// The platform keeps naming the tenant, but a tenant's role can be held only in that tenant.
#[tokio::test]
async fn a_tenants_role_cannot_be_assigned_in_another_tenant() {
    let Some(rig) = Rig::boot("rbac_mismatch", "fraiseql_tenant_admin_rbac_mismatch").await else {
        return;
    };
    let (_, role) = rig
        .call(
            ADMIN_TOKEN,
            Method::POST,
            "/api/roles",
            Some(json!({ "name": "a-role", "permissions": [], "tenant_id": TENANT_A })),
        )
        .await;
    let (status, body) = rig
        .call(
            ADMIN_TOKEN,
            Method::POST,
            "/api/user-roles",
            Some(json!({ "user_id": "u-1", "role_id": role["id"], "tenant_id": TENANT_B })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "tenant_mismatch", "the refusal names the rule");

    let (_, global) = rig
        .call(
            ADMIN_TOKEN,
            Method::POST,
            "/api/roles",
            Some(json!({ "name": "g", "permissions": [] })),
        )
        .await;
    let (status, _) = rig
        .call(
            ADMIN_TOKEN,
            Method::POST,
            "/api/user-roles",
            Some(json!({ "user_id": "u-1", "role_id": global["id"], "tenant_id": TENANT_B })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "a global role may be held in any tenant");

    rig.shutdown().await;
}

/// The document this suite serves loads, checked with no database: every other test here
/// skips before building it when `DATABASE_URL` is absent, so a refusal of it would
/// otherwise read as a pass.
#[test]
fn the_document_loads_without_a_database() {
    empty_schema();
}
